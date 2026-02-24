use std::{path::Path, sync::Arc};

use db::models::{
    session::CreateSession,
    swarm::{Swarm, SwarmStatus},
    swarm_agent::{SwarmAgent, SwarmAgentStatus},
    swarm_agent_dependency::SwarmAgentDependency,
    swarm_succession::{SwarmSuccession, SwarmSuccessionStatus},
    task::Task,
    task_dependency::TaskDependency,
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use executors::{
    actions::{
        ExecutorAction, ExecutorActionType, coding_agent_initial::CodingAgentInitialRequest,
    },
    profile::ExecutorProfileId,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use super::{container::ContainerService, context_monitor::ContextMonitor};

#[derive(Debug, Error)]
pub enum SwarmCoordinatorError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Container(#[from] super::container::ContainerError),
    #[error(transparent)]
    Workspace(#[from] db::models::workspace::WorkspaceError),
    #[error(transparent)]
    Session(#[from] db::models::session::SessionError),
    #[error("Swarm not found: {0}")]
    SwarmNotFound(Uuid),
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Max generations exceeded for agent {0}")]
    MaxGenerationsExceeded(Uuid),
    #[error("Max agents exceeded for swarm {0}")]
    MaxAgentsExceeded(Uuid),
    #[error("Verifier failed: {0}")]
    VerifierFailed(String),
    #[error("No child tasks found for task {0}")]
    NoChildTasks(Uuid),
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),
}

// Safety caps
const MAX_GENERATIONS: i64 = 5;
const MAX_TOTAL_AGENTS: usize = 50;
const DEFAULT_CONTEXT_THRESHOLD: f64 = 0.8;

#[derive(Debug, Deserialize)]
pub struct VerificationReport {
    pub completed: Vec<String>,
    pub issues: Vec<String>,
    pub remaining: Vec<String>,
    pub confidence: f64,
}

pub enum SpawnResult {
    AgentStarted(Uuid),
    AllComplete,
    Deadlocked,
}

/// Start a new swarm for the given task. The task must already have child tasks
/// (decomposition children). Creates SwarmAgent records for each child, wires up
/// dependencies, and starts executing the first eligible agent.
pub async fn start_swarm(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    task: &Task,
    workspace: &Workspace,
    routing_decision: Option<String>,
    executor_profile_id: &ExecutorProfileId,
) -> Result<Swarm, SwarmCoordinatorError> {
    // Get child tasks
    let children = Task::find_by_parent_task_id(pool, task.id).await?;
    if children.is_empty() {
        return Err(SwarmCoordinatorError::NoChildTasks(task.id));
    }

    // Create swarm record
    let swarm = Swarm::create(
        pool,
        Uuid::new_v4(),
        task.id,
        workspace.id,
        None, // root swarm, no parent agent
        0,
        MAX_GENERATIONS,
        routing_decision,
    )
    .await?;

    // Create SwarmAgent records for each child task, and build a mapping
    // from child task_id -> swarm_agent_id for dependency wiring
    let mut task_to_agent: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();

    for (i, child) in children.iter().enumerate() {
        let subtask_description = if let Some(desc) = &child.description {
            format!("{}\n\n{}", child.title, desc)
        } else {
            child.title.clone()
        };

        let agent = SwarmAgent::create(
            pool,
            Uuid::new_v4(),
            swarm.id,
            subtask_description,
            0, // generation 0
            None,
            DEFAULT_CONTEXT_THRESHOLD,
            i as i64,
        )
        .await?;

        task_to_agent.insert(child.id, agent.id);
    }

    // Wire up dependencies: for each child task's TaskDependency, create a SwarmAgentDependency
    for child in &children {
        let deps = TaskDependency::find_dependencies(pool, child.id).await?;
        if !deps.is_empty() {
            let agent_id = task_to_agent[&child.id];
            let dep_agent_ids: Vec<Uuid> = deps
                .iter()
                .filter_map(|dep_task_id| task_to_agent.get(dep_task_id).copied())
                .collect();
            if !dep_agent_ids.is_empty() {
                SwarmAgentDependency::create_batch(pool, agent_id, &dep_agent_ids).await?;
            }
        }
    }

    // Set swarm to running
    Swarm::update_status(pool, swarm.id, SwarmStatus::Running).await?;

    // Start executing first eligible agent
    spawn_next_agent(pool, container, swarm.id, executor_profile_id).await?;

    // Re-fetch swarm with updated status
    Swarm::find_by_id(pool, swarm.id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm.id))
}

/// Find and spawn the next eligible agent in the swarm.
pub async fn spawn_next_agent(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
    executor_profile_id: &ExecutorProfileId,
) -> Result<SpawnResult, SwarmCoordinatorError> {
    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm_id))?;

    // Find next eligible agent
    let next_agent = SwarmAgent::find_next_eligible(pool, swarm_id).await?;

    match next_agent {
        Some(agent) => {
            // Safety cap: total agents
            let total = SwarmAgent::count_by_swarm_id(pool, swarm_id).await? as usize;
            if total >= MAX_TOTAL_AGENTS {
                Swarm::update_status(pool, swarm_id, SwarmStatus::Failed).await?;
                return Err(SwarmCoordinatorError::MaxAgentsExceeded(swarm_id));
            }

            // Build agent prompt
            let prompt = build_agent_prompt(&agent.subtask_description, None);

            // Get parent workspace for branch/container info
            let parent_workspace = Workspace::find_by_id(pool, swarm.workspace_id)
                .await?
                .ok_or(SwarmCoordinatorError::WorkspaceNotFound(swarm.workspace_id))?;

            // Create workspace for this agent (shares parent's worktree)
            let workspace_id = Uuid::new_v4();
            let child_workspace = Workspace::create(
                pool,
                &CreateWorkspace {
                    branch: parent_workspace.branch.clone(),
                    agent_working_dir: parent_workspace.agent_working_dir.clone(),
                },
                workspace_id,
                swarm.task_id,
            )
            .await?;

            // Copy workspace repos from parent
            let parent_repos = WorkspaceRepo::find_repos_with_target_branch_for_workspace(
                pool,
                parent_workspace.id,
            )
            .await?;
            let workspace_repos: Vec<CreateWorkspaceRepo> = parent_repos
                .iter()
                .map(|r| CreateWorkspaceRepo {
                    repo_id: r.repo.id,
                    target_branch: r.target_branch.clone(),
                })
                .collect();
            WorkspaceRepo::create_many(pool, child_workspace.id, &workspace_repos).await?;

            // Share container ref from parent
            if let Some(ref container_ref) = parent_workspace.container_ref {
                Workspace::update_container_ref(pool, child_workspace.id, container_ref).await?;
            }

            // Re-fetch workspace with container_ref
            let child_workspace = Workspace::find_by_id(pool, child_workspace.id)
                .await?
                .ok_or(SwarmCoordinatorError::WorkspaceNotFound(child_workspace.id))?;

            // Create session
            let session = db::models::session::Session::create(
                pool,
                &CreateSession {
                    executor: Some(executor_profile_id.executor.to_string()),
                },
                Uuid::new_v4(),
                child_workspace.id,
            )
            .await?;

            let working_dir = child_workspace
                .agent_working_dir
                .as_ref()
                .filter(|dir| !dir.is_empty())
                .cloned();

            let coding_action = ExecutorAction::new(
                ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                    prompt,
                    executor_profile_id: executor_profile_id.clone(),
                    working_dir,
                }),
                None,
            );

            // Start the execution
            let execution_process = container
                .start_execution(
                    &child_workspace,
                    &session,
                    &coding_action,
                    &db::models::execution_process::ExecutionProcessRunReason::CodingAgent,
                )
                .await?;

            // Link execution_process_id to the agent
            SwarmAgent::update_execution_process_id(pool, agent.id, execution_process.id).await?;

            // Set agent status to Running
            SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Running).await?;

            // Start context monitor for this execution
            if let Some(msg_store) = container.get_msg_store_by_id(&execution_process.id).await {
                start_context_monitor(
                    pool.clone(),
                    container,
                    agent.id,
                    execution_process.id,
                    msg_store,
                    agent.context_threshold,
                );
            }

            Ok(SpawnResult::AgentStarted(agent.id))
        }
        None => {
            // No eligible agent. Check if all done or deadlocked.
            if SwarmAgent::all_complete(pool, swarm_id).await? {
                complete_swarm(pool, swarm_id).await?;
                Ok(SpawnResult::AllComplete)
            } else {
                Swarm::update_status(pool, swarm_id, SwarmStatus::Failed).await?;
                tracing::warn!(
                    "Swarm {} deadlocked: no eligible agents but not all complete",
                    swarm_id
                );
                Ok(SpawnResult::Deadlocked)
            }
        }
    }
}

/// Build the execution prompt for a swarm agent.
fn build_agent_prompt(subtask_description: &str, verification_report: Option<&str>) -> String {
    let mut prompt = String::new();

    prompt.push_str("# Swarm Agent Task\n\n");
    prompt.push_str("Implement the following task. Work through each step carefully.\n\n");
    prompt.push_str(subtask_description);

    if let Some(report) = verification_report {
        prompt.push_str("\n\n## Predecessor Verification Report\n\n");
        prompt.push_str(
            "A previous agent worked on this task but hit its context limit. \
             The verifier found the following:\n\n",
        );
        // Cap at ~10K chars
        let capped = if report.len() > 10_000 {
            &report[..10_000]
        } else {
            report
        };
        prompt.push_str(capped);
    }

    prompt.push_str("\n\n## Execution Guidelines\n");
    prompt.push_str("- Complete each step in order\n");
    prompt.push_str("- Verify each change compiles/works before moving to the next\n");
    prompt.push_str("- Commit your changes frequently with descriptive messages\n");
    prompt.push_str("- If you encounter blockers, document them clearly\n");
    prompt.push_str("- When done, summarize all changes made\n");

    prompt
}

/// Called when a context threshold event fires for a swarm agent.
pub async fn on_threshold_crossed(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    agent_id: Uuid,
) -> Result<(), SwarmCoordinatorError> {
    let agent = SwarmAgent::find_by_id(pool, agent_id)
        .await?
        .ok_or(SwarmCoordinatorError::AgentNotFound(agent_id))?;

    // Mark agent as threshold
    SwarmAgent::update_status(pool, agent_id, SwarmAgentStatus::Threshold).await?;

    // Send graceful shutdown message
    if let Some(exec_id) = agent.execution_process_id {
        let shutdown_msg = "IMPORTANT: You have reached your context utilization threshold.\n\
            Please wrap up your current work immediately:\n\
            1. Save all changes (git add + commit)\n\
            2. Write a brief self-assessment of what you completed and what remains\n\
            3. Exit gracefully"
            .to_string();

        if let Err(e) = container.send_message_to_agent(exec_id, shutdown_msg).await {
            tracing::warn!(
                "Failed to send shutdown message to agent {}: {}",
                agent_id,
                e
            );
        }
    }

    Ok(())
}

/// Called from finalize_task() when an execution belonging to a swarm agent completes.
pub async fn on_agent_completed(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    execution_process_id: Uuid,
    executor_profile_id: &ExecutorProfileId,
) -> Result<(), SwarmCoordinatorError> {
    let agent = SwarmAgent::find_by_execution_process_id(pool, execution_process_id)
        .await?
        .ok_or(SwarmCoordinatorError::AgentNotFound(execution_process_id))?;

    let swarm = Swarm::find_by_id(pool, agent.swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(agent.swarm_id))?;

    if agent.status == SwarmAgentStatus::Threshold {
        // Agent hit context threshold -> initiate succession
        begin_succession(pool, container, &agent, &swarm, executor_profile_id).await?;
    } else {
        // Normal completion
        SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Completed).await?;

        tracing::info!(
            "Swarm agent {} completed normally, advancing swarm {}",
            agent.id,
            swarm.id
        );

        // Try to spawn the next agent
        spawn_next_agent(pool, container, swarm.id, executor_profile_id).await?;
    }

    Ok(())
}

/// Initiate a verified succession for an agent that hit its context threshold.
async fn begin_succession(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    agent: &SwarmAgent,
    swarm: &Swarm,
    executor_profile_id: &ExecutorProfileId,
) -> Result<(), SwarmCoordinatorError> {
    // Check safety cap
    if agent.generation >= MAX_GENERATIONS {
        SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Failed).await?;
        Swarm::update_status(pool, swarm.id, SwarmStatus::Failed).await?;
        return Err(SwarmCoordinatorError::MaxGenerationsExceeded(agent.id));
    }

    // For V1, use a placeholder self-assessment
    let self_assessment = "Agent hit context threshold and was instructed to wrap up.".to_string();

    // Create succession record
    let succession = SwarmSuccession::create(
        pool,
        Uuid::new_v4(),
        swarm.id,
        agent.id,
        Some(self_assessment.clone()),
    )
    .await?;

    SwarmSuccession::update_status(pool, succession.id, SwarmSuccessionStatus::Verifying).await?;

    // Get workspace for git diff
    let workspace = Workspace::find_by_id(pool, swarm.workspace_id)
        .await?
        .ok_or(SwarmCoordinatorError::WorkspaceNotFound(swarm.workspace_id))?;

    let working_dir = if let Some(ref container_ref) = workspace.container_ref {
        std::path::PathBuf::from(container_ref)
    } else {
        std::path::PathBuf::from(".")
    };

    // Get git diff for verifier (best-effort)
    let git_diff = get_git_diff_summary(&working_dir);

    // Run verifier
    let report = match run_verifier(
        &agent.subtask_description,
        &git_diff,
        &self_assessment,
        &working_dir,
    ) {
        Ok(report) => report,
        Err(e) => {
            tracing::warn!("Verifier failed for agent {}: {}", agent.id, e);
            // Default to corrective strategy if verifier fails
            VerificationReport {
                completed: vec![],
                issues: vec![format!("Verifier failed: {}", e)],
                remaining: vec!["Unable to determine remaining work".to_string()],
                confidence: 0.5,
            }
        }
    };

    let report_json = serde_json::to_string_pretty(&serde_json::json!({
        "completed": report.completed,
        "issues": report.issues,
        "remaining": report.remaining,
        "confidence": report.confidence,
    }))
    .unwrap_or_default();

    // Determine recovery strategy
    let recovery_strategy = if report.confidence >= 0.3 {
        "corrective"
    } else {
        "clean_restart"
    };

    // Update succession with verification results
    SwarmSuccession::update_verification(
        pool,
        succession.id,
        &report_json,
        report.confidence,
        recovery_strategy,
    )
    .await?;

    // Create successor agent
    let successor_prompt = if recovery_strategy == "corrective" {
        // Successor gets predecessor's verification report
        build_agent_prompt(&agent.subtask_description, Some(&report_json))
    } else {
        // Clean restart: only original subtask
        build_agent_prompt(&agent.subtask_description, None)
    };

    let successor = SwarmAgent::create(
        pool,
        Uuid::new_v4(),
        swarm.id,
        successor_prompt,
        agent.generation + 1,
        Some(agent.id),
        DEFAULT_CONTEXT_THRESHOLD,
        agent.sort_order,
    )
    .await?;

    // Link successor to succession
    SwarmSuccession::update_successor_id(pool, succession.id, successor.id).await?;
    SwarmSuccession::update_status(pool, succession.id, SwarmSuccessionStatus::SuccessorRunning)
        .await?;

    tracing::info!(
        "Created successor agent {} (gen {}) for predecessor {} in swarm {}",
        successor.id,
        successor.generation,
        agent.id,
        swarm.id
    );

    // Spawn the successor (find_next_eligible will pick it up since it's pending)
    spawn_next_agent(pool, container, swarm.id, executor_profile_id).await?;

    Ok(())
}

/// Run the verifier using `claude --print` to evaluate the predecessor's work.
fn run_verifier(
    subtask: &str,
    git_diff: &str,
    self_assessment: &str,
    working_dir: &Path,
) -> Result<VerificationReport, SwarmCoordinatorError> {
    let prompt = format!(
        r#"You are a code verification agent. Evaluate the work done on a subtask.

## Original Subtask
{subtask}

## Git Diff Summary
{git_diff}

## Agent Self-Assessment
{self_assessment}

## Instructions
Evaluate what was actually completed vs what was claimed. Respond with ONLY a JSON object:

{{"completed": ["list of completed items"], "issues": ["list of issues found"], "remaining": ["list of remaining work"], "confidence": 0.0 to 1.0}}

The confidence score should reflect how much of the subtask was successfully completed (1.0 = fully done, 0.0 = nothing done)."#
    );

    let output = std::process::Command::new("claude")
        .args(["--print", "-p", &prompt])
        .current_dir(working_dir)
        .output()
        .map_err(|e| {
            SwarmCoordinatorError::VerifierFailed(format!("Failed to spawn claude: {e}"))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SwarmCoordinatorError::VerifierFailed(format!(
            "claude exited with status {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    parse_verification_output(&stdout)
}

/// Parse the verifier output, tolerating markdown fences and surrounding text.
fn parse_verification_output(output: &str) -> Result<VerificationReport, SwarmCoordinatorError> {
    let trimmed = output.trim();

    // Strip markdown fences if present
    let json_str = if trimmed.starts_with("```") {
        let without_opening = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
    } else {
        trimmed
    };

    // Try to find JSON object boundaries (same pattern as decomposer)
    let json_str = if json_str.starts_with('{') {
        json_str
    } else {
        find_json_object_start(json_str).unwrap_or(json_str)
    };

    serde_json::from_str(json_str).map_err(|e| {
        SwarmCoordinatorError::VerifierFailed(format!("Failed to parse verification report: {e}"))
    })
}

/// Find the start of a JSON object in text (copied from decomposer pattern).
fn find_json_object_start(s: &str) -> Option<&str> {
    let last_brace = s.rfind('}')?;
    let mut search_from = 0;
    while let Some(pos) = s[search_from..].find('{') {
        let abs_pos = search_from + pos;
        let after = s[abs_pos + 1..].trim_start();
        if after.starts_with('"') {
            return Some(&s[abs_pos..=last_brace]);
        }
        search_from = abs_pos + 1;
    }
    None
}

/// Mark a swarm as completed and handle any parent linkage.
async fn complete_swarm(pool: &SqlitePool, swarm_id: Uuid) -> Result<(), SwarmCoordinatorError> {
    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm_id))?;

    Swarm::update_status(pool, swarm_id, SwarmStatus::Completed).await?;

    tracing::info!("Swarm {} completed", swarm_id);

    // If root swarm, transition parent task to QA
    if swarm.parent_agent_id.is_none() {
        super::ralph_loop::complete_ralph_loop(pool, swarm.task_id).await?;
    }

    Ok(())
}

/// Get a summary of git changes in the working directory (best-effort).
fn get_git_diff_summary(working_dir: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["diff", "--stat", "HEAD~1"])
        .current_dir(working_dir)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let diff = String::from_utf8_lossy(&out.stdout).to_string();
            if diff.len() > 5000 {
                format!("{}...(truncated)", &diff[..5000])
            } else {
                diff
            }
        }
        _ => "(git diff unavailable)".to_string(),
    }
}

/// Start the context monitor for a swarm agent execution, spawning a background
/// task that triggers succession on threshold.
fn start_context_monitor(
    pool: SqlitePool,
    _container: &(impl ContainerService + Sync + ?Sized),
    agent_id: Uuid,
    execution_process_id: Uuid,
    msg_store: Arc<utils::msg_store::MsgStore>,
    threshold: f64,
) {
    let (_handle, rx) = ContextMonitor::watch(execution_process_id, msg_store, threshold);

    // We can't hold a reference to `container` in the spawned task, so the
    // threshold crossing will be handled when the agent's execution completes
    // in on_agent_completed() by checking the agent's status (Threshold).
    // The monitor just marks the agent status.
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        match rx.await {
            Ok(event) => {
                tracing::info!(
                    "Context threshold crossed for swarm agent {} (exec {}): {:.1}% utilization",
                    agent_id,
                    event.execution_process_id,
                    event.utilization * 100.0
                );

                // Update agent context token info
                if let Err(e) = SwarmAgent::update_context_tokens(
                    &pool_clone,
                    agent_id,
                    event.tokens_used as i64,
                )
                .await
                {
                    tracing::error!(
                        "Failed to update context tokens for agent {}: {}",
                        agent_id,
                        e
                    );
                }

                // Mark agent as threshold - on_agent_completed will handle succession
                if let Err(e) =
                    SwarmAgent::update_status(&pool_clone, agent_id, SwarmAgentStatus::Threshold)
                        .await
                {
                    tracing::error!(
                        "Failed to update agent {} status to Threshold: {}",
                        agent_id,
                        e
                    );
                }
            }
            Err(_) => {
                tracing::debug!(
                    "Context monitor channel closed for agent {} (execution may have finished)",
                    agent_id
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_agent_prompt_basic() {
        let prompt = build_agent_prompt("Create a login page", None);
        assert!(prompt.contains("# Swarm Agent Task"));
        assert!(prompt.contains("Create a login page"));
        assert!(prompt.contains("## Execution Guidelines"));
        assert!(!prompt.contains("Predecessor Verification Report"));
    }

    #[test]
    fn test_build_agent_prompt_with_verification() {
        let report = r#"{"completed": ["Step 1"], "remaining": ["Step 2"]}"#;
        let prompt = build_agent_prompt("Create a login page", Some(report));
        assert!(prompt.contains("## Predecessor Verification Report"));
        assert!(prompt.contains("Step 1"));
    }

    #[test]
    fn test_build_agent_prompt_verification_report_capped() {
        let long_report = "x".repeat(15_000);
        let prompt = build_agent_prompt("task", Some(&long_report));
        // The report should be capped
        assert!(prompt.len() < 15_500);
    }

    #[test]
    fn test_parse_verification_output_clean() {
        let output =
            r#"{"completed": ["a", "b"], "issues": [], "remaining": ["c"], "confidence": 0.7}"#;
        let report = parse_verification_output(output).unwrap();
        assert_eq!(report.completed, vec!["a", "b"]);
        assert_eq!(report.confidence, 0.7);
    }

    #[test]
    fn test_parse_verification_output_with_fences() {
        let output = "```json\n{\"completed\": [], \"issues\": [], \"remaining\": [], \"confidence\": 0.5}\n```";
        let report = parse_verification_output(output).unwrap();
        assert_eq!(report.confidence, 0.5);
    }

    #[test]
    fn test_parse_verification_output_with_surrounding_text() {
        let output = "Here is my evaluation:\n{\"completed\": [\"done\"], \"issues\": [], \"remaining\": [], \"confidence\": 0.9}\nThat's all.";
        let report = parse_verification_output(output).unwrap();
        assert_eq!(report.completed, vec!["done"]);
    }

    #[test]
    fn test_find_json_object_start() {
        let s = "some text {\"key\": \"value\"}";
        let result = find_json_object_start(s);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with('{'));
    }

    #[test]
    fn test_find_json_object_start_with_url_braces() {
        let s = "Route {event}/path\n{\"key\": \"value\"}";
        let result = find_json_object_start(s);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "{\"key\": \"value\"}");
    }
}
