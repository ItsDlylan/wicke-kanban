use std::{path::Path, sync::Arc};

use db::models::{
    execution_process::{ExecutionProcess, ExecutionProcessStatus},
    session::CreateSession,
    spec_sheet::SpecSheet,
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
    logs::{NormalizedEntryType, utils::patch::extract_normalized_entry_from_patch},
    profile::ExecutorProfileId,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use thiserror::Error;
use utils::log_msg::LogMsg;
use uuid::Uuid;

use super::{
    container::ContainerService,
    context_monitor::ContextMonitor,
    decomposer::{self, DecomposerError},
};

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
    #[error("Max sub-swarm depth exceeded for swarm {0}")]
    MaxDepthExceeded(Uuid),
    #[error("Decomposition failed: {0}")]
    DecompositionFailed(#[from] DecomposerError),
}

// Safety caps
const MAX_GENERATIONS: i64 = 5;
const MAX_TOTAL_AGENTS: usize = 50;
const DEFAULT_CONTEXT_THRESHOLD: f64 = 0.6;
const DEFAULT_VERIFIER_MODEL: &str = "claude-haiku-4-5-20251001";

// Redecomposition heuristic: trigger when succession_count >= this AND confidence < threshold
const REDECOMPOSITION_SUCCESSION_THRESHOLD: i64 = 2;
const REDECOMPOSITION_CONFIDENCE_THRESHOLD: f64 = 0.5;

#[derive(Debug, Deserialize)]
pub struct VerificationReport {
    pub completed: Vec<String>,
    pub issues: Vec<String>,
    pub remaining: Vec<String>,
    pub confidence: f64,
}

pub enum SpawnResult {
    AgentStarted(Uuid),
    AgentsStarted(Vec<Uuid>),
    AllComplete,
    Deadlocked,
}

/// Start a new swarm for the given task. The task must already have child tasks
/// (decomposition children). Creates SwarmAgent records for each child, wires up
/// dependencies, and starts executing all eligible agents concurrently.
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

    // Start executing all eligible agents concurrently (section 3.2.1)
    spawn_all_eligible_agents(pool, container, swarm.id, executor_profile_id).await?;

    // Re-fetch swarm with updated status
    Swarm::find_by_id(pool, swarm.id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm.id))
}

/// Find and spawn ALL eligible agents concurrently (section 3.2.1).
/// This replaces the sequential spawn_next_agent for initial spawning.
pub async fn spawn_all_eligible_agents(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
    executor_profile_id: &ExecutorProfileId,
) -> Result<SpawnResult, SwarmCoordinatorError> {
    let eligible_agents = SwarmAgent::find_all_eligible(pool, swarm_id).await?;

    if eligible_agents.is_empty() {
        // No eligible agents. Check if all done or deadlocked.
        if SwarmAgent::all_complete(pool, swarm_id).await? {
            complete_swarm(pool, container, swarm_id).await?;
            return Ok(SpawnResult::AllComplete);
        } else {
            Swarm::update_status(pool, swarm_id, SwarmStatus::Failed).await?;
            tracing::warn!(
                "Swarm {} deadlocked: no eligible agents but not all complete",
                swarm_id
            );
            return Ok(SpawnResult::Deadlocked);
        }
    }

    // Safety cap: total agents
    let total = SwarmAgent::count_by_swarm_id(pool, swarm_id).await? as usize;
    if total >= MAX_TOTAL_AGENTS {
        Swarm::update_status(pool, swarm_id, SwarmStatus::Failed).await?;
        return Err(SwarmCoordinatorError::MaxAgentsExceeded(swarm_id));
    }

    let mut started_ids = Vec::new();
    for agent in eligible_agents {
        match spawn_single_agent(pool, container, swarm_id, &agent, executor_profile_id).await {
            Ok(agent_id) => started_ids.push(agent_id),
            Err(e) => {
                tracing::warn!(
                    "Failed to spawn agent {} in swarm {}: {}",
                    agent.id,
                    swarm_id,
                    e
                );
            }
        }
    }

    if started_ids.is_empty() {
        Ok(SpawnResult::Deadlocked)
    } else if started_ids.len() == 1 {
        Ok(SpawnResult::AgentStarted(started_ids[0]))
    } else {
        Ok(SpawnResult::AgentsStarted(started_ids))
    }
}

/// Find and spawn the next eligible agent in the swarm (single-agent variant).
pub async fn spawn_next_agent(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
    executor_profile_id: &ExecutorProfileId,
) -> Result<SpawnResult, SwarmCoordinatorError> {
    spawn_all_eligible_agents(pool, container, swarm_id, executor_profile_id).await
}

/// Spawn a single agent within the swarm, creating its workspace, session,
/// and execution process.
async fn spawn_single_agent(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
    agent: &SwarmAgent,
    executor_profile_id: &ExecutorProfileId,
) -> Result<Uuid, SwarmCoordinatorError> {
    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm_id))?;

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
    let parent_repos =
        WorkspaceRepo::find_repos_with_target_branch_for_workspace(pool, parent_workspace.id)
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

    // Create git branch BEFORE building prompt so we can include it in instructions.
    // Uses `git branch` (not checkout) to avoid switching the shared working tree.
    let git_branch = format!("swarm/{}/gen-{}", swarm_id, agent.generation);
    SwarmAgent::update_git_branch(pool, agent.id, &git_branch).await?;
    create_agent_git_branch(&child_workspace, &git_branch);

    // Build agent prompt — enrich with verification report for successors
    // and include git branch for isolated checkout
    let prompt = if agent.predecessor_id.is_some() {
        let succession =
            SwarmSuccession::find_active_for_agent(pool, agent.predecessor_id.unwrap()).await?;
        let report = succession.and_then(|s| s.verification_report);
        build_agent_prompt(
            &agent.subtask_description,
            report.as_deref(),
            Some(&git_branch),
        )
    } else {
        build_agent_prompt(&agent.subtask_description, None, Some(&git_branch))
    };

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
        let protocol_peer = container.get_protocol_peer(&execution_process.id).await;
        start_context_monitor(
            pool.clone(),
            agent.id,
            execution_process.id,
            msg_store,
            agent.context_threshold,
            protocol_peer,
        );
    }

    Ok(agent.id)
}

/// Build the execution prompt for a swarm agent.
fn build_agent_prompt(
    subtask_description: &str,
    verification_report: Option<&str>,
    git_branch: Option<&str>,
) -> String {
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
    if let Some(branch) = git_branch {
        prompt.push_str(&format!(
            "- FIRST: Run `git checkout {branch}` to switch to your isolated branch\n"
        ));
    }
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

/// Cancel a swarm, stopping all running agents and marking the swarm as cancelled.
pub async fn cancel_swarm(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
) -> Result<(), SwarmCoordinatorError> {
    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm_id))?;

    if swarm.status != SwarmStatus::Running && swarm.status != SwarmStatus::Pending {
        return Ok(()); // Already terminated
    }

    // Find and stop all running agents
    let agents = SwarmAgent::find_by_swarm_id(pool, swarm_id).await?;
    for agent in &agents {
        if agent.status == SwarmAgentStatus::Running {
            if let Some(exec_id) = agent.execution_process_id {
                if let Ok(Some(exec_process)) = ExecutionProcess::find_by_id(pool, exec_id).await {
                    if let Err(e) = container
                        .stop_execution(&exec_process, ExecutionProcessStatus::Killed)
                        .await
                    {
                        tracing::warn!("Failed to stop agent {} execution: {}", agent.id, e);
                    }
                }
            }
            SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Failed).await?;
        }
    }

    Swarm::update_status(pool, swarm_id, SwarmStatus::Cancelled).await?;
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

        // Try to spawn all newly eligible agents
        spawn_all_eligible_agents(pool, container, swarm.id, executor_profile_id).await?;
    }

    Ok(())
}

/// Extract self-assessment from MsgStore history by scanning the last assistant messages
/// (section 3.4 Phase 1).
async fn extract_self_assessment_from_msg_store(
    container: &(impl ContainerService + Sync + ?Sized),
    execution_process_id: Uuid,
) -> String {
    let msg_store = match container.get_msg_store_by_id(&execution_process_id).await {
        Some(store) => store,
        None => {
            return "Agent hit context threshold and was instructed to wrap up.".to_string();
        }
    };

    let history = msg_store.get_history();

    // Scan history in reverse for the last AssistantMessage entries
    let mut assessment_parts: Vec<String> = Vec::new();
    for log_msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = log_msg {
            if let Some((_index, entry)) = extract_normalized_entry_from_patch(patch) {
                match &entry.entry_type {
                    NormalizedEntryType::AssistantMessage => {
                        if !entry.content.is_empty() {
                            assessment_parts.push(entry.content.clone());
                            // Collect up to 3 recent assistant messages for context
                            if assessment_parts.len() >= 3 {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if assessment_parts.is_empty() {
        "Agent hit context threshold and was instructed to wrap up.".to_string()
    } else {
        // Reverse so messages are in chronological order, then join
        assessment_parts.reverse();
        let combined = assessment_parts.join("\n\n---\n\n");
        // Cap at 5K characters
        if combined.len() > 5000 {
            combined[..5000].to_string()
        } else {
            combined
        }
    }
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

    // Increment succession count on the agent for redecomposition heuristic
    SwarmAgent::increment_succession_count(pool, agent.id).await?;

    // Extract real self-assessment from the agent's conversation (section 3.4 Phase 1)
    let self_assessment = if let Some(exec_id) = agent.execution_process_id {
        extract_self_assessment_from_msg_store(container, exec_id).await
    } else {
        "Agent hit context threshold and was instructed to wrap up.".to_string()
    };

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

    // Get git diff for verifier (best-effort, blocking I/O)
    let working_dir_for_diff = working_dir.clone();
    let git_diff = tokio::task::spawn_blocking(move || get_git_diff_summary(&working_dir_for_diff))
        .await
        .unwrap_or_else(|_| "(git diff panicked)".to_string());

    // Run verifier with configurable model (section 10.4 — hybrid model selection)
    let verifier_model = swarm
        .verifier_model
        .clone()
        .unwrap_or_else(|| DEFAULT_VERIFIER_MODEL.to_string());
    let subtask = agent.subtask_description.clone();
    let assessment = self_assessment.clone();
    let working_dir_for_verifier = working_dir.clone();
    let git_diff_for_verifier = git_diff.clone();
    let verifier_result = tokio::task::spawn_blocking(move || {
        run_verifier(
            &subtask,
            &git_diff_for_verifier,
            &assessment,
            &working_dir_for_verifier,
            &verifier_model,
        )
    })
    .await
    .unwrap_or_else(|_| {
        Err(SwarmCoordinatorError::VerifierFailed(
            "Verifier panicked".into(),
        ))
    });

    let report = match verifier_result {
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

    // Determine recovery strategy (section 3.5)
    // Re-fetch agent to get updated succession_count
    let agent = SwarmAgent::find_by_id(pool, agent.id)
        .await?
        .ok_or(SwarmCoordinatorError::AgentNotFound(agent.id))?;

    let recovery_strategy = determine_recovery_strategy(&agent, &report);

    // Update succession with verification results
    SwarmSuccession::update_verification(
        pool,
        succession.id,
        &report_json,
        report.confidence,
        &recovery_strategy,
    )
    .await?;

    // Execute recovery strategy
    match recovery_strategy.as_str() {
        "redecomposition" => {
            // Section 3.5 + 3.1/3.3/4.3: Redecomposition — split into sub-swarm
            tracing::info!(
                "Agent {} triggering redecomposition (succession_count={}, confidence={})",
                agent.id,
                agent.succession_count,
                report.confidence,
            );
            execute_redecomposition(pool, container, &agent, swarm, executor_profile_id).await?;
            SwarmSuccession::update_status(
                pool,
                succession.id,
                SwarmSuccessionStatus::SuccessorRunning,
            )
            .await?;
        }
        "escalation" => {
            // Section 3.5: Escalation — mark agent as failed, report to parent
            tracing::warn!("Agent {} escalating: external blocker detected", agent.id,);
            SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Failed).await?;
            SwarmSuccession::update_status(pool, succession.id, SwarmSuccessionStatus::Failed)
                .await?;

            // If this is a sub-swarm, the parent swarm will handle the failure
            // when it detects the sub-swarm failed.
            if swarm.parent_agent_id.is_some() {
                Swarm::update_status(pool, swarm.id, SwarmStatus::Failed).await?;
            }
        }
        _ => {
            // Corrective or clean_restart: create a successor agent
            let successor = SwarmAgent::create(
                pool,
                Uuid::new_v4(),
                swarm.id,
                agent.subtask_description.clone(),
                agent.generation + 1,
                Some(agent.id),
                DEFAULT_CONTEXT_THRESHOLD,
                agent.sort_order,
            )
            .await?;

            // Link successor to succession
            SwarmSuccession::update_successor_id(pool, succession.id, successor.id).await?;
            SwarmSuccession::update_status(
                pool,
                succession.id,
                SwarmSuccessionStatus::SuccessorRunning,
            )
            .await?;

            tracing::info!(
                "Created successor agent {} (gen {}) for predecessor {} in swarm {} [strategy={}]",
                successor.id,
                successor.generation,
                agent.id,
                swarm.id,
                recovery_strategy,
            );

            // Spawn the successor
            spawn_all_eligible_agents(pool, container, swarm.id, executor_profile_id).await?;
        }
    }

    Ok(())
}

/// Determine the recovery strategy based on agent history and verification report (section 3.5).
fn determine_recovery_strategy(agent: &SwarmAgent, report: &VerificationReport) -> String {
    // Check for escalation: if the report mentions external blockers
    let has_external_blocker = report.issues.iter().any(|issue| {
        let lower = issue.to_lowercase();
        lower.contains("external blocker")
            || lower.contains("blocked by")
            || lower.contains("permission denied")
            || lower.contains("access denied")
            || lower.contains("authentication")
            || lower.contains("rate limit")
    });

    if has_external_blocker {
        return "escalation".to_string();
    }

    // Check for redecomposition: repeated succession with low progress
    if agent.succession_count >= REDECOMPOSITION_SUCCESSION_THRESHOLD
        && report.confidence < REDECOMPOSITION_CONFIDENCE_THRESHOLD
    {
        return "redecomposition".to_string();
    }

    // Standard recovery strategies
    if report.confidence >= 0.3 {
        "corrective".to_string()
    } else {
        "clean_restart".to_string()
    }
}

/// Execute redecomposition: create a sub-swarm for the subtask (section 3.1, 3.3, 4.3).
async fn execute_redecomposition(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    agent: &SwarmAgent,
    parent_swarm: &Swarm,
    executor_profile_id: &ExecutorProfileId,
) -> Result<(), SwarmCoordinatorError> {
    // Check depth limit before creating sub-swarm
    if parent_swarm.depth + 1 >= parent_swarm.max_depth {
        tracing::warn!(
            "Cannot create sub-swarm: depth {} would exceed max_depth {}",
            parent_swarm.depth + 1,
            parent_swarm.max_depth,
        );
        // Fall back to corrective strategy
        SwarmAgent::update_status(pool, agent.id, SwarmAgentStatus::Failed).await?;
        return Err(SwarmCoordinatorError::MaxDepthExceeded(parent_swarm.id));
    }

    spawn_sub_swarm(pool, container, agent, parent_swarm, executor_profile_id).await
}

/// Spawn a sub-swarm for an agent's subtask (section 3.1, 3.3, 4.3).
/// Creates a new Swarm with parent_agent_id set, decomposes the subtask,
/// creates SwarmAgent records for each sub-task, and starts execution.
async fn spawn_sub_swarm(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    parent_agent: &SwarmAgent,
    parent_swarm: &Swarm,
    executor_profile_id: &ExecutorProfileId,
) -> Result<(), SwarmCoordinatorError> {
    let workspace = Workspace::find_by_id(pool, parent_swarm.workspace_id)
        .await?
        .ok_or(SwarmCoordinatorError::WorkspaceNotFound(
            parent_swarm.workspace_id,
        ))?;

    let working_dir = if let Some(ref container_ref) = workspace.container_ref {
        std::path::PathBuf::from(container_ref)
    } else {
        std::path::PathBuf::from(".")
    };

    // Decompose the subtask into smaller pieces using the existing decomposer
    let task = Task::find_by_id(pool, parent_swarm.task_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(parent_swarm.task_id))?;

    // Build a minimal spec sheet for decomposition
    let spec = SpecSheet {
        id: Uuid::new_v4(),
        task_id: parent_swarm.task_id,
        overview: Some(parent_agent.subtask_description.clone()),
        requirements: None,
        acceptance_criteria: None,
        constraints: None,
        tech_notes: Some("This subtask needs to be broken into smaller pieces because previous agents could not complete it within their context window.".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let prompt = decomposer::generate_decomposition_prompt(&spec, &task.title);
    let working_dir_for_decompose = working_dir.clone();
    let decomp_output = tokio::task::spawn_blocking(move || {
        decomposer::run_decomposition(&prompt, &working_dir_for_decompose)
    })
    .await
    .unwrap_or_else(|_| {
        Err(DecomposerError::ExecutionFailed(
            "Decomposer panicked".into(),
        ))
    })?;

    let decomp_result = decomposer::parse_decomposition_output(&decomp_output)?;

    // Create the sub-swarm
    let sub_swarm = Swarm::create(
        pool,
        Uuid::new_v4(),
        parent_swarm.task_id,
        parent_swarm.workspace_id,
        Some(parent_agent.id), // Link to parent agent
        parent_swarm.depth + 1,
        parent_swarm.max_depth,
        parent_swarm.routing_decision.clone(),
    )
    .await?;

    // Create SwarmAgent records for each decomposed story
    let mut story_id_to_agent_id: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();

    for (i, story) in decomp_result.stories.iter().enumerate() {
        let subtask = format!("{}\n\n{}", story.title, story.description);
        let agent = SwarmAgent::create(
            pool,
            Uuid::new_v4(),
            sub_swarm.id,
            subtask,
            0,
            None,
            DEFAULT_CONTEXT_THRESHOLD,
            i as i64,
        )
        .await?;
        story_id_to_agent_id.insert(story.id.clone(), agent.id);
    }

    // Wire up dependencies from decomposition
    for story in &decomp_result.stories {
        let agent_id = story_id_to_agent_id[&story.id];
        let dep_agent_ids: Vec<Uuid> = story
            .depends_on
            .iter()
            .filter_map(|dep_id| story_id_to_agent_id.get(dep_id).copied())
            .collect();
        if !dep_agent_ids.is_empty() {
            SwarmAgentDependency::create_batch(pool, agent_id, &dep_agent_ids).await?;
        }
    }

    // Start the sub-swarm
    Swarm::update_status(pool, sub_swarm.id, SwarmStatus::Running).await?;
    spawn_all_eligible_agents(pool, container, sub_swarm.id, executor_profile_id).await?;

    tracing::info!(
        "Spawned sub-swarm {} (depth {}) for agent {} with {} sub-agents",
        sub_swarm.id,
        sub_swarm.depth,
        parent_agent.id,
        decomp_result.stories.len(),
    );

    Ok(())
}

/// Run the verifier using `claude --print` to evaluate the predecessor's work.
/// Accepts a model parameter for hybrid model selection (section 10.4).
fn run_verifier(
    subtask: &str,
    git_diff: &str,
    self_assessment: &str,
    working_dir: &Path,
    model: &str,
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

    let mut cmd = std::process::Command::new("claude");
    cmd.args(["--print", "-p", &prompt]);

    // Apply model selection (section 10.4)
    if model != DEFAULT_VERIFIER_MODEL {
        cmd.args(["--model", model]);
    } else {
        cmd.args(["--model", DEFAULT_VERIFIER_MODEL]);
    }

    let output = cmd.current_dir(working_dir).output().map_err(|e| {
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
        utils::text::find_json_object_start(json_str).unwrap_or(json_str)
    };

    serde_json::from_str(json_str).map_err(|e| {
        SwarmCoordinatorError::VerifierFailed(format!("Failed to parse verification report: {e}"))
    })
}

/// Mark a swarm as completed and handle any parent linkage (section 3.6).
async fn complete_swarm(
    pool: &SqlitePool,
    container: &(impl ContainerService + Sync + ?Sized),
    swarm_id: Uuid,
) -> Result<(), SwarmCoordinatorError> {
    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(SwarmCoordinatorError::SwarmNotFound(swarm_id))?;

    Swarm::update_status(pool, swarm_id, SwarmStatus::Completed).await?;

    tracing::info!("Swarm {} completed", swarm_id);

    let workspace = Workspace::find_by_id(pool, swarm.workspace_id)
        .await?
        .ok_or(SwarmCoordinatorError::WorkspaceNotFound(swarm.workspace_id))?;

    let aggregation_passed = run_aggregation_check(pool, &swarm, &workspace).await;

    if !aggregation_passed {
        // Aggregation found critical gaps — mark swarm as failed so it doesn't
        // silently pass incomplete work upstream.
        Swarm::update_status(pool, swarm_id, SwarmStatus::Failed).await?;
        tracing::warn!(
            "Swarm {} failed aggregation check — marked as failed",
            swarm_id,
        );
        return Ok(());
    }

    if let Some(parent_agent_id) = swarm.parent_agent_id {
        // Sub-swarm completed (section 3.6): mark the parent agent as completed
        // and advance the parent swarm.
        SwarmAgent::update_status(pool, parent_agent_id, SwarmAgentStatus::Completed).await?;

        let parent_agent = SwarmAgent::find_by_id(pool, parent_agent_id)
            .await?
            .ok_or(SwarmCoordinatorError::AgentNotFound(parent_agent_id))?;

        tracing::info!(
            "Sub-swarm {} completed, advancing parent swarm {}",
            swarm_id,
            parent_agent.swarm_id,
        );

        let parent_swarm = Swarm::find_by_id(pool, parent_agent.swarm_id)
            .await?
            .ok_or(SwarmCoordinatorError::SwarmNotFound(parent_agent.swarm_id))?;

        // Check if all parent swarm agents are complete
        if SwarmAgent::all_complete(pool, parent_swarm.id).await? {
            // Recursively complete the parent swarm
            Box::pin(complete_swarm(pool, container, parent_swarm.id)).await?;
        }
    } else {
        // Root swarm: transition parent task to QA
        super::ralph_loop::complete_ralph_loop(pool, swarm.task_id).await?;
    }

    Ok(())
}

/// Structured result from an aggregation check (section 3.6).
#[derive(Debug, Deserialize)]
struct AggregationResult {
    pub complete: bool,
    pub coverage_pct: u8,
    pub missing_items: Vec<String>,
    pub summary: String,
}

/// Run an aggregation check to verify completeness of all child agents' work (section 3.6).
/// Returns true if the aggregation passes, false if critical gaps were found.
async fn run_aggregation_check(pool: &SqlitePool, swarm: &Swarm, workspace: &Workspace) -> bool {
    let working_dir = if let Some(ref container_ref) = workspace.container_ref {
        std::path::PathBuf::from(container_ref)
    } else {
        std::path::PathBuf::from(".")
    };

    let agents = match SwarmAgent::find_by_swarm_id(pool, swarm.id).await {
        Ok(agents) => agents,
        Err(e) => {
            tracing::warn!("Failed to fetch agents for aggregation check: {}", e);
            return true; // Don't block on fetch failure
        }
    };

    let agent_summaries: Vec<String> = agents
        .iter()
        .filter(|a| a.status == SwarmAgentStatus::Completed)
        .enumerate()
        .map(|(i, a)| {
            format!(
                "Agent {} (gen {}): {}",
                i + 1,
                a.generation,
                a.subtask_description.chars().take(200).collect::<String>()
            )
        })
        .collect();

    if agent_summaries.is_empty() {
        return true;
    }

    let task = match Task::find_by_id(pool, swarm.task_id).await {
        Ok(Some(t)) => t,
        _ => return true,
    };

    // Get actual git diff for the aggregation check
    let working_dir_for_diff = working_dir.clone();
    let git_diff = tokio::task::spawn_blocking(move || get_git_diff_summary(&working_dir_for_diff))
        .await
        .unwrap_or_else(|_| "(git diff unavailable)".to_string());

    let task_desc = task.description.as_deref().unwrap_or("(no description)");

    let prompt = format!(
        r#"You are a result aggregation agent. Verify that the following agents' work covers the original task.

## Original Task
{title}

{task_desc}

## Agent Work Summary
{summaries}

## Git Diff
{git_diff}

## Instructions
Evaluate completeness. Respond with ONLY a JSON object:

{{"complete": true/false, "coverage_pct": 0-100, "missing_items": ["list of gaps"], "summary": "brief assessment"}}"#,
        title = task.title,
        task_desc = task_desc,
        summaries = agent_summaries.join("\n"),
        git_diff = git_diff,
    );

    let working_dir_clone = working_dir.clone();
    let verifier_model = swarm
        .verifier_model
        .clone()
        .unwrap_or_else(|| DEFAULT_VERIFIER_MODEL.to_string());

    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("claude")
            .args(["--print", "-p", &prompt, "--model", &verifier_model])
            .current_dir(&working_dir_clone)
            .output()
    })
    .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            match parse_aggregation_output(&stdout) {
                Ok(agg) => {
                    tracing::info!(
                        "Aggregation check for swarm {}: complete={}, coverage={}%, missing={:?}",
                        swarm.id,
                        agg.complete,
                        agg.coverage_pct,
                        agg.missing_items,
                    );

                    // Store the aggregation result as a log for audit trail
                    if let Ok(json) = serde_json::to_string(&serde_json::json!({
                        "complete": agg.complete,
                        "coverage_pct": agg.coverage_pct,
                        "missing_items": agg.missing_items,
                        "summary": agg.summary,
                    })) {
                        tracing::info!("Aggregation result for swarm {}: {}", swarm.id, json);
                    }

                    // Return false if coverage is critically low (<50%) to signal
                    // the swarm should not be marked as completed
                    if !agg.complete && agg.coverage_pct < 50 {
                        tracing::warn!(
                            "Aggregation check FAILED for swarm {}: only {}% coverage",
                            swarm.id,
                            agg.coverage_pct,
                        );
                        return false;
                    }

                    true
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse aggregation output for swarm {}: {}",
                        swarm.id,
                        e
                    );
                    true // Don't block on parse failure
                }
            }
        }
        _ => {
            tracing::warn!("Aggregation check command failed for swarm {}", swarm.id);
            true // Don't block on command failure
        }
    }
}

/// Parse the aggregation verifier output, tolerating markdown fences.
fn parse_aggregation_output(output: &str) -> Result<AggregationResult, SwarmCoordinatorError> {
    let trimmed = output.trim();

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

    let json_str = if json_str.starts_with('{') {
        json_str
    } else {
        utils::text::find_json_object_start(json_str).unwrap_or(json_str)
    };

    serde_json::from_str(json_str).map_err(|e| {
        SwarmCoordinatorError::VerifierFailed(format!("Failed to parse aggregation result: {e}"))
    })
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

/// Create a git branch for agent generation isolation (section 4.5, 7.4).
///
/// Uses `git branch` (not `git checkout -b`) to avoid switching the shared
/// working tree, which would cause race conditions when multiple agents run
/// concurrently in the same workspace. The agent's execution prompt instructs
/// it to `git checkout <branch>` as its first step, ensuring each agent
/// operates on its own branch without conflicting with siblings.
fn create_agent_git_branch(workspace: &Workspace, branch_name: &str) {
    let working_dir = if let Some(ref container_ref) = workspace.container_ref {
        std::path::PathBuf::from(container_ref)
    } else {
        return;
    };

    // Create the branch without switching to it (safe for concurrent agents)
    let result = std::process::Command::new("git")
        .args(["branch", branch_name])
        .current_dir(&working_dir)
        .output();

    match result {
        Ok(out) if out.status.success() => {
            tracing::debug!("Created git branch {} for agent", branch_name);
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("already exists") {
                tracing::debug!("Git branch {} already exists", branch_name);
            } else {
                tracing::warn!("Failed to create git branch {}: {}", branch_name, stderr);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to run git for branch {}: {}", branch_name, e);
        }
    }
}

/// Start the context monitor for a swarm agent execution, spawning a background
/// task that triggers succession on threshold.
fn start_context_monitor(
    pool: SqlitePool,
    agent_id: Uuid,
    execution_process_id: Uuid,
    msg_store: Arc<utils::msg_store::MsgStore>,
    threshold: f64,
    protocol_peer: Option<executors::executors::ProtocolPeer>,
) {
    let (_handle, rx) = ContextMonitor::watch(execution_process_id, msg_store, threshold);

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

                // Send graceful shutdown message via ProtocolPeer
                if let Some(peer) = protocol_peer {
                    let shutdown_msg =
                        "IMPORTANT: You have reached your context utilization threshold.\n\
                        Please wrap up your current work immediately:\n\
                        1. Save all changes (git add + commit)\n\
                        2. Write a brief self-assessment of what you completed and what remains\n\
                        3. Exit gracefully"
                            .to_string();
                    if let Err(e) = peer.send_user_message(shutdown_msg).await {
                        tracing::warn!(
                            "Failed to send shutdown message to agent {}: {}",
                            agent_id,
                            e
                        );
                    }
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
        let prompt = build_agent_prompt("Create a login page", None, None);
        assert!(prompt.contains("# Swarm Agent Task"));
        assert!(prompt.contains("Create a login page"));
        assert!(prompt.contains("## Execution Guidelines"));
        assert!(!prompt.contains("Predecessor Verification Report"));
        assert!(!prompt.contains("git checkout"));
    }

    #[test]
    fn test_build_agent_prompt_with_verification() {
        let report = r#"{"completed": ["Step 1"], "remaining": ["Step 2"]}"#;
        let prompt = build_agent_prompt("Create a login page", Some(report), None);
        assert!(prompt.contains("## Predecessor Verification Report"));
        assert!(prompt.contains("Step 1"));
    }

    #[test]
    fn test_build_agent_prompt_verification_report_capped() {
        let long_report = "x".repeat(15_000);
        let prompt = build_agent_prompt("task", Some(&long_report), None);
        // The report should be capped
        assert!(prompt.len() < 15_500);
    }

    #[test]
    fn test_build_agent_prompt_with_git_branch() {
        let prompt = build_agent_prompt("Implement auth", None, Some("swarm/abc-123/gen-0"));
        assert!(prompt.contains("git checkout swarm/abc-123/gen-0"));
        assert!(prompt.contains("FIRST"));
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
    fn test_determine_recovery_strategy_corrective() {
        let agent = make_test_agent(0);
        let report = VerificationReport {
            completed: vec!["step 1".into()],
            issues: vec![],
            remaining: vec!["step 2".into()],
            confidence: 0.6,
        };
        assert_eq!(determine_recovery_strategy(&agent, &report), "corrective");
    }

    #[test]
    fn test_determine_recovery_strategy_clean_restart() {
        let agent = make_test_agent(0);
        let report = VerificationReport {
            completed: vec![],
            issues: vec!["nothing worked".into()],
            remaining: vec!["everything".into()],
            confidence: 0.1,
        };
        assert_eq!(
            determine_recovery_strategy(&agent, &report),
            "clean_restart"
        );
    }

    #[test]
    fn test_determine_recovery_strategy_redecomposition() {
        let agent = make_test_agent(2); // 2 successions
        let report = VerificationReport {
            completed: vec![],
            issues: vec!["task too large".into()],
            remaining: vec!["most of it".into()],
            confidence: 0.3, // below threshold
        };
        assert_eq!(
            determine_recovery_strategy(&agent, &report),
            "redecomposition"
        );
    }

    #[test]
    fn test_determine_recovery_strategy_escalation() {
        let agent = make_test_agent(0);
        let report = VerificationReport {
            completed: vec![],
            issues: vec!["External blocker: API key missing".into()],
            remaining: vec!["everything".into()],
            confidence: 0.0,
        };
        assert_eq!(determine_recovery_strategy(&agent, &report), "escalation");
    }

    #[test]
    fn test_parse_aggregation_output_complete() {
        let output = r#"{"complete": true, "coverage_pct": 95, "missing_items": [], "summary": "All tasks covered"}"#;
        let result = parse_aggregation_output(output).unwrap();
        assert!(result.complete);
        assert_eq!(result.coverage_pct, 95);
        assert!(result.missing_items.is_empty());
    }

    #[test]
    fn test_parse_aggregation_output_incomplete() {
        let output = r#"{"complete": false, "coverage_pct": 40, "missing_items": ["auth module", "tests"], "summary": "Significant gaps"}"#;
        let result = parse_aggregation_output(output).unwrap();
        assert!(!result.complete);
        assert_eq!(result.coverage_pct, 40);
        assert_eq!(result.missing_items.len(), 2);
    }

    fn make_test_agent(succession_count: i64) -> SwarmAgent {
        SwarmAgent {
            id: Uuid::new_v4(),
            swarm_id: Uuid::new_v4(),
            execution_process_id: None,
            subtask_description: "test task".to_string(),
            generation: 1,
            predecessor_id: None,
            status: SwarmAgentStatus::Threshold,
            context_tokens_used: None,
            context_window_size: None,
            context_threshold: 0.6,
            sort_order: 0,
            git_branch: None,
            succession_count,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}
