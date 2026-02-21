use std::path::Path;

use db::models::{
    project_repo::ProjectRepo,
    spec_sheet::{CreateSpecSheet, SpecSheet},
    task::{Task, TaskStatus},
};
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use super::{decomposer, spec_generator};

#[derive(Debug, Error)]
pub enum AutoPlannerError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Failed to run plan generation: {0}")]
    ExecutionFailed(String),
    #[error("No repos found for project {0}")]
    NoRepos(Uuid),
}

/// Recover tasks whose plan_status is stuck at "generating" (e.g. from a server restart)
/// by resetting them to "pending" and re-spawning plan generation.
/// Also ensures these tasks have PlanGenerating status.
pub async fn recover_stuck_plans(pool: &SqlitePool) {
    match Task::reset_stuck_generating_plans(pool).await {
        Ok(tasks) if !tasks.is_empty() => {
            tracing::info!(
                "Recovering {} task(s) with stuck 'generating' plan_status",
                tasks.len()
            );
            for task in tasks {
                // Ensure task is in PlanGenerating status
                if task.status != TaskStatus::PlanGenerating {
                    let _ = Task::update_status(pool, task.id, TaskStatus::PlanGenerating).await;
                }
                spawn_auto_plan(
                    pool.clone(),
                    task.id,
                    task.project_id,
                    task.title,
                    task.description,
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Failed to recover stuck generating plans: {}", e);
        }
    }
}

/// Build a prompt that instructs Claude to analyze the codebase and produce a step-by-step
/// implementation plan from a task's title and description.
pub fn build_auto_plan_prompt(title: &str, description: Option<&str>) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Implementation Plan: {}\n\n", title));

    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            prompt.push_str("## Task Description\n");
            prompt.push_str(desc);
            prompt.push_str("\n\n");
        }
    }

    prompt.push_str("## Instructions\n\n");
    prompt.push_str("Analyze the codebase and produce a detailed, step-by-step implementation plan for this task.\n\n");
    prompt.push_str("Your plan should:\n");
    prompt.push_str("1. Identify all files that need to be created or modified\n");
    prompt.push_str("2. Outline the order of operations (what to do first, second, etc.)\n");
    prompt.push_str("3. Include specific code changes or patterns to follow\n");
    prompt.push_str("4. Note any potential risks, edge cases, or dependencies\n");
    prompt.push_str("5. Reference existing code patterns in the codebase where applicable\n\n");
    prompt.push_str("Format the plan in clear markdown with numbered steps and sub-steps.\n");

    prompt
}

/// Shell out to `claude --print -p <prompt>` to generate a plan.
/// This is a blocking call — use `spawn_blocking` from async context.
pub fn run_plan_generation(prompt: &str, working_dir: &Path) -> Result<String, AutoPlannerError> {
    let output = std::process::Command::new("claude")
        .args(["--print", "-p", prompt])
        .current_dir(working_dir)
        .output()
        .map_err(|e| AutoPlannerError::ExecutionFailed(format!("Failed to spawn claude: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AutoPlannerError::ExecutionFailed(format!(
            "claude exited with status {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

/// Spawn a background task that generates a plan for the given task.
///
/// 1. Sets plan_status = "generating"
/// 2. Gets working dir from project repos
/// 3. Runs `claude --print` via spawn_blocking
/// 4. On success: stores plan text, transitions to Plan status
/// 5. On failure: sets plan_status = "failed", leaves task in Backlog
pub fn spawn_auto_plan(
    pool: SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: String,
    description: Option<String>,
) {
    tokio::spawn(async move {
        // Set plan_status = "generating" and task status to PlanGenerating
        if let Err(e) = Task::update_plan_status(&pool, task_id, "generating").await {
            tracing::error!(
                "Failed to set plan_status to generating for task {}: {}",
                task_id,
                e
            );
            return;
        }
        if let Err(e) = Task::update_status(&pool, task_id, TaskStatus::PlanGenerating).await {
            tracing::error!(
                "Failed to set task {} to PlanGenerating status: {}",
                task_id,
                e
            );
        }

        // Get working directory from project repos
        let working_dir = match ProjectRepo::find_repos_for_project(&pool, project_id).await {
            Ok(repos) if !repos.is_empty() => repos[0].path.clone(),
            Ok(_) => {
                tracing::error!(
                    "No repos found for project {} while generating plan for task {}",
                    project_id,
                    task_id
                );
                let _ = Task::update_plan(
                    &pool,
                    task_id,
                    "No repositories configured for this project.",
                    "failed",
                )
                .await;
                return;
            }
            Err(e) => {
                tracing::error!("Failed to find repos for project {}: {}", project_id, e);
                let _ = Task::update_plan(
                    &pool,
                    task_id,
                    &format!("Failed to find repos: {e}"),
                    "failed",
                )
                .await;
                return;
            }
        };

        let prompt = build_auto_plan_prompt(&title, description.as_deref());
        let working_path = std::path::PathBuf::from(&working_dir);

        // Run claude --print in a blocking thread
        let result =
            tokio::task::spawn_blocking(move || run_plan_generation(&prompt, &working_path)).await;

        match result {
            Ok(Ok(plan_text)) => {
                // Success: store plan
                if let Err(e) = Task::update_plan(&pool, task_id, &plan_text, "completed").await {
                    tracing::error!("Failed to store plan for task {}: {}", task_id, e);
                    return;
                }

                // Auto-generate spec sheet and decompose into child tasks
                // so the task is prepared for Ralph execution.
                // Failures here are non-fatal — the task still transitions to Ready.
                auto_prepare_for_ralph(
                    &pool,
                    task_id,
                    project_id,
                    &title,
                    description.as_deref(),
                    &plan_text,
                    &working_dir,
                )
                .await;

                if let Err(e) = Task::update_status(&pool, task_id, TaskStatus::Ready).await {
                    tracing::error!(
                        "Failed to transition task {} to Ready status: {}",
                        task_id,
                        e
                    );
                }
                tracing::info!("Auto-plan generated successfully for task {}", task_id);
            }
            Ok(Err(e)) => {
                // Claude execution failed
                tracing::error!("Plan generation failed for task {}: {}", task_id, e);
                let _ = Task::update_plan(
                    &pool,
                    task_id,
                    &format!("Plan generation failed: {e}"),
                    "failed",
                )
                .await;
            }
            Err(e) => {
                // spawn_blocking panicked
                tracing::error!("Plan generation task panicked for task {}: {}", task_id, e);
                let _ =
                    Task::update_plan(&pool, task_id, "Plan generation task panicked", "failed")
                        .await;
            }
        }
    });
}

/// After plan generation succeeds, automatically generate a spec sheet and decompose the task
/// into child tasks so it's prepared for Ralph execution. Failures are logged but non-fatal.
async fn auto_prepare_for_ralph(
    pool: &SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: &str,
    description: Option<&str>,
    plan_text: &str,
    working_dir: &Path,
) {
    // Step 1: Generate spec sheet via Claude
    let spec_prompt =
        spec_generator::build_spec_generation_prompt(title, description, Some(plan_text));
    let spec_working_path = working_dir.to_path_buf();

    let spec_result = tokio::task::spawn_blocking(move || {
        spec_generator::run_spec_generation(&spec_prompt, &spec_working_path)
    })
    .await;

    let spec = match spec_result {
        Ok(Ok(raw_output)) => match spec_generator::parse_spec_output(&raw_output) {
            Ok(spec) => spec,
            Err(e) => {
                tracing::warn!(
                    "Failed to parse auto-generated spec for task {}: {}",
                    task_id,
                    e
                );
                return;
            }
        },
        Ok(Err(e)) => {
            tracing::warn!("Auto spec generation failed for task {}: {}", task_id, e);
            return;
        }
        Err(e) => {
            tracing::warn!(
                "Auto spec generation task panicked for task {}: {}",
                task_id,
                e
            );
            return;
        }
    };

    // Step 2: Store the spec sheet
    let spec_data = CreateSpecSheet {
        overview: Some(spec.overview),
        requirements: Some(serde_json::to_string(&spec.requirements).unwrap_or_default()),
        acceptance_criteria: Some(
            serde_json::to_string(&spec.acceptance_criteria).unwrap_or_default(),
        ),
        constraints: Some(serde_json::to_string(&spec.constraints).unwrap_or_default()),
        tech_notes: Some(spec.tech_notes),
    };

    let stored_spec = match SpecSheet::upsert(pool, task_id, &spec_data).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "Failed to store auto-generated spec for task {}: {}",
                task_id,
                e
            );
            return;
        }
    };

    tracing::info!("Auto-generated spec sheet for task {}", task_id);

    // Step 3: Decompose into child tasks
    let decompose_prompt = decomposer::generate_decomposition_prompt(&stored_spec, title);
    let decompose_working_path = working_dir.to_path_buf();

    let decompose_result = tokio::task::spawn_blocking(move || {
        decomposer::run_decomposition(&decompose_prompt, &decompose_working_path)
    })
    .await;

    let decomposition = match decompose_result {
        Ok(Ok(raw_output)) => match decomposer::parse_decomposition_output(&raw_output) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(
                    "Failed to parse auto-decomposition for task {}: {}",
                    task_id,
                    e
                );
                return;
            }
        },
        Ok(Err(e)) => {
            tracing::warn!("Auto decomposition failed for task {}: {}", task_id, e);
            return;
        }
        Err(e) => {
            tracing::warn!(
                "Auto decomposition task panicked for task {}: {}",
                task_id,
                e
            );
            return;
        }
    };

    // Step 4: Create child tasks
    match decomposer::create_child_tasks(pool, project_id, task_id, &decomposition).await {
        Ok(children) => {
            tracing::info!(
                "Auto-decomposed task {} into {} child tasks",
                task_id,
                children.len()
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create child tasks for auto-decomposition of task {}: {}",
                task_id,
                e
            );
        }
    }
}
