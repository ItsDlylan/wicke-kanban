use std::{collections::HashMap, path::Path};

use db::models::{
    pending_approval::PendingApprovalRecord,
    project_repo::ProjectRepo,
    spec_sheet::{CreateSpecSheet, SpecSheet},
    task::{Task, TaskStatus},
};
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use super::{container::ContainerService, decomposer, spec_generator};

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
///
/// Tasks that have pending approvals (AskUserQuestion waiting for user input) are SKIPPED —
/// those will be handled by the pending approval recovery flow instead.
pub async fn recover_stuck_plans<C: ContainerService + Clone + Send + Sync + 'static>(
    pool: &SqlitePool,
    container: C,
) {
    // Collect task IDs that have pending approvals — these should NOT be re-planned
    let tasks_with_pending_approvals: std::collections::HashSet<Uuid> =
        match PendingApprovalRecord::find_pending_orphaned(pool).await {
            Ok(approvals) => approvals.iter().map(|a| a.task_id).collect(),
            Err(e) => {
                tracing::warn!(
                    "Failed to check for pending approvals during recovery: {}",
                    e
                );
                std::collections::HashSet::new()
            }
        };

    match Task::reset_stuck_generating_plans(pool).await {
        Ok(tasks) if !tasks.is_empty() => {
            tracing::info!(
                "Recovering {} task(s) with stuck 'generating' plan_status",
                tasks.len()
            );
            for task in tasks {
                // Skip tasks with pending approvals — let the user answer first
                if tasks_with_pending_approvals.contains(&task.id) {
                    tracing::info!(
                        "Skipping recovery of task {} — has pending approval awaiting user input",
                        task.id
                    );
                    continue;
                }

                // Ensure task is in PlanGenerating status
                if task.status != TaskStatus::PlanGenerating {
                    let _ = Task::update_status(pool, task.id, TaskStatus::PlanGenerating).await;
                }
                spawn_auto_plan(
                    container.clone(),
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

/// Recover tasks whose plan completed but the server restarted before auto_prepare_for_ralph
/// could finish (spec generation, decomposition) and transition to Ready.
/// These tasks have status = PlanGenerating + plan_status = "completed".
pub async fn recover_stuck_plan_completed(pool: &SqlitePool) {
    match Task::find_stuck_plan_completed(pool).await {
        Ok(tasks) if !tasks.is_empty() => {
            tracing::info!(
                "Recovering {} task(s) with completed plan but stuck in PlanGenerating status",
                tasks.len()
            );
            for task in tasks {
                let plan_text = match &task.plan {
                    Some(p) => p.clone(),
                    None => {
                        tracing::warn!(
                            "Task {} has plan_status=completed but no plan text, transitioning to Ready",
                            task.id
                        );
                        let _ = Task::update_status(pool, task.id, TaskStatus::Ready).await;
                        continue;
                    }
                };
                spawn_post_plan_recovery(
                    pool.clone(),
                    task.id,
                    task.project_id,
                    task.title,
                    task.description,
                    plan_text,
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Failed to recover stuck plan-completed tasks: {}", e);
        }
    }
}

/// Re-run auto_prepare_for_ralph for a task whose plan completed but got interrupted.
/// Skips spec/decompose steps if they already succeeded (idempotent).
fn spawn_post_plan_recovery(
    pool: SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: String,
    description: Option<String>,
    plan_text: String,
) {
    tokio::spawn(async move {
        // Get working directory from project repos
        let working_dir = match ProjectRepo::find_repos_for_project(&pool, project_id).await {
            Ok(repos) if !repos.is_empty() => repos[0].path.clone(),
            Ok(_) => {
                tracing::warn!(
                    "No repos found for project {} during recovery of task {}, transitioning to Ready",
                    project_id,
                    task_id
                );
                let _ = Task::update_status(&pool, task_id, TaskStatus::Ready).await;
                return;
            }
            Err(e) => {
                tracing::error!(
                    "Failed to find repos during recovery for task {}: {}",
                    task_id,
                    e
                );
                let _ = Task::update_status(&pool, task_id, TaskStatus::Ready).await;
                return;
            }
        };

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
                "Failed to transition recovered task {} to Ready status: {}",
                task_id,
                e
            );
        }
        tracing::info!(
            "Successfully recovered task {} from stuck PlanGenerating state",
            task_id
        );
    });
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

/// Spawn a background task that generates a plan for the given task
/// using the interactive Claude executor (same pipeline as task execution).
///
/// 1. Sets plan_status = "generating" and task status to PlanGenerating
/// 2. Gets working dir from project repos
/// 3. Starts a plan workspace with the interactive executor
/// 4. The exit monitor handles plan extraction and post-plan steps
pub fn spawn_auto_plan<C: ContainerService + Clone + Send + Sync + 'static>(
    container: C,
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
        let repo_path = match ProjectRepo::find_repos_for_project(&pool, project_id).await {
            Ok(repos) if !repos.is_empty() => repos[0].path.to_string_lossy().to_string(),
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

        // Get the task record for start_plan_workspace
        let task = match Task::find_by_id(&pool, task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                tracing::error!("Task {} not found for auto-plan", task_id);
                return;
            }
            Err(e) => {
                tracing::error!("Failed to find task {}: {}", task_id, e);
                return;
            }
        };

        let prompt = build_auto_plan_prompt(&title, description.as_deref());

        // Start the interactive plan workspace — the exit monitor handles the rest
        if let Err(e) = container
            .start_plan_workspace(&task, &repo_path, prompt)
            .await
        {
            tracing::error!("Failed to start plan workspace for task {}: {}", task_id, e);
            let _ = Task::update_plan(
                &pool,
                task_id,
                &format!("Failed to start plan generation: {e}"),
                "failed",
            )
            .await;
        }
    });
}

/// After plan generation succeeds, automatically generate a spec sheet and decompose the task
/// into child tasks so it's prepared for Ralph execution. Failures are logged but non-fatal.
/// This function is idempotent — it skips steps that have already completed (e.g. on recovery).
/// Returns `true` if spec generation and decomposition succeeded, `false` on failure.
pub async fn auto_prepare_for_ralph(
    pool: &SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: &str,
    description: Option<&str>,
    plan_text: &str,
    working_dir: &Path,
) -> bool {
    // Step 1: Check if spec already exists (idempotent on recovery)
    let stored_spec = match SpecSheet::find_by_task_id(pool, task_id).await {
        Ok(Some(existing)) => {
            tracing::info!(
                "Spec sheet already exists for task {}, skipping generation",
                task_id
            );
            existing
        }
        _ => {
            // Generate spec sheet via Claude
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
                        return false;
                    }
                },
                Ok(Err(e)) => {
                    tracing::warn!("Auto spec generation failed for task {}: {}", task_id, e);
                    return false;
                }
                Err(e) => {
                    tracing::warn!(
                        "Auto spec generation task panicked for task {}: {}",
                        task_id,
                        e
                    );
                    return false;
                }
            };

            // Store the spec sheet
            let spec_data = CreateSpecSheet {
                overview: Some(spec.overview),
                requirements: Some(serde_json::to_string(&spec.requirements).unwrap_or_default()),
                acceptance_criteria: Some(
                    serde_json::to_string(&spec.acceptance_criteria).unwrap_or_default(),
                ),
                constraints: Some(serde_json::to_string(&spec.constraints).unwrap_or_default()),
                tech_notes: Some(spec.tech_notes),
            };

            match SpecSheet::upsert(pool, task_id, &spec_data).await {
                Ok(s) => {
                    tracing::info!("Auto-generated spec sheet for task {}", task_id);
                    s
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to store auto-generated spec for task {}: {}",
                        task_id,
                        e
                    );
                    return false;
                }
            }
        }
    };

    // Step 2: Check if child tasks already exist (idempotent on recovery)
    let existing_children = Task::find_by_parent_task_id(pool, task_id).await;
    if let Ok(ref children) = existing_children {
        if !children.is_empty() {
            tracing::info!(
                "Task {} already has {} child tasks, skipping decomposition",
                task_id,
                children.len()
            );
            return true;
        }
    }

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
                // Spec was generated but decomposition failed — still consider success
                return true;
            }
        },
        Ok(Err(e)) => {
            tracing::warn!("Auto decomposition failed for task {}: {}", task_id, e);
            return true;
        }
        Err(e) => {
            tracing::warn!(
                "Auto decomposition task panicked for task {}: {}",
                task_id,
                e
            );
            return true;
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

    true
}

/// Recover pending approvals that were orphaned by a server restart.
/// - Times out approvals past their deadline
/// - Keeps valid orphaned approvals as 'pending' in DB for frontend to pick up
pub async fn recover_pending_approvals(pool: &SqlitePool) {
    // First, time out any expired approvals
    match PendingApprovalRecord::timeout_expired(pool).await {
        Ok(count) if count > 0 => {
            tracing::info!("Timed out {} expired pending approvals", count);
        }
        Err(e) => {
            tracing::error!("Failed to timeout expired pending approvals: {}", e);
        }
        _ => {}
    }

    // Find orphaned approvals (execution process is dead but approval is still pending)
    match PendingApprovalRecord::find_pending_orphaned(pool).await {
        Ok(approvals) if !approvals.is_empty() => {
            tracing::info!(
                "Found {} orphaned pending approval(s) awaiting user response",
                approvals.len()
            );
            for approval in &approvals {
                tracing::info!(
                    "  Orphaned approval {} for task {} (tool: {})",
                    approval.id,
                    approval.task_id,
                    approval.tool_name
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("Failed to find orphaned pending approvals: {}", e);
        }
    }
}

/// Build a plan prompt that includes prior user answers from an AskUserQuestion interaction.
pub fn build_auto_plan_prompt_with_answers(
    title: &str,
    description: Option<&str>,
    answers: &HashMap<String, String>,
) -> String {
    let mut prompt = build_auto_plan_prompt(title, description);

    if !answers.is_empty() {
        prompt.push_str("\n## Previous User Input\n\n");
        prompt.push_str(
            "You previously asked the user questions. Here are their answers — incorporate them directly:\n\n",
        );
        for (question, answer) in answers {
            prompt.push_str(&format!("**Q:** {}\n**A:** {}\n\n", question, answer));
        }
        prompt.push_str("Do NOT re-ask any of the above questions. Use the answers as given.\n");
    }

    prompt
}

/// Spawn a new auto-plan that incorporates user answers from a recovered approval.
/// This re-runs the plan generation with the Q&A context injected into the prompt.
pub fn spawn_auto_plan_with_answers<C: ContainerService + Clone + Send + Sync + 'static>(
    container: C,
    pool: SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: String,
    description: Option<String>,
    answers: HashMap<String, String>,
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
        let repo_path = match ProjectRepo::find_repos_for_project(&pool, project_id).await {
            Ok(repos) if !repos.is_empty() => repos[0].path.to_string_lossy().to_string(),
            Ok(_) => {
                tracing::error!(
                    "No repos found for project {} while re-generating plan for task {}",
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

        // Get the task record for start_plan_workspace
        let task = match Task::find_by_id(&pool, task_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                tracing::error!("Task {} not found for auto-plan with answers", task_id);
                return;
            }
            Err(e) => {
                tracing::error!("Failed to find task {}: {}", task_id, e);
                return;
            }
        };

        let prompt = build_auto_plan_prompt_with_answers(&title, description.as_deref(), &answers);

        // Start the interactive plan workspace — the exit monitor handles the rest
        if let Err(e) = container
            .start_plan_workspace(&task, &repo_path, prompt)
            .await
        {
            tracing::error!(
                "Failed to start plan workspace with answers for task {}: {}",
                task_id,
                e
            );
            let _ = Task::update_plan(
                &pool,
                task_id,
                &format!("Failed to start plan generation: {e}"),
                "failed",
            )
            .await;
        }
    });
}
