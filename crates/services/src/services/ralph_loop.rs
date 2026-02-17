use db::models::{
    task::{Task, TaskStatus},
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use executors::profile::ExecutorProfileId;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

use crate::services::container::ContainerService;

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
        // Allow starting from Spec, Plan, or Ralph (multi-sprint: already in Ralph is fine)
        if task.status == TaskStatus::Plan
            || task.status == TaskStatus::Spec
            || task.status == TaskStatus::Ralph
        {
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

/// Advances a task from Ralph to InReview, indicating execution is complete.
pub async fn complete_ralph_loop(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
    if let Some(task) = Task::find_by_id(pool, task_id).await? {
        if task.status == TaskStatus::Ralph {
            Task::update_status(pool, task_id, TaskStatus::InReview).await?;
        }
    }
    Ok(())
}

/// Called when a child task's execution finishes.
/// Advances the Ralph loop to the next eligible child, or completes the loop.
pub async fn advance_ralph_loop<C: ContainerService + Sync + ?Sized>(
    pool: &SqlitePool,
    container: &C,
    child_task: &Task,
    executor_profile_id: &ExecutorProfileId,
    repos: &[WorkspaceRepoInput],
) -> Result<(), RalphLoopError> {
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
        return Ok(());
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
    } else if Task::all_children_done(pool, parent_workspace_id).await? {
        // This sprint's children are all done. Check if ALL children of the parent
        // (across all sprints) are done before completing.
        if Task::all_parent_children_done(pool, parent_task.id).await? {
            tracing::info!(
                "All children done across all sprints, completing Ralph loop for parent task {}",
                parent_task.id
            );
            complete_ralph_loop(pool, parent_task.id).await?;
        } else {
            tracing::info!(
                "Sprint workspace {} done, but parent {} has other active sprints",
                parent_workspace_id,
                parent_task.id
            );
        }
    } else {
        tracing::warn!(
            "No eligible children but not all done — possible dependency deadlock for parent task {}",
            parent_task.id
        );
        // Mark parent as InReview so user can intervene
        Task::update_status(pool, parent_task.id, TaskStatus::InReview).await?;
    }

    Ok(())
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
