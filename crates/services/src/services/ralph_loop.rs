use db::models::{
    ralph_session::{RalphSession, RalphSessionStatus, RalphSessionTask},
    task::{Task, TaskStatus},
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use executors::profile::ExecutorProfileId;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::services::container::ContainerService;

/// Result of advancing the Ralph loop after a child task completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceResult {
    /// A new child task was started.
    NextChildStarted,
    /// All tasks in the ralph session are complete.
    AllComplete,
    /// Advanced to the next parent task in the ralph session.
    NextSessionTaskStarted,
    /// No eligible children but not all done — possible dependency deadlock.
    Deadlocked,
}

#[derive(Debug, Error)]
pub enum RalphLoopError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Container(#[from] super::container::ContainerError),
    #[error(transparent)]
    Workspace(#[from] db::models::workspace::WorkspaceError),
    #[error("Parent workspace not found")]
    ParentWorkspaceNotFound,
    #[error("Parent task not found")]
    ParentTaskNotFound,
}

/// Repo input for creating workspace repos during child execution.
#[derive(Debug, Clone)]
pub struct WorkspaceRepoInput {
    pub repo_id: Uuid,
    pub target_branch: String,
}

/// The Ralph Loop service manages the execution lifecycle for a task.
/// It takes a generated plan and converts it into actionable execution steps.
///
/// The loop follows this pattern:
/// 1. Task moves to "ralph" status when execution begins
/// 2. The plan prompt is fed to the coding agent as the initial instruction
/// 3. On completion, the task auto-advances to "inreview"

/// Builds the execution prompt for the Ralph loop from a plan.
pub fn build_ralph_prompt(plan: &str, task_title: &str) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Execute Plan: {}\n\n", task_title));
    prompt.push_str("Implement the following plan. Work through each step carefully.\n\n");
    prompt.push_str(plan);
    prompt.push_str("\n\n## Execution Guidelines\n");
    prompt.push_str("- Complete each step in order\n");
    prompt.push_str("- Verify each change compiles/works before moving to the next\n");
    prompt.push_str("- Report any blockers or issues encountered\n");
    prompt.push_str("- When done, summarize all changes made\n");

    prompt
}

/// Advances a task to the Ralph status, indicating execution has started.
pub async fn start_ralph_loop(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
    if let Some(task) = Task::find_by_id(pool, task_id).await? {
        // Allow starting from Ready or Ralph (multi-sprint: already in Ralph is fine)
        if task.status == TaskStatus::Ready || task.status == TaskStatus::Ralph {
            Task::update_status(pool, task_id, TaskStatus::Ralph).await?;
        }
    }
    Ok(())
}

/// Checks if a task's parent workspace belongs to a Ralph-status task.
/// If so, returns that workspace so the child task can share the same worktree.
pub async fn find_shared_ralph_workspace(
    pool: &SqlitePool,
    task: &Task,
) -> Result<Option<Workspace>, sqlx::Error> {
    if let Some(parent_workspace_id) = task.parent_workspace_id {
        if let Some(parent_workspace) = Workspace::find_by_id(pool, parent_workspace_id).await? {
            if let Some(parent_task) = Task::find_by_id(pool, parent_workspace.task_id).await? {
                if parent_task.status == TaskStatus::Ralph {
                    return Ok(Some(parent_workspace));
                }
            }
        }
    }
    Ok(None)
}

/// Advances a task from Ralph to QA, indicating execution is complete.
pub async fn complete_ralph_loop(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
    if let Some(task) = Task::find_by_id(pool, task_id).await? {
        if task.status == TaskStatus::Ralph {
            Task::update_status(pool, task_id, TaskStatus::QA).await?;
        }
    }
    Ok(())
}

/// Called when a child task's execution finishes.
/// Advances the Ralph loop to the next eligible child, advances to the next
/// parent task in the ralph session, or completes the session.
pub async fn advance_ralph_loop<C: ContainerService + Sync + ?Sized>(
    pool: &SqlitePool,
    container: &C,
    child_task: &Task,
    executor_profile_id: &ExecutorProfileId,
    repos: &[WorkspaceRepoInput],
) -> Result<AdvanceResult, RalphLoopError> {
    let parent_workspace_id = child_task
        .parent_workspace_id
        .ok_or(RalphLoopError::ParentWorkspaceNotFound)?;

    let parent_workspace = Workspace::find_by_id(pool, parent_workspace_id)
        .await?
        .ok_or(RalphLoopError::ParentWorkspaceNotFound)?;

    let parent_task = Task::find_by_id(pool, parent_workspace.task_id)
        .await?
        .ok_or(RalphLoopError::ParentTaskNotFound)?;

    // Only advance if parent is still in Ralph status
    if parent_task.status != TaskStatus::Ralph {
        tracing::warn!(
            "Parent task {} is not in Ralph status ({}), skipping advance",
            parent_task.id,
            parent_task.status
        );
        return Ok(AdvanceResult::NextChildStarted);
    }

    // Mark the completed child as Done
    Task::update_status(pool, child_task.id, TaskStatus::Done).await?;

    let (done, total) = Task::count_children(pool, parent_workspace_id).await?;
    tracing::info!(
        "Ralph loop progress for parent {}: {}/{} children done",
        parent_task.id,
        done,
        total
    );

    // Try to start the next eligible child
    if let Some(next_child) = Task::find_next_eligible_child(pool, parent_workspace_id).await? {
        tracing::info!(
            "Starting next child task: {} ({})",
            next_child.title,
            next_child.id
        );
        start_child_execution(
            pool,
            container,
            &next_child,
            &parent_workspace,
            executor_profile_id,
            repos,
        )
        .await?;
        Ok(AdvanceResult::NextChildStarted)
    } else if Task::all_children_done(pool, parent_workspace_id).await? {
        // This sprint's children are all done. Check if ALL children of the parent
        // (across all sprints) are done before completing.
        if Task::all_parent_children_done(pool, parent_task.id).await? {
            tracing::info!(
                "All children done for parent task {}, checking ralph session for next task",
                parent_task.id
            );

            // Try to advance to the next parent task in the ralph session
            match advance_ralph_session(
                pool,
                container,
                &parent_task,
                &parent_workspace,
                executor_profile_id,
                repos,
            )
            .await?
            {
                SessionAdvanceResult::NextTaskStarted => Ok(AdvanceResult::NextSessionTaskStarted),
                SessionAdvanceResult::SessionComplete => {
                    // This was the last task — move it to QA for user review
                    complete_ralph_loop(pool, parent_task.id).await?;
                    Ok(AdvanceResult::AllComplete)
                }
                SessionAdvanceResult::NoSession => {
                    // Not part of a session — just complete this single task
                    complete_ralph_loop(pool, parent_task.id).await?;
                    Ok(AdvanceResult::AllComplete)
                }
            }
        } else {
            tracing::info!(
                "Sprint workspace {} done, but parent {} has other active sprints",
                parent_workspace_id,
                parent_task.id
            );
            Ok(AdvanceResult::NextChildStarted)
        }
    } else {
        tracing::warn!(
            "No eligible children but not all done — possible dependency deadlock for parent task {}",
            parent_task.id
        );
        // Mark parent as QA so user can intervene
        Task::update_status(pool, parent_task.id, TaskStatus::QA).await?;
        Ok(AdvanceResult::Deadlocked)
    }
}

/// Result of attempting to advance to the next task in a ralph session.
#[derive(Debug)]
enum SessionAdvanceResult {
    /// Started the next parent task's children in the session.
    NextTaskStarted,
    /// All tasks in the session are complete.
    SessionComplete,
    /// The parent task is not part of a ralph session.
    NoSession,
}

/// After a parent task's children all complete, check if there are more tasks
/// in the ralph session. If so, advance to the next one using the same worktree.
async fn advance_ralph_session<C: ContainerService + Sync + ?Sized>(
    pool: &SqlitePool,
    container: &C,
    completed_parent: &Task,
    parent_workspace: &Workspace,
    executor_profile_id: &ExecutorProfileId,
    repos: &[WorkspaceRepoInput],
) -> Result<SessionAdvanceResult, RalphLoopError> {
    // Find the ralph session this task belongs to
    let session = match RalphSession::find_active_by_task_id(pool, completed_parent.id).await? {
        Some(s) => s,
        None => return Ok(SessionAdvanceResult::NoSession),
    };

    tracing::info!(
        "Ralph session {}: parent task '{}' ({}) completed, checking for next task",
        session.id,
        completed_parent.title,
        completed_parent.id
    );

    // Find the next task in the session after the current one
    let next_session_task =
        RalphSessionTask::find_next_task_after(pool, session.id, completed_parent.id).await?;

    match next_session_task {
        None => {
            // This was the last task — session is complete.
            // Don't mark as Done here; let the caller move it to QA for PR review.
            tracing::info!("Ralph session {} complete — all tasks done", session.id);
            RalphSession::update_status(pool, session.id, RalphSessionStatus::Completed).await?;
            return Ok(SessionAdvanceResult::SessionComplete);
        }
        Some(ref _next) => {
            // Mark the completed parent as Done (no intermediate review needed)
            Task::update_status(pool, completed_parent.id, TaskStatus::Done).await?;
        }
    }

    let next_session_task = next_session_task.unwrap();

    let next_task = Task::find_by_id(pool, next_session_task.task_id)
        .await?
        .ok_or(RalphLoopError::ParentTaskNotFound)?;

    tracing::info!(
        "Ralph session {}: advancing to next task '{}' ({})",
        session.id,
        next_task.title,
        next_task.id
    );

    // Update the shared workspace to point to the new parent task
    Workspace::update_task_id(pool, parent_workspace.id, next_task.id).await?;

    // Assign the next task's children to the shared workspace
    let children = Task::find_by_parent_task_id(pool, next_task.id).await?;
    for child in &children {
        Task::update_parent_workspace_id(pool, child.id, Some(parent_workspace.id)).await?;
    }

    // Re-fetch workspace after update
    let workspace = Workspace::find_by_id(pool, parent_workspace.id)
        .await?
        .ok_or(RalphLoopError::ParentWorkspaceNotFound)?;

    // Start the first eligible child of the new parent task
    if let Some(next_child) = Task::find_next_eligible_child(pool, workspace.id).await? {
        tracing::info!(
            "Starting first child of next session task: {} ({})",
            next_child.title,
            next_child.id
        );
        start_child_execution(
            pool,
            container,
            &next_child,
            &workspace,
            executor_profile_id,
            repos,
        )
        .await?;
    } else {
        tracing::warn!(
            "Next session task '{}' has no eligible children to start",
            next_task.title
        );
    }

    Ok(SessionAdvanceResult::NextTaskStarted)
}

/// Start execution for a child task, reusing the parent's worktree.
pub async fn start_child_execution<C: ContainerService + Sync + ?Sized>(
    pool: &SqlitePool,
    container: &C,
    child_task: &Task,
    parent_workspace: &Workspace,
    executor_profile_id: &ExecutorProfileId,
    repos: &[WorkspaceRepoInput],
) -> Result<(), RalphLoopError> {
    tracing::debug!(
        child_task_id = %child_task.id,
        parent_workspace_id = %parent_workspace.id,
        "Starting child execution"
    );
    // Create workspace for the child task
    let workspace_id = Uuid::new_v4();
    let branch = parent_workspace.branch.clone();

    let workspace = Workspace::create(
        pool,
        &CreateWorkspace {
            branch,
            agent_working_dir: parent_workspace.agent_working_dir.clone(),
        },
        workspace_id,
        child_task.id,
    )
    .await?;

    // Create workspace repos
    let workspace_repos: Vec<CreateWorkspaceRepo> = repos
        .iter()
        .map(|r| CreateWorkspaceRepo {
            repo_id: r.repo_id,
            target_branch: r.target_branch.clone(),
        })
        .collect();
    WorkspaceRepo::create_many(pool, workspace.id, &workspace_repos).await?;

    // Share container ref from parent workspace
    if let Some(ref container_ref) = parent_workspace.container_ref {
        Workspace::update_container_ref(pool, workspace.id, container_ref).await?;
    }

    // Re-fetch workspace with updated container_ref
    let workspace = Workspace::find_by_id(pool, workspace.id)
        .await?
        .ok_or(RalphLoopError::ParentWorkspaceNotFound)?;

    // Start workspace execution
    container
        .start_workspace(&workspace, executor_profile_id.clone())
        .await?;

    tracing::info!(workspace_id = %workspace.id, "Child execution started");
    Ok(())
}

/// Convenience function: find and start the next eligible child.
/// Returns true if a child was started, false if no eligible child found.
pub async fn start_next_child<C: ContainerService + Sync + ?Sized>(
    pool: &SqlitePool,
    container: &C,
    parent_workspace_id: Uuid,
    executor_profile_id: &ExecutorProfileId,
    repos: &[WorkspaceRepoInput],
) -> Result<bool, RalphLoopError> {
    let parent_workspace = Workspace::find_by_id(pool, parent_workspace_id)
        .await?
        .ok_or(RalphLoopError::ParentWorkspaceNotFound)?;

    if let Some(next_child) = Task::find_next_eligible_child(pool, parent_workspace_id).await? {
        start_child_execution(
            pool,
            container,
            &next_child,
            &parent_workspace,
            executor_profile_id,
            repos,
        )
        .await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ralph_prompt() {
        let plan = "1. Create file A\n2. Modify file B\n3. Run tests";
        let prompt = build_ralph_prompt(plan, "My Feature");

        assert!(prompt.contains("# Execute Plan: My Feature"));
        assert!(prompt.contains("Create file A"));
        assert!(prompt.contains("## Execution Guidelines"));
    }
}
