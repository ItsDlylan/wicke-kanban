use std::path::Path;

use db::models::{
    project_repo::ProjectRepo,
    spec_sheet::{CreateSpecSheet, SpecSheet},
    task::{Task, TaskStatus},
};
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use super::{decomposer, interview, spec_assessor, spec_generator};

/// Maximum re-entries for the interview insufficiency loop (section 6.6, Loop 1).
const MAX_PLAN_REENTRIES: u32 = 2;
/// Minimum confidence threshold for planning output (section 6.6, Loop 1).
const MIN_PLAN_CONFIDENCE: f64 = 0.6;

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

/// Build a plan prompt that includes additional context from a previous attempt
/// and targeted follow-up areas (section 6.6, Loop 1).
fn build_reentry_plan_prompt(
    title: &str,
    description: Option<&str>,
    previous_plan: &str,
    focus_areas: &[String],
) -> String {
    let mut prompt = build_auto_plan_prompt(title, description);

    prompt.push_str("\n## Previous Plan Attempt\n\n");
    prompt.push_str("A previous planning attempt produced the following plan, but it was identified as having low confidence. ");
    prompt
        .push_str("Please refine the plan, paying special attention to the areas noted below.\n\n");

    // Cap the previous plan to avoid excessive prompt size
    let capped_plan = if previous_plan.len() > 8000 {
        &previous_plan[..8000]
    } else {
        previous_plan
    };
    prompt.push_str(capped_plan);

    if !focus_areas.is_empty() {
        prompt.push_str("\n\n## Areas Needing More Detail\n\n");
        for area in focus_areas {
            prompt.push_str(&format!("- {}\n", area));
        }
    }

    prompt
}

/// Shell out to `claude --print -p <prompt>` to generate a plan.
/// This is a blocking call — use `spawn_blocking` from async context.
pub fn run_plan_generation(prompt: &str, working_dir: &Path) -> Result<String, AutoPlannerError> {
    let output = std::process::Command::new("claude")
        .args(["--print", "--permission-mode=plan", "-p", prompt])
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

/// Estimate planning confidence from the generated plan text (section 6.6, Loop 1).
/// Heuristic: checks for TODO/TBD markers, question marks, and plan length as
/// signals of confidence. Returns a value between 0.0 and 1.0.
fn estimate_plan_confidence(plan_text: &str) -> f64 {
    let lines: Vec<&str> = plan_text.lines().collect();
    let total_lines = lines.len() as f64;

    if total_lines < 5.0 {
        return 0.3; // Very short plans are low confidence
    }

    let mut penalty = 0.0;

    // Count uncertainty markers
    let uncertainty_markers = lines
        .iter()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("todo:")
                || lower.contains("tbd:")
                || lower.contains("unclear")
                || lower.contains("not sure")
                || lower.contains("need to determine")
                || lower.contains("clarify:")
        })
        .count() as f64;

    penalty += (uncertainty_markers / total_lines) * 0.5;

    // Count open questions in plan
    let question_count = lines.iter().filter(|l| l.trim().ends_with('?')).count() as f64;
    penalty += (question_count / total_lines) * 0.3;

    // Very short plans with few steps
    if total_lines < 15.0 {
        penalty += 0.1;
    }

    (1.0 - penalty).max(0.0).min(1.0)
}

/// Spawn a background task that generates a plan for the given task.
///
/// 1. Sets plan_status = "generating"
/// 2. Gets working dir from project repos
/// 3. Runs `claude --print` via spawn_blocking
/// 4. Checks planning confidence (section 6.6, Loop 1)
/// 5. On success: stores plan text, transitions to Plan status
/// 6. On failure: sets plan_status = "failed", leaves task in Backlog
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

        // Plan generation with re-entry loop (section 6.6, Loop 1)
        let mut plan_text = String::new();
        let mut attempt = 0u32;

        loop {
            let prompt = if attempt == 0 {
                build_auto_plan_prompt(&title, description.as_deref())
            } else {
                let open_questions = interview::extract_open_questions(&plan_text);
                let focus_areas: Vec<String> =
                    open_questions.iter().map(|q| q.question.clone()).collect();
                build_reentry_plan_prompt(&title, description.as_deref(), &plan_text, &focus_areas)
            };

            let working_path = std::path::PathBuf::from(&working_dir);
            let result =
                tokio::task::spawn_blocking(move || run_plan_generation(&prompt, &working_path))
                    .await;

            match result {
                Ok(Ok(new_plan_text)) => {
                    plan_text = new_plan_text;
                }
                Ok(Err(e)) => {
                    tracing::error!("Plan generation failed for task {}: {}", task_id, e);
                    let _ = Task::update_plan(
                        &pool,
                        task_id,
                        &format!("Plan generation failed: {e}"),
                        "failed",
                    )
                    .await;
                    return;
                }
                Err(e) => {
                    tracing::error!("Plan generation task panicked for task {}: {}", task_id, e);
                    let _ = Task::update_plan(
                        &pool,
                        task_id,
                        "Plan generation task panicked",
                        "failed",
                    )
                    .await;
                    return;
                }
            }

            // Check planning confidence (section 6.6, Loop 1)
            let confidence = estimate_plan_confidence(&plan_text);
            attempt += 1;

            if confidence >= MIN_PLAN_CONFIDENCE || attempt > MAX_PLAN_REENTRIES {
                if confidence < MIN_PLAN_CONFIDENCE {
                    tracing::warn!(
                        "Plan for task {} has low confidence ({:.2}) after {} attempts, proceeding anyway",
                        task_id,
                        confidence,
                        attempt
                    );
                } else {
                    tracing::info!(
                        "Plan for task {} confidence: {:.2} (attempt {})",
                        task_id,
                        confidence,
                        attempt
                    );
                }
                break;
            }

            tracing::info!(
                "Plan for task {} has low confidence ({:.2}), re-entering planning (attempt {}/{})",
                task_id,
                confidence,
                attempt,
                MAX_PLAN_REENTRIES + 1
            );
        }

        // Success: store plan
        if let Err(e) = Task::update_plan(&pool, task_id, &plan_text, "completed").await {
            tracing::error!("Failed to store plan for task {}: {}", task_id, e);
            return;
        }

        // Extract and log interview questions from the plan (section 6.3.2)
        let open_questions = interview::extract_open_questions(&plan_text);
        if !open_questions.is_empty() {
            tracing::info!(
                "Plan for task {} has {} open questions that may need interview",
                task_id,
                open_questions.len()
            );
            // Store interview phase as JSON in the plan for future use
            let interview_phase = interview::InterviewPhase::with_questions(open_questions);
            if let Ok(interview_json) = serde_json::to_string(&interview_phase) {
                // Store interview data alongside the plan (non-fatal if fails)
                if let Err(e) = Task::update_plan_status(&pool, task_id, "completed").await {
                    tracing::warn!(
                        "Failed to update plan status after interview extraction for task {}: {}",
                        task_id,
                        e
                    );
                }
                // Log the interview data for now; full DB storage can be added later
                tracing::debug!("Interview phase for task {}: {}", task_id, interview_json);
            }
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
    });
}

/// After plan generation succeeds, automatically generate a spec sheet and decompose the task
/// into child tasks so it's prepared for Ralph execution. Failures are logged but non-fatal.
/// This function is idempotent — it skips steps that have already completed (e.g. on recovery).
async fn auto_prepare_for_ralph(
    pool: &SqlitePool,
    task_id: Uuid,
    project_id: Uuid,
    title: &str,
    description: Option<&str>,
    plan_text: &str,
    working_dir: &Path,
) {
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
                    return;
                }
            }
        }
    };

    // Step 1.5: Run routing assessment (skip for trivial tasks)
    // Uses the concurrent 3-agent ranking pipeline when spec is substantial enough (section 6.3.4)
    let should_assess = stored_spec
        .overview
        .as_deref()
        .map_or(false, |o| o.len() > 50);
    if should_assess {
        match spec_assessor::run_full_assessment(&stored_spec, title, Path::new(&working_dir)).await
        {
            Ok((assessment, synthesis)) => {
                let routing = if let Some(ref synth) = synthesis {
                    spec_assessor::route_from_synthesis(synth)
                } else {
                    spec_assessor::route_from_score(&assessment)
                };

                if let Err(e) = SpecSheet::update_complexity_score(
                    pool,
                    stored_spec.id,
                    assessment.complexity_score as i64,
                )
                .await
                {
                    tracing::warn!(
                        "Failed to store complexity score for task {}: {}",
                        task_id,
                        e
                    );
                }
                if let Err(e) = Task::update_routing_decision(pool, task_id, &routing).await {
                    tracing::warn!(
                        "Failed to store routing decision for task {}: {}",
                        task_id,
                        e
                    );
                }

                if let Some(synth) = &synthesis {
                    tracing::info!(
                        "Spec assessment for task {} (3-agent synthesis): score={}, routing={}, confidence={:.2}, divergence={}",
                        task_id,
                        assessment.complexity_score,
                        routing,
                        synth.confidence,
                        synth.divergence_detected
                    );
                } else {
                    tracing::info!(
                        "Spec assessment for task {} (fallback single): score={}, routing={}",
                        task_id,
                        assessment.complexity_score,
                        routing
                    );
                }
            }
            Err(e) => {
                tracing::warn!("Spec assessment failed for task {}: {}", task_id, e);
            }
        }
    }

    // Step 2: Check if child tasks already exist (idempotent on recovery)
    let existing_children = Task::find_by_parent_task_id(pool, task_id).await;
    if let Ok(ref children) = existing_children {
        if !children.is_empty() {
            tracing::info!(
                "Task {} already has {} child tasks, skipping decomposition",
                task_id,
                children.len()
            );
            return;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_auto_plan_prompt_with_description() {
        let prompt = build_auto_plan_prompt("Add login", Some("Create login page"));
        assert!(prompt.contains("# Implementation Plan: Add login"));
        assert!(prompt.contains("Create login page"));
    }

    #[test]
    fn test_build_auto_plan_prompt_no_description() {
        let prompt = build_auto_plan_prompt("Add login", None);
        assert!(prompt.contains("# Implementation Plan: Add login"));
        assert!(!prompt.contains("## Task Description"));
    }

    #[test]
    fn test_build_reentry_plan_prompt() {
        let prompt = build_reentry_plan_prompt(
            "Feature X",
            Some("Build feature X"),
            "## Step 1\nDo something\nTODO: Determine schema",
            &["What schema to use?".to_string()],
        );
        assert!(prompt.contains("Previous Plan Attempt"));
        assert!(prompt.contains("Do something"));
        assert!(prompt.contains("Areas Needing More Detail"));
        assert!(prompt.contains("What schema to use?"));
    }

    #[test]
    fn test_estimate_plan_confidence_high() {
        let plan = (0..30)
            .map(|i| format!("{}. Step {}: Do something concrete here", i + 1, i + 1))
            .collect::<Vec<_>>()
            .join("\n");
        let confidence = estimate_plan_confidence(&plan);
        assert!(confidence > 0.8);
    }

    #[test]
    fn test_estimate_plan_confidence_low_with_todos() {
        let plan = "1. Step 1: Do X\nTODO: Figure out the schema\nTODO: Unclear API design\nTBD: Which library to use\n4. Step 4: Deploy";
        let confidence = estimate_plan_confidence(plan);
        assert!(confidence < 0.7);
    }

    #[test]
    fn test_estimate_plan_confidence_very_short() {
        let plan = "Do the thing.\nThat's it.";
        let confidence = estimate_plan_confidence(plan);
        assert!(confidence <= 0.3);
    }

    #[test]
    fn test_estimate_plan_confidence_questions() {
        let plan = "1. Create the database schema\nShould we use PostgreSQL?\nWhat about migrations?\n4. Implement API";
        let confidence = estimate_plan_confidence(plan);
        assert!(confidence < 0.8);
    }
}
