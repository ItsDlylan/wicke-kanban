use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use async_trait::async_trait;
use command_group::AsyncGroupChild;
use db::{
    DBService,
    models::{
        coding_agent_turn::CodingAgentTurn,
        execution_process::{
            ExecutionContext, ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus,
        },
        execution_process_repo_state::ExecutionProcessRepoState,
        project::Project,
        project_repo::ProjectRepo,
        repo::Repo,
        scratch::{DraftFollowUpData, Scratch, ScratchType},
        session::{Session, SessionError},
        task::{Task, TaskStatus},
        workspace::Workspace,
        workspace_repo::WorkspaceRepo,
    },
};
use deployment::DeploymentError;
use executors::{
    actions::{
        Executable, ExecutorAction, ExecutorActionType,
        coding_agent_follow_up::CodingAgentFollowUpRequest,
        coding_agent_initial::CodingAgentInitialRequest,
    },
    approvals::{ExecutorApprovalService, NoopExecutorApprovalService},
    env::{ExecutionEnv, RepoContext},
    executors::{BaseCodingAgent, CancellationToken, ExecutorExitResult, ExecutorExitSignal},
    logs::{NormalizedEntryType, utils::patch::extract_normalized_entry_from_patch},
};
use futures::{FutureExt, TryStreamExt, stream::select};
use git::GitService;
use serde_json::json;
use services::services::{
    analytics::AnalyticsContext,
    approvals::{Approvals, executor_approvals::ExecutorApprovalBridge},
    auto_planner,
    config::{Config, DEFAULT_COMMIT_REMINDER_PROMPT},
    container::{ContainerError, ContainerRef, ContainerService},
    diff_stream::{self, DiffStreamHandle},
    image::ImageService,
    notification::NotificationService,
    queued_message::QueuedMessageService,
    workspace_manager::{RepoWorkspaceInput, WorkspaceManager},
};
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::io::ReaderStream;
use utils::{
    log_msg::LogMsg,
    msg_store::MsgStore,
    text::{git_branch_id, short_uuid, truncate_to_char_boundary},
};
use uuid::Uuid;

use crate::{command, copy};

const WORKSPACE_TOUCH_DEBOUNCE: Duration = Duration::from_mins(2);

#[derive(Clone)]
pub struct LocalContainerService {
    db: DBService,
    child_store: Arc<RwLock<HashMap<Uuid, Arc<RwLock<AsyncGroupChild>>>>>,
    cancellation_tokens: Arc<RwLock<HashMap<Uuid, CancellationToken>>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    /// Tracks background tasks that stream logs to the database.
    /// When stopping execution, we await these to ensure logs are fully persisted.
    db_stream_handles: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    exit_monitor_handles: Arc<RwLock<HashMap<Uuid, JoinHandle<()>>>>,
    workspace_touch_times: Arc<RwLock<HashMap<Uuid, Instant>>>,
    config: Arc<RwLock<Config>>,
    git: GitService,
    image_service: ImageService,
    analytics: Option<AnalyticsContext>,
    approvals: Approvals,
    queued_message_service: QueuedMessageService,
    notification_service: NotificationService,
}

impl LocalContainerService {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        db: DBService,
        msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
        config: Arc<RwLock<Config>>,
        git: GitService,
        image_service: ImageService,
        analytics: Option<AnalyticsContext>,
        approvals: Approvals,
        queued_message_service: QueuedMessageService,
    ) -> Self {
        let child_store = Arc::new(RwLock::new(HashMap::new()));
        let cancellation_tokens = Arc::new(RwLock::new(HashMap::new()));
        let db_stream_handles = Arc::new(RwLock::new(HashMap::new()));
        let exit_monitor_handles = Arc::new(RwLock::new(HashMap::new()));
        let workspace_touch_times = Arc::new(RwLock::new(HashMap::new()));
        let notification_service = NotificationService::new(config.clone());

        let container = LocalContainerService {
            db,
            child_store,
            cancellation_tokens,
            msg_stores,
            db_stream_handles,
            exit_monitor_handles,
            workspace_touch_times,
            config,
            git,
            image_service,
            analytics,
            approvals,
            queued_message_service,
            notification_service,
        };

        container.spawn_workspace_cleanup();

        container
    }

    pub async fn get_child_from_store(&self, id: &Uuid) -> Option<Arc<RwLock<AsyncGroupChild>>> {
        let map = self.child_store.read().await;
        map.get(id).cloned()
    }

    pub async fn add_child_to_store(&self, id: Uuid, exec: AsyncGroupChild) {
        let mut map = self.child_store.write().await;
        map.insert(id, Arc::new(RwLock::new(exec)));
    }

    pub async fn remove_child_from_store(&self, id: &Uuid) {
        let mut map = self.child_store.write().await;
        map.remove(id);
    }

    async fn add_cancellation_token(&self, id: Uuid, token: CancellationToken) {
        let mut map = self.cancellation_tokens.write().await;
        map.insert(id, token);
    }

    async fn take_cancellation_token(&self, id: &Uuid) -> Option<CancellationToken> {
        let mut map = self.cancellation_tokens.write().await;
        map.remove(id)
    }

    async fn add_db_stream_handle(&self, id: Uuid, handle: JoinHandle<()>) {
        let mut map = self.db_stream_handles.write().await;
        map.insert(id, handle);
    }

    async fn take_db_stream_handle(&self, id: &Uuid) -> Option<JoinHandle<()>> {
        let mut map = self.db_stream_handles.write().await;
        map.remove(id)
    }

    async fn add_exit_monitor_handle(&self, id: Uuid, handle: JoinHandle<()>) {
        let mut map = self.exit_monitor_handles.write().await;
        map.insert(id, handle);
    }

    async fn take_exit_monitor_handle(&self, id: &Uuid) -> Option<JoinHandle<()>> {
        let mut map = self.exit_monitor_handles.write().await;
        map.remove(id)
    }

    pub async fn cleanup_workspace(db: &DBService, workspace: &Workspace) {
        let Some(container_ref) = &workspace.container_ref else {
            return;
        };
        let workspace_dir = PathBuf::from(container_ref);

        // Hard deny-list: NEVER delete project repo directories, their parents,
        // or the user's home directory — regardless of any other checks.
        if Self::is_protected_path(db, &workspace_dir).await {
            tracing::error!(
                "BLOCKED: Refusing to delete protected path {} for workspace {} — \
                 path is a project repo directory, a parent of a repo, or a user home directory. \
                 Clearing container_ref only.",
                workspace_dir.display(),
                workspace.id,
            );
            let _ = Workspace::clear_container_ref(&db.pool, workspace.id).await;
            return;
        }

        // Safety guard: refuse to delete directories outside known worktree base paths.
        // This prevents catastrophic deletion of user project directories if container_ref
        // somehow points to a non-workspace path (e.g., from legacy records).
        if !Self::is_safe_workspace_dir(db, &workspace_dir).await {
            tracing::error!(
                "Refusing to delete workspace directory {} for workspace {} — \
                 path is not inside a known worktree base directory. \
                 Clearing container_ref only.",
                workspace_dir.display(),
                workspace.id,
            );
            let _ = Workspace::clear_container_ref(&db.pool, workspace.id).await;
            return;
        }

        let repositories = WorkspaceRepo::find_repos_for_workspace(&db.pool, workspace.id)
            .await
            .unwrap_or_default();

        if repositories.is_empty() {
            tracing::warn!(
                "No repositories found for workspace {}, cleaning up workspace directory only",
                workspace.id
            );
            if workspace_dir.exists()
                && let Err(e) = tokio::fs::remove_dir_all(&workspace_dir).await
            {
                tracing::warn!("Failed to remove workspace directory: {}", e);
            }
        } else {
            WorkspaceManager::cleanup_workspace(&workspace_dir, &repositories)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "Failed to clean up workspace for workspace {}: {}",
                        workspace.id,
                        e
                    );
                });
        }

        // Clear container_ref so this workspace won't be picked up again
        let _ = Workspace::clear_container_ref(&db.pool, workspace.id).await;
    }

    /// Check whether a workspace directory is inside a known worktree base path.
    /// Returns true if the path is safe to delete (i.e., it lives under a
    /// project-specific or global worktree directory).
    async fn is_safe_workspace_dir(db: &DBService, workspace_dir: &Path) -> bool {
        let mut known_bases = vec![WorkspaceManager::get_workspace_base_dir()];

        // Primary source: stored worktree_base_dir values from repos
        if let Ok(stored_dirs) = Repo::list_worktree_base_dirs(&db.pool).await {
            for dir in stored_dirs {
                known_bases.push(PathBuf::from(dir));
            }
        }

        // Fallback: compute for any projects whose repos may not be backfilled yet
        if let Ok(projects) = Project::find_all(&db.pool).await {
            for project in &projects {
                if let Ok(repos) = ProjectRepo::find_repos_for_project(&db.pool, project.id).await {
                    if let Some(primary_repo) = repos.first() {
                        if primary_repo.worktree_base_dir.is_none() {
                            known_bases.push(WorkspaceManager::get_project_workspace_base_dir(
                                &project.name,
                                Path::new(&primary_repo.path),
                            ));
                        }
                    }
                }
            }
        }

        is_path_under_known_bases(workspace_dir, &known_bases)
    }

    /// Hard deny-list: returns true if the path must NEVER be deleted.
    /// This catches project repo directories, parent directories of repos
    /// (e.g. ~/Desktop/Projects), and the user's home directory.
    async fn is_protected_path(db: &DBService, workspace_dir: &Path) -> bool {
        let mut repo_paths = Vec::new();
        if let Ok(projects) = Project::find_all(&db.pool).await {
            for project in &projects {
                if let Ok(repos) = ProjectRepo::find_repos_for_project(&db.pool, project.id).await {
                    for repo in &repos {
                        repo_paths.push(PathBuf::from(&repo.path));
                    }
                }
            }
        }
        is_path_protected(workspace_dir, &repo_paths)
    }

    pub async fn cleanup_expired_workspaces(db: &DBService) -> Result<(), DeploymentError> {
        if std::env::var("DISABLE_WORKTREE_CLEANUP").is_ok() {
            tracing::info!(
                "Expired workspace cleanup is disabled via DISABLE_WORKTREE_CLEANUP environment variable"
            );
            return Ok(());
        }

        let expired_workspaces = Workspace::find_expired_for_cleanup(&db.pool).await?;
        if expired_workspaces.is_empty() {
            tracing::debug!("No expired workspaces found");
            return Ok(());
        }
        tracing::info!(
            "Found {} expired workspaces to clean up",
            expired_workspaces.len()
        );
        for workspace in &expired_workspaces {
            Self::cleanup_workspace(db, workspace).await;
        }
        Ok(())
    }

    pub fn spawn_workspace_cleanup(&self) {
        let db = self.db.clone();
        let cleanup_expired = Self::cleanup_expired_workspaces;
        tokio::spawn(async move {
            WorkspaceManager::cleanup_orphan_workspaces(&db.pool).await;

            let mut cleanup_interval =
                tokio::time::interval(tokio::time::Duration::from_secs(1800)); // 30 minutes
            loop {
                cleanup_interval.tick().await;
                tracing::info!("Starting periodic workspace cleanup...");
                cleanup_expired(&db).await.unwrap_or_else(|e| {
                    tracing::error!("Failed to clean up expired workspaces: {}", e)
                });
            }
        });
    }

    /// Record the current HEAD commit for each repository as the "after" state.
    /// Errors are silently ignored since this runs after the main execution completes
    /// and failure should not block process finalization.
    async fn update_after_head_commits(&self, exec_id: Uuid) {
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, exec_id).await {
            let workspace_root = self.workspace_to_current_dir(&ctx.workspace);
            for repo in &ctx.repos {
                let repo_path = workspace_root.join(&repo.name);
                if let Ok(head) = self.git().get_head_info(&repo_path) {
                    let _ = ExecutionProcessRepoState::update_after_head_commit(
                        &self.db.pool,
                        exec_id,
                        repo.id,
                        &head.oid,
                    )
                    .await;
                }
            }
        }
    }

    /// Get the commit message based on the execution run reason.
    async fn get_commit_message(&self, ctx: &ExecutionContext) -> String {
        match ctx.execution_process.run_reason {
            ExecutionProcessRunReason::CodingAgent => {
                // Try to retrieve the task summary from the coding agent turn
                // otherwise fallback to default message
                match CodingAgentTurn::find_by_execution_process_id(
                    &self.db().pool,
                    ctx.execution_process.id,
                )
                .await
                {
                    Ok(Some(turn)) if turn.summary.is_some() => turn.summary.unwrap(),
                    Ok(_) => {
                        tracing::debug!(
                            "No summary found for execution process {}, using default message",
                            ctx.execution_process.id
                        );
                        format!(
                            "Commit changes from coding agent for workspace {}",
                            ctx.workspace.id
                        )
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Failed to retrieve summary for execution process {}: {}",
                            ctx.execution_process.id,
                            e
                        );
                        format!(
                            "Commit changes from coding agent for workspace {}",
                            ctx.workspace.id
                        )
                    }
                }
            }
            ExecutionProcessRunReason::CleanupScript => {
                format!("Cleanup script changes for workspace {}", ctx.workspace.id)
            }
            _ => format!(
                "Changes from execution process {}",
                ctx.execution_process.id
            ),
        }
    }

    /// Check which repos have uncommitted changes. Fails if any repo is inaccessible.
    fn check_repos_for_changes(
        &self,
        workspace_root: &Path,
        repos: &[Repo],
    ) -> Result<Vec<(Repo, PathBuf)>, ContainerError> {
        let git = GitService::new();
        let mut repos_with_changes = Vec::new();

        for repo in repos {
            let worktree_path = workspace_root.join(&repo.name);

            match git.get_worktree_status(&worktree_path) {
                Ok(ws) if !ws.entries.is_empty() => {
                    repos_with_changes.push((repo.clone(), worktree_path));
                }
                Ok(_) => {
                    tracing::debug!("No changes in repo '{}'", repo.name);
                }
                Err(e) => {
                    return Err(ContainerError::Other(anyhow!(
                        "Pre-flight check failed for repo '{}': {}",
                        repo.name,
                        e
                    )));
                }
            }
        }

        Ok(repos_with_changes)
    }

    async fn has_commits_from_execution(
        &self,
        ctx: &ExecutionContext,
    ) -> Result<bool, ContainerError> {
        let workspace_root = self.workspace_to_current_dir(&ctx.workspace);

        let repo_states = ExecutionProcessRepoState::find_by_execution_process_id(
            &self.db.pool,
            ctx.execution_process.id,
        )
        .await?;

        for repo in &ctx.repos {
            let repo_path = workspace_root.join(&repo.name);
            let current_head = self.git().get_head_info(&repo_path).ok().map(|h| h.oid);

            let before_head = repo_states
                .iter()
                .find(|s| s.repo_id == repo.id)
                .and_then(|s| s.before_head_commit.clone());

            if current_head != before_head {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Commit changes to each repo. Logs failures but continues with other repos.
    fn commit_repos(&self, repos_with_changes: Vec<(Repo, PathBuf)>, message: &str) -> bool {
        let mut any_committed = false;

        for (repo, worktree_path) in repos_with_changes {
            tracing::debug!(
                "Committing changes for repo '{}' at {:?}",
                repo.name,
                &worktree_path
            );

            match self.git().commit(&worktree_path, message) {
                Ok(true) => {
                    any_committed = true;
                    tracing::info!("Committed changes in repo '{}'", repo.name);
                }
                Ok(false) => {
                    tracing::warn!("No changes committed in repo '{}' (unexpected)", repo.name);
                }
                Err(e) => {
                    tracing::warn!("Failed to commit in repo '{}': {}", repo.name, e);
                }
            }
        }

        any_committed
    }

    /// Handle completion of an AutoPlan execution process.
    /// Extracts the plan from the MsgStore, stores it on the task, and spawns post-plan steps.
    async fn handle_auto_plan_completion(
        &self,
        ctx: &ExecutionContext,
        error_summary: Option<&str>,
    ) {
        let pool = &self.db.pool;
        let task_id = ctx.task.id;

        // Always attempt plan extraction first — the process may have been
        // intentionally killed after ExitPlanMode (stop_after_plan behaviour)
        // so the plan text is already in the logs even if exit status is non-zero.
        let plan_text = self
            .extract_plan_from_msg_store(&ctx.execution_process.id)
            .await;

        if plan_text.is_none() {
            let success = matches!(
                ctx.execution_process.status,
                ExecutionProcessStatus::Completed
            ) && ctx.execution_process.exit_code == Some(0);

            if !success {
                tracing::warn!(
                    "AutoPlan execution failed for task {} (status={:?}, exit_code={:?})",
                    task_id,
                    ctx.execution_process.status,
                    ctx.execution_process.exit_code
                );
                let failure_msg = match error_summary {
                    Some(summary) => format!("Plan generation failed: {}", summary),
                    None => "Plan generation failed".to_string(),
                };
                let _ = Task::update_plan(pool, task_id, &failure_msg, "failed").await;
                return;
            }
        }

        match plan_text {
            Some(plan) => {
                if let Err(e) = Task::update_plan(pool, task_id, &plan, "completed").await {
                    tracing::error!("Failed to store plan for task {}: {}", task_id, e);
                    return;
                }
                tracing::info!("AutoPlan: stored plan for task {}", task_id);

                // Get working directory from repos
                let working_dir = if let Some(repo) = ctx.repos.first() {
                    repo.path.to_string_lossy().to_string()
                } else {
                    tracing::warn!("AutoPlan: no repos found for task {}", task_id);
                    let _ = Task::update_status(pool, task_id, TaskStatus::Ready).await;
                    return;
                };

                // Spawn post-plan steps (spec generation + decomposition)
                let pool_clone = pool.clone();
                let project_id = ctx.task.project_id;
                let title = ctx.task.title.clone();
                let description = ctx.task.description.clone();
                tokio::spawn(async move {
                    auto_planner::auto_prepare_for_ralph(
                        &pool_clone,
                        task_id,
                        project_id,
                        &title,
                        description.as_deref(),
                        &plan,
                        Path::new(&working_dir),
                    )
                    .await;

                    if let Err(e) =
                        Task::update_status(&pool_clone, task_id, TaskStatus::Ready).await
                    {
                        tracing::error!(
                            "Failed to transition task {} to Ready status: {}",
                            task_id,
                            e
                        );
                    }
                    tracing::info!("AutoPlan: post-plan steps completed for task {}", task_id);
                });
            }
            None => {
                tracing::warn!("AutoPlan: no plan found in MsgStore for task {}", task_id);
                let _ = Task::update_plan(
                    pool,
                    task_id,
                    "Plan generation completed but no plan was extracted",
                    "failed",
                )
                .await;
            }
        }
    }

    /// Scan the MsgStore for an ExitPlanMode tool use and extract the plan text.
    async fn extract_plan_from_msg_store(&self, exec_id: &Uuid) -> Option<String> {
        let store = self.get_msg_store_by_id(exec_id).await?;
        let history = store.get_history();

        // MsgStore entries are raw byte chunks from ReaderStream, so a single
        // JSON line may be split across multiple LogMsg::Stdout entries.
        // Concatenate all stdout first, then split by newlines.
        let mut all_stdout = String::new();
        for msg in history.iter() {
            if let LogMsg::Stdout(text) = msg {
                all_stdout.push_str(text);
            }
        }

        // Scan lines in reverse (most recent first) for the ExitPlanMode tool call
        for line in all_stdout.lines().rev() {
            if !line.contains("ExitPlanMode") {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                // Claude Code emits ExitPlanMode as a top-level tool_use event:
                // {"type":"tool_use","name":"ExitPlanMode","input":{"plan":"..."}}
                if val.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                    && val.get("name").and_then(|n| n.as_str()) == Some("ExitPlanMode")
                {
                    if let Some(plan) = val
                        .get("input")
                        .and_then(|i| i.get("plan"))
                        .and_then(|p| p.as_str())
                    {
                        return Some(plan.to_string());
                    }
                }

                // Also check assistant message format (content array under message)
                // for backwards compatibility
                if let Some(content) = val
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for item in content {
                        if item.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                            && item.get("name").and_then(|n| n.as_str()) == Some("ExitPlanMode")
                        {
                            if let Some(plan) = item
                                .get("input")
                                .and_then(|i| i.get("plan"))
                                .and_then(|p| p.as_str())
                            {
                                return Some(plan.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Scan MsgStore history for result-type messages and extract error information.
    async fn extract_error_summary_from_msg_store(&self, exec_id: &Uuid) -> Option<String> {
        let store = self.get_msg_store_by_id(exec_id).await?;
        let history = store.get_history();

        // Concatenate all stdout chunks to handle lines split across entries
        let mut all_stdout = String::new();
        for msg in history.iter() {
            if let LogMsg::Stdout(text) = msg {
                all_stdout.push_str(text);
            }
        }

        let mut collected_errors: Vec<String> = Vec::new();

        // Scan in reverse for result messages containing errors
        for line in all_stdout.lines().rev() {
            if !line.contains("\"result\"") && !line.contains("\"type\":\"result\"") {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                // Check for "errors" array
                if let Some(errors) = val.get("errors").and_then(|e| e.as_array()) {
                    for err in errors {
                        if let Some(s) = err.as_str() {
                            collected_errors.push(s.to_string());
                        }
                    }
                }
                // Check for singular "error" string
                if let Some(error) = val.get("error").and_then(|e| e.as_str()) {
                    collected_errors.push(error.to_string());
                }
            }
            // Stop early once we have errors
            if !collected_errors.is_empty() {
                break;
            }
        }

        if collected_errors.is_empty() {
            return None;
        }

        Some(Self::categorize_error_messages(&collected_errors))
    }

    /// Categorize error messages into user-friendly summaries.
    fn categorize_error_messages(errors: &[String]) -> String {
        let joined = errors.join(" ");
        let lower = joined.to_lowercase();

        if lower.contains("429") || lower.contains("rate limit") {
            "API rate limit reached".to_string()
        } else if lower.contains("401") || lower.contains("403") || lower.contains("unauthorized") {
            "API authentication failed".to_string()
        } else if lower.contains("500") || lower.contains("502") || lower.contains("503") {
            "API server error".to_string()
        } else if lower.contains("econnrefused") || lower.contains("enotfound") {
            "Network connection error".to_string()
        } else if lower.contains("aborted")
            || lower.contains("timeout")
            || lower.contains("timed out")
        {
            "Request timed out".to_string()
        } else {
            // Fallback: first error truncated
            let first = &errors[0];
            if first.len() > 120 {
                format!("{}...", &first[..120])
            } else {
                first.clone()
            }
        }
    }

    /// Spawn a background task that polls the child process for completion and
    /// cleans up the execution entry when it exits.
    pub fn spawn_exit_monitor(
        &self,
        exec_id: &Uuid,
        exit_signal: Option<ExecutorExitSignal>,
    ) -> JoinHandle<()> {
        let exec_id = *exec_id;
        let child_store = self.child_store.clone();
        let msg_stores = self.msg_stores.clone();
        let db = self.db.clone();
        let config = self.config.clone();
        let container = self.clone();
        let analytics = self.analytics.clone();

        let mut process_exit_rx = self.spawn_os_exit_watcher(exec_id);

        tokio::spawn(async move {
            let mut exit_signal_future = exit_signal
                .map(|rx| rx.boxed()) // wait for result
                .unwrap_or_else(|| std::future::pending().boxed()); // no signal, stall forever

            let status_result: std::io::Result<std::process::ExitStatus>;

            // Wait for process to exit, or exit signal from executor
            tokio::select! {
                // Exit signal with result.
                // Some coding agent processes do not automatically exit after processing the user request; instead the executor
                // signals when processing has finished to gracefully kill the process.
                exit_result = &mut exit_signal_future => {
                    // Executor signaled completion: kill group and use the provided result
                    if let Some(child_lock) = child_store.read().await.get(&exec_id).cloned() {
                        let mut child = child_lock.write().await ;
                        if let Err(err) = command::kill_process_group(&mut child).await {
                            tracing::error!("Failed to kill process group after exit signal: {} {}", exec_id, err);
                        }
                    }

                    // Map the exit result to appropriate exit status
                    status_result = match exit_result {
                        Ok(ExecutorExitResult::Success) => Ok(success_exit_status()),
                        Ok(ExecutorExitResult::Failure) => Ok(failure_exit_status()),
                        Err(_) => Ok(success_exit_status()), // Channel closed, assume success
                    };
                }
                // Process exit
                exit_status_result = &mut process_exit_rx => {
                    status_result = exit_status_result.unwrap_or_else(|e| Err(std::io::Error::other(e)));
                }
            }

            let (exit_code, status) = match status_result {
                Ok(exit_status) => {
                    let code = exit_status.code().unwrap_or(-1) as i64;
                    let status = if exit_status.success() {
                        ExecutionProcessStatus::Completed
                    } else {
                        ExecutionProcessStatus::Failed
                    };
                    (Some(code), status)
                }
                Err(_) => (None, ExecutionProcessStatus::Failed),
            };

            let is_failed_or_killed = matches!(
                status,
                ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed
            );

            if !ExecutionProcess::was_stopped(&db.pool, exec_id).await
                && let Err(e) =
                    ExecutionProcess::update_completion(&db.pool, exec_id, status, exit_code).await
            {
                tracing::error!("Failed to update execution process completion: {}", e);
            }

            // Extract and store error summary for failed/killed processes
            let error_summary = if is_failed_or_killed {
                if let Some(summary) = container
                    .extract_error_summary_from_msg_store(&exec_id)
                    .await
                {
                    if let Err(e) =
                        ExecutionProcess::update_error_summary(&db.pool, exec_id, &summary).await
                    {
                        tracing::warn!("Failed to store error summary: {}", e);
                    }
                    Some(summary)
                } else {
                    None
                }
            } else {
                None
            };

            if let Ok(ctx) = ExecutionProcess::load_context(&db.pool, exec_id).await {
                // Update executor session summary if available
                if let Err(e) = container.update_executor_session_summary(&exec_id).await {
                    tracing::warn!("Failed to update executor session summary: {}", e);
                }

                // AutoPlan: extract plan and run post-plan steps, skip normal commit/finalize flow
                if matches!(
                    ctx.execution_process.run_reason,
                    ExecutionProcessRunReason::AutoPlan
                ) {
                    container
                        .handle_auto_plan_completion(&ctx, error_summary.as_deref())
                        .await;
                    // Clean up the isolated plan workspace directory
                    Self::cleanup_workspace(&container.db, &ctx.workspace).await;
                    // Fall through to MsgStore cleanup below
                    container.update_after_head_commits(exec_id).await;
                    let db_stream_handle = container.take_db_stream_handle(&exec_id).await;
                    if let Some(msg_arc) = msg_stores.write().await.remove(&exec_id) {
                        msg_arc.push_finished();
                    }
                    if let Some(handle) = db_stream_handle {
                        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
                    }
                    child_store.write().await.remove(&exec_id);
                    return;
                }

                let success = matches!(
                    ctx.execution_process.status,
                    ExecutionProcessStatus::Completed
                ) && exit_code == Some(0);

                let cleanup_done = matches!(
                    ctx.execution_process.run_reason,
                    ExecutionProcessRunReason::CleanupScript
                ) && !matches!(
                    ctx.execution_process.status,
                    ExecutionProcessStatus::Running
                );

                if success || cleanup_done {
                    // Commit changes (if any) and get feedback about whether changes were made
                    let changes_committed = match container.try_commit_changes(&ctx).await {
                        Ok(committed) => committed,
                        Err(e) => {
                            tracing::error!("Failed to commit changes after execution: {}", e);
                            // Treat commit failures as if changes were made to be safe
                            true
                        }
                    };

                    let should_start_next = if matches!(
                        ctx.execution_process.run_reason,
                        ExecutionProcessRunReason::CodingAgent
                    ) {
                        // Check if agent made commits OR if we just committed uncommitted changes
                        changes_committed
                            || container
                                .has_commits_from_execution(&ctx)
                                .await
                                .unwrap_or(false)
                    } else {
                        true
                    };

                    if should_start_next {
                        // If the process exited successfully, start the next action
                        if let Err(e) = container.try_start_next_action(&ctx).await {
                            tracing::error!("Failed to start next action after completion: {}", e);
                        }
                    } else {
                        tracing::info!(
                            "Skipping cleanup script for workspace {} - no changes made by coding agent",
                            ctx.workspace.id
                        );

                        // Manually finalize task since we're bypassing normal execution flow
                        container.finalize_task(&ctx).await;
                    }
                }

                if container.should_finalize(&ctx) {
                    // Only execute queued messages if the execution succeeded
                    // If it failed or was killed, just clear the queue and finalize
                    let should_execute_queued = !matches!(
                        ctx.execution_process.status,
                        ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed
                    );

                    if let Some(queued_msg) =
                        container.queued_message_service.take_queued(ctx.session.id)
                    {
                        if should_execute_queued {
                            tracing::info!(
                                "Found queued message for session {}, starting follow-up execution",
                                ctx.session.id
                            );

                            // Delete the scratch since we're consuming the queued message
                            if let Err(e) = Scratch::delete(
                                &db.pool,
                                ctx.session.id,
                                &ScratchType::DraftFollowUp,
                            )
                            .await
                            {
                                tracing::warn!(
                                    "Failed to delete scratch after consuming queued message: {}",
                                    e
                                );
                            }

                            // Execute the queued follow-up
                            if let Err(e) = container
                                .start_queued_follow_up(&ctx, &queued_msg.data)
                                .await
                            {
                                tracing::error!("Failed to start queued follow-up: {}", e);
                                // Fall back to finalization if follow-up fails
                                container.finalize_task(&ctx).await;
                            }
                        } else {
                            // Execution failed or was killed - discard the queued message and finalize
                            tracing::info!(
                                "Discarding queued message for session {} due to execution status {:?}",
                                ctx.session.id,
                                ctx.execution_process.status
                            );
                            container.finalize_task(&ctx).await;
                        }
                    } else {
                        container.finalize_task(&ctx).await;
                    }
                }

                // Fire analytics event when CodingAgent execution has finished
                if config.read().await.analytics_enabled
                    && matches!(
                        &ctx.execution_process.run_reason,
                        ExecutionProcessRunReason::CodingAgent
                    )
                    && let Some(analytics) = &analytics
                {
                    analytics.analytics_service.track_event(&analytics.user_id, "task_attempt_finished", Some(json!({
                        "task_id": ctx.task.id.to_string(),
                        "project_id": ctx.task.project_id.to_string(),
                        "workspace_id": ctx.workspace.id.to_string(),
                        "session_id": ctx.session.id.to_string(),
                        "execution_success": matches!(ctx.execution_process.status, ExecutionProcessStatus::Completed),
                        "exit_code": ctx.execution_process.exit_code,
                    })));
                }
            }

            // Now that commit/next-action/finalization steps for this process are complete,
            // capture the HEAD OID as the definitive "after" state (best-effort).
            container.update_after_head_commits(exec_id).await;

            // Wait for DB persistence to complete before cleaning up MsgStore
            let db_stream_handle = container.take_db_stream_handle(&exec_id).await;
            if let Some(msg_arc) = msg_stores.write().await.remove(&exec_id) {
                msg_arc.push_finished();
            }
            if let Some(handle) = db_stream_handle {
                let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
            }

            // Cleanup child handle
            child_store.write().await.remove(&exec_id);
        })
    }

    pub fn spawn_os_exit_watcher(
        &self,
        exec_id: Uuid,
    ) -> tokio::sync::oneshot::Receiver<std::io::Result<std::process::ExitStatus>> {
        let (tx, rx) = tokio::sync::oneshot::channel::<std::io::Result<std::process::ExitStatus>>();
        let child_store = self.child_store.clone();
        tokio::spawn(async move {
            loop {
                let child_lock = {
                    let map = child_store.read().await;
                    map.get(&exec_id).cloned()
                };
                if let Some(child_lock) = child_lock {
                    let mut child_handler = child_lock.write().await;
                    match child_handler.try_wait() {
                        Ok(Some(status)) => {
                            let _ = tx.send(Ok(status));
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            break;
                        }
                    }
                } else {
                    let _ = tx.send(Err(io::Error::other(format!(
                        "Child handle missing for {exec_id}"
                    ))));
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
        rx
    }

    pub fn dir_name_from_workspace(workspace_id: &Uuid, task_title: &str) -> String {
        let task_title_id = git_branch_id(task_title);
        format!("{}-{}", short_uuid(workspace_id), task_title_id)
    }

    async fn track_child_msgs_in_store(&self, id: Uuid, child: &mut AsyncGroupChild) {
        let store = Arc::new(MsgStore::new());

        let out = child.inner().stdout.take().expect("no stdout");
        let err = child.inner().stderr.take().expect("no stderr");

        // Map stdout bytes -> LogMsg::Stdout
        let out = ReaderStream::new(out)
            .map_ok(|chunk| LogMsg::Stdout(String::from_utf8_lossy(&chunk).into_owned()));

        // Map stderr bytes -> LogMsg::Stderr
        let err = ReaderStream::new(err)
            .map_ok(|chunk| LogMsg::Stderr(String::from_utf8_lossy(&chunk).into_owned()));

        // If you have a JSON Patch source, map it to LogMsg::JsonPatch too, then select all three.

        // Merge and forward into the store
        let merged = select(out, err); // Stream<Item = Result<LogMsg, io::Error>>
        store.clone().spawn_forwarder(merged);

        let mut map = self.msg_stores().write().await;
        map.insert(id, store);
    }

    /// Create a live diff log stream for ongoing attempts for WebSocket
    /// Returns a stream that owns the filesystem watcher - when dropped, watcher is cleaned up
    async fn create_live_diff_stream(
        &self,
        args: diff_stream::DiffStreamArgs,
    ) -> Result<DiffStreamHandle, ContainerError> {
        diff_stream::create(args)
            .await
            .map_err(|e| ContainerError::Other(anyhow!("{e}")))
    }

    /// Extract the last assistant message from the MsgStore history
    fn extract_last_assistant_message(&self, exec_id: &Uuid) -> Option<String> {
        // Get the MsgStore for this execution
        let msg_stores = self.msg_stores.try_read().ok()?;
        let msg_store = msg_stores.get(exec_id)?;

        // Get the history and scan in reverse for the last assistant message
        let history = msg_store.get_history();

        for msg in history.iter().rev() {
            if let LogMsg::JsonPatch(patch) = msg {
                // Try to extract a NormalizedEntry from the patch
                if let Some((_, entry)) = extract_normalized_entry_from_patch(patch)
                    && matches!(entry.entry_type, NormalizedEntryType::AssistantMessage)
                {
                    let content = entry.content.trim();
                    if !content.is_empty() {
                        const MAX_SUMMARY_LENGTH: usize = 4096;
                        if content.len() > MAX_SUMMARY_LENGTH {
                            let truncated = truncate_to_char_boundary(content, MAX_SUMMARY_LENGTH);
                            return Some(format!("{truncated}..."));
                        }
                        return Some(content.to_string());
                    }
                }
            }
        }

        None
    }

    /// Update the coding agent turn summary with the final assistant message
    async fn update_executor_session_summary(&self, exec_id: &Uuid) -> Result<(), anyhow::Error> {
        // Check if there's a coding agent turn for this execution process
        let turn = CodingAgentTurn::find_by_execution_process_id(&self.db.pool, *exec_id).await?;

        if let Some(turn) = turn {
            // Only update if summary is not already set
            if turn.summary.is_none() {
                if let Some(summary) = self.extract_last_assistant_message(exec_id) {
                    CodingAgentTurn::update_summary(&self.db.pool, *exec_id, &summary).await?;
                } else {
                    tracing::debug!("No assistant message found for execution {}", exec_id);
                }
            }
        }

        Ok(())
    }

    /// Copy project files and images to the workspace.
    /// Skips files/images that already exist (fast no-op if all exist).
    async fn copy_files_and_images(
        &self,
        workspace_dir: &Path,
        workspace: &Workspace,
    ) -> Result<(), ContainerError> {
        let repos = WorkspaceRepo::find_repos_with_copy_files(&self.db.pool, workspace.id).await?;

        for repo in &repos {
            if let Some(copy_files) = &repo.copy_files
                && !copy_files.trim().is_empty()
            {
                let worktree_path = workspace_dir.join(&repo.name);
                self.copy_project_files(&repo.path, &worktree_path, copy_files)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            "Failed to copy project files for repo '{}': {}",
                            repo.name,
                            e
                        );
                    });
            }
        }

        if let Err(e) = self
            .image_service
            .copy_images_by_task_to_worktree(
                workspace_dir,
                workspace.task_id,
                workspace.agent_working_dir.as_deref(),
            )
            .await
        {
            tracing::warn!("Failed to copy task images to workspace: {}", e);
        }

        Ok(())
    }

    /// Create workspace-level CLAUDE.md and AGENTS.md files that import from each repo.
    /// Uses the @import syntax to reference each repo's config files.
    /// Skips creating files if they already exist or if no repos have the source file.
    async fn create_workspace_config_files(
        workspace_dir: &Path,
        repos: &[Repo],
    ) -> Result<(), ContainerError> {
        const CONFIG_FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

        for config_file in CONFIG_FILES {
            let workspace_config_path = workspace_dir.join(config_file);

            if workspace_config_path.exists() {
                tracing::trace!(
                    "Workspace config file {} already exists, skipping",
                    config_file
                );
                continue;
            }

            let mut import_lines = Vec::new();
            for repo in repos {
                let repo_config_path = workspace_dir.join(&repo.name).join(config_file);
                if repo_config_path.exists() {
                    import_lines.push(format!("@{}/{}", repo.name, config_file));
                }
            }

            if import_lines.is_empty() {
                tracing::trace!(
                    "No repos have {}, skipping workspace config creation",
                    config_file
                );
                continue;
            }

            let content = import_lines.join("\n") + "\n";
            if let Err(e) = tokio::fs::write(&workspace_config_path, &content).await {
                tracing::warn!(
                    "Failed to create workspace config file {}: {}",
                    config_file,
                    e
                );
                continue;
            }

            tracing::info!(
                "Created workspace {} with {} import(s)",
                config_file,
                import_lines.len()
            );
        }

        Ok(())
    }

    /// Start a follow-up execution from a queued message
    async fn start_queued_follow_up(
        &self,
        ctx: &ExecutionContext,
        queued_data: &DraftFollowUpData,
    ) -> Result<ExecutionProcess, ContainerError> {
        let executor_profile_id = queued_data.executor_profile_id.clone();

        // Validate executor matches session if session has prior executions
        let expected_executor: Option<String> =
            ExecutionProcess::latest_executor_profile_for_session(&self.db.pool, ctx.session.id)
                .await?
                .map(|profile| profile.executor.to_string())
                .or_else(|| ctx.session.executor.clone());

        if let Some(expected) = expected_executor {
            let actual = executor_profile_id.executor.to_string();
            if expected != actual {
                return Err(SessionError::ExecutorMismatch { expected, actual }.into());
            }
        }

        if ctx.session.executor.is_none() {
            Session::update_executor(
                &self.db.pool,
                ctx.session.id,
                &executor_profile_id.executor.to_string(),
            )
            .await?;
        }

        // Get latest agent turn for session continuity (from coding agent turns)
        let latest_session_info =
            CodingAgentTurn::find_latest_session_info(&self.db.pool, ctx.session.id).await?;

        let repos =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, ctx.workspace.id).await?;
        let cleanup_action = self.cleanup_actions_for_repos(&repos);

        let working_dir = ctx
            .workspace
            .agent_working_dir
            .as_ref()
            .filter(|dir| !dir.is_empty())
            .cloned();

        let action_type = if let Some(info) = latest_session_info {
            ExecutorActionType::CodingAgentFollowUpRequest(CodingAgentFollowUpRequest {
                prompt: queued_data.message.clone(),
                session_id: info.session_id,
                reset_to_message_id: None,
                executor_profile_id: executor_profile_id.clone(),
                working_dir: working_dir.clone(),
            })
        } else {
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: queued_data.message.clone(),
                executor_profile_id: executor_profile_id.clone(),
                working_dir,
            })
        };

        let action = ExecutorAction::new(action_type, cleanup_action.map(Box::new));

        self.start_execution(
            &ctx.workspace,
            &ctx.session,
            &action,
            &ExecutionProcessRunReason::CodingAgent,
        )
        .await
    }
}

fn failure_exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatusExt::from_raw(256) // Exit code 1 (shifted by 8 bits)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatusExt::from_raw(1)
    }
}

#[async_trait]
impl ContainerService for LocalContainerService {
    fn msg_stores(&self) -> &Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>> {
        &self.msg_stores
    }

    fn db(&self) -> &DBService {
        &self.db
    }

    fn git(&self) -> &GitService {
        &self.git
    }

    fn notification_service(&self) -> &NotificationService {
        &self.notification_service
    }

    async fn touch(&self, workspace: &Workspace) -> Result<(), ContainerError> {
        let now = Instant::now();

        // We debounce touches to avoid excessive database writes, which in SQLites causes DB locks
        let should_debounce = |last_touch: &Instant| -> bool {
            now.duration_since(*last_touch) < WORKSPACE_TOUCH_DEBOUNCE
        };

        // Quick check with read lock
        if self
            .workspace_touch_times
            .read()
            .await
            .get(&workspace.id)
            .is_some_and(should_debounce)
        {
            return Ok(());
        }

        let mut map = self.workspace_touch_times.write().await;
        // Clean up stale entries older than the debounce window, reduce memory usage over time
        map.retain(|_, time| should_debounce(time));
        // check in case another thread has touched already
        if map.get(&workspace.id).is_some_and(should_debounce) {
            return Ok(());
        }
        map.insert(workspace.id, now);
        drop(map);

        Workspace::touch(&self.db.pool, workspace.id).await?;
        Ok(())
    }

    async fn store_db_stream_handle(&self, id: Uuid, handle: JoinHandle<()>) {
        self.add_db_stream_handle(id, handle).await;
    }

    async fn take_db_stream_handle(&self, id: &Uuid) -> Option<JoinHandle<()>> {
        LocalContainerService::take_db_stream_handle(self, id).await
    }

    async fn git_branch_prefix(&self) -> String {
        self.config.read().await.git_branch_prefix.clone()
    }

    fn workspace_to_current_dir(&self, workspace: &Workspace) -> PathBuf {
        PathBuf::from(workspace.container_ref.clone().unwrap_or_default())
    }

    async fn create(&self, workspace: &Workspace) -> Result<ContainerRef, ContainerError> {
        // If container_ref is already set (e.g. shared Ralph workspace),
        // just verify it exists and return it.
        if let Some(ref container_ref) = workspace.container_ref {
            let workspace_dir = PathBuf::from(container_ref);
            let repositories =
                WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;
            // Skip worktree creation if the workspace points directly at the original repo
            // or uses symlinks (e.g. auto-plan workspaces)
            let is_direct_repo = repositories.len() == 1
                && workspace_dir.join(&repositories[0].name) == repositories[0].path;
            let has_symlinked_repos = repositories
                .iter()
                .any(|repo| workspace_dir.join(&repo.name).is_symlink());
            if !repositories.is_empty() && !is_direct_repo && !has_symlinked_repos {
                WorkspaceManager::ensure_workspace_exists(
                    &workspace_dir,
                    &repositories,
                    &workspace.branch,
                )
                .await?;
            }
            return Ok(container_ref.clone());
        }

        let task = workspace
            .parent_task(&self.db.pool)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        let workspace_dir_name =
            LocalContainerService::dir_name_from_workspace(&workspace.id, &task.title);

        let project = Project::find_by_id(&self.db.pool, task.project_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        let repos = ProjectRepo::find_repos_for_project(&self.db.pool, task.project_id).await?;
        let primary_repo = repos.first().ok_or_else(|| {
            ContainerError::Other(anyhow!("Project has no repositories configured"))
        })?;
        let base_dir =
            WorkspaceManager::get_project_workspace_base_dir(&project.name, &primary_repo.path);
        let workspace_dir = base_dir.join(&workspace_dir_name);

        // Backfill worktree_base_dir if not yet set on the primary repo
        if primary_repo.worktree_base_dir.is_none() {
            if let Err(e) = Repo::set_worktree_base_dir(
                &self.db.pool,
                primary_repo.id,
                &base_dir.to_string_lossy(),
            )
            .await
            {
                tracing::warn!(
                    "Failed to set worktree_base_dir for repo {}: {}",
                    primary_repo.id,
                    e
                );
            }
        }

        let workspace_repos =
            WorkspaceRepo::find_by_workspace_id(&self.db.pool, workspace.id).await?;
        if workspace_repos.is_empty() {
            return Err(ContainerError::Other(anyhow!(
                "Workspace has no repositories configured"
            )));
        }

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        let target_branches: HashMap<_, _> = workspace_repos
            .iter()
            .map(|wr| (wr.repo_id, wr.target_branch.clone()))
            .collect();

        let workspace_inputs: Vec<RepoWorkspaceInput> = repositories
            .iter()
            .map(|repo| {
                let target_branch = target_branches.get(&repo.id).cloned().unwrap_or_default();
                RepoWorkspaceInput::new(repo.clone(), target_branch)
            })
            .collect();

        let created_workspace = WorkspaceManager::create_workspace(
            &workspace_dir,
            &workspace_inputs,
            &workspace.branch,
        )
        .await?;

        // Copy project files and images to workspace
        self.copy_files_and_images(&created_workspace.workspace_dir, workspace)
            .await?;

        Self::create_workspace_config_files(&created_workspace.workspace_dir, &repositories)
            .await?;

        Workspace::update_container_ref(
            &self.db.pool,
            workspace.id,
            &created_workspace.workspace_dir.to_string_lossy(),
        )
        .await?;

        Ok(created_workspace
            .workspace_dir
            .to_string_lossy()
            .to_string())
    }

    async fn delete(&self, workspace: &Workspace) -> Result<(), ContainerError> {
        self.try_stop(workspace, true).await;
        Self::cleanup_workspace(&self.db, workspace).await;
        Ok(())
    }

    async fn ensure_container_exists(
        &self,
        workspace: &Workspace,
    ) -> Result<ContainerRef, ContainerError> {
        self.touch(workspace).await?;
        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        if repositories.is_empty() {
            return Err(ContainerError::Other(anyhow!(
                "Workspace has no repositories configured"
            )));
        }

        let workspace_dir = if let Some(container_ref) = &workspace.container_ref {
            PathBuf::from(container_ref)
        } else {
            let task = workspace
                .parent_task(&self.db.pool)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let workspace_dir_name =
                LocalContainerService::dir_name_from_workspace(&workspace.id, &task.title);

            let project = Project::find_by_id(&self.db.pool, task.project_id)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            let project_repos =
                ProjectRepo::find_repos_for_project(&self.db.pool, task.project_id).await?;
            let primary_repo = project_repos.first().ok_or_else(|| {
                ContainerError::Other(anyhow!("Project has no repositories configured"))
            })?;
            WorkspaceManager::get_project_workspace_base_dir(&project.name, &primary_repo.path)
                .join(&workspace_dir_name)
        };

        // Skip worktree creation if the workspace points directly at the original repo
        // or uses symlinks (e.g., auto-plan workspaces that symlink repos without a git worktree).
        let is_direct_repo = repositories.len() == 1
            && workspace_dir.join(&repositories[0].name) == repositories[0].path;
        let has_symlinked_repos = repositories
            .iter()
            .any(|repo| workspace_dir.join(&repo.name).is_symlink());
        if !is_direct_repo && !has_symlinked_repos {
            WorkspaceManager::ensure_workspace_exists(
                &workspace_dir,
                &repositories,
                &workspace.branch,
            )
            .await?;
        }

        if workspace.container_ref.is_none() {
            Workspace::update_container_ref(
                &self.db.pool,
                workspace.id,
                &workspace_dir.to_string_lossy(),
            )
            .await?;
        }

        if !is_direct_repo {
            // Copy project files and images (fast no-op if already exist)
            self.copy_files_and_images(&workspace_dir, workspace)
                .await?;

            Self::create_workspace_config_files(&workspace_dir, &repositories).await?;
        }

        Ok(workspace_dir.to_string_lossy().to_string())
    }

    async fn is_container_clean(&self, workspace: &Workspace) -> Result<bool, ContainerError> {
        let Some(container_ref) = &workspace.container_ref else {
            return Ok(true);
        };

        let workspace_dir = PathBuf::from(container_ref);
        if !workspace_dir.exists() {
            return Ok(true);
        }

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        for repo in &repositories {
            let worktree_path = workspace_dir.join(&repo.name);
            if worktree_path.exists() && !self.git().is_worktree_clean(&worktree_path)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn start_execution_inner(
        &self,
        workspace: &Workspace,
        execution_process: &ExecutionProcess,
        executor_action: &ExecutorAction,
    ) -> Result<(), ContainerError> {
        // Get the worktree path
        let container_ref = workspace
            .container_ref
            .as_ref()
            .ok_or(ContainerError::Other(anyhow!(
                "Container ref not found for workspace"
            )))?;
        let current_dir = PathBuf::from(container_ref);

        let approvals_service: Arc<dyn ExecutorApprovalService> =
            match executor_action.base_executor() {
                Some(
                    BaseCodingAgent::Codex | BaseCodingAgent::ClaudeCode | BaseCodingAgent::Gemini,
                ) => ExecutorApprovalBridge::new(
                    self.approvals.clone(),
                    self.db.clone(),
                    self.notification_service.clone(),
                    execution_process.id,
                ),
                _ => Arc::new(NoopExecutorApprovalService {}),
            };

        let repos = WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;
        let repo_names: Vec<String> = repos.iter().map(|r| r.name.clone()).collect();
        let repo_context = RepoContext::new(current_dir.clone(), repo_names);

        let config = self.config.read().await;
        let commit_reminder_enabled = config.commit_reminder_enabled;
        let commit_reminder_prompt = config
            .commit_reminder_prompt
            .clone()
            .unwrap_or_else(|| DEFAULT_COMMIT_REMINDER_PROMPT.to_string());
        drop(config);
        let mut env = ExecutionEnv::new(
            repo_context,
            commit_reminder_enabled,
            commit_reminder_prompt,
        );

        // Always inject workspace/session context
        env.insert("VK_WORKSPACE_ID", workspace.id.to_string());
        env.insert("VK_WORKSPACE_BRANCH", &workspace.branch);
        env.insert("VK_SESSION_ID", execution_process.session_id.to_string());

        // Create the child and stream, add to execution tracker with timeout
        let mut spawned = tokio::time::timeout(
            Duration::from_secs(30),
            executor_action.spawn(&current_dir, approvals_service, &env),
        )
        .await
        .map_err(|_| {
            ContainerError::Other(anyhow!(
                "Timeout: process took more than 30 seconds to start"
            ))
        })??;

        self.track_child_msgs_in_store(execution_process.id, &mut spawned.child)
            .await;

        self.add_child_to_store(execution_process.id, spawned.child)
            .await;

        // Store cancellation token for graceful shutdown
        if let Some(cancel) = spawned.cancel {
            self.add_cancellation_token(execution_process.id, cancel)
                .await;
        }

        // Spawn unified exit monitor: watches OS exit and optional executor signal
        let hn = self.spawn_exit_monitor(&execution_process.id, spawned.exit_signal);
        self.add_exit_monitor_handle(execution_process.id, hn).await;

        Ok(())
    }

    async fn stop_execution(
        &self,
        execution_process: &ExecutionProcess,
        status: ExecutionProcessStatus,
    ) -> Result<(), ContainerError> {
        let child = self
            .get_child_from_store(&execution_process.id)
            .await
            .ok_or_else(|| {
                ContainerError::Other(anyhow!("Child process not found for execution"))
            })?;
        let exit_code = if status == ExecutionProcessStatus::Completed {
            Some(0)
        } else {
            None
        };

        ExecutionProcess::update_completion(&self.db.pool, execution_process.id, status, exit_code)
            .await?;

        // Try graceful cancellation first, then force kill
        if let Some(cancel) = self.take_cancellation_token(&execution_process.id).await {
            cancel.cancel();

            // Wait for exit monitor to finish gracefully
            if let Some(monitor_handle) = self.take_exit_monitor_handle(&execution_process.id).await
            {
                match tokio::time::timeout(Duration::from_secs(5), monitor_handle).await {
                    Ok(_) => {
                        tracing::debug!("Process {} exited gracefully", execution_process.id);
                    }
                    Err(_) => {
                        tracing::debug!(
                            "Graceful shutdown timed out for process {}, force killing",
                            execution_process.id
                        );
                    }
                }
            }
        }

        {
            let mut child_guard = child.write().await;
            if let Err(e) = command::kill_process_group(&mut child_guard).await {
                tracing::error!(
                    "Failed to stop execution process {}: {}",
                    execution_process.id,
                    e
                );
                return Err(e);
            }
        }
        self.remove_child_from_store(&execution_process.id).await;

        // Mark the process finished in the MsgStore and wait for DB persistence
        let db_stream_handle = self.take_db_stream_handle(&execution_process.id).await;
        if let Some(msg) = self.msg_stores.write().await.remove(&execution_process.id) {
            msg.push_finished();
        }
        if let Some(handle) = db_stream_handle {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }

        // Update task status to QA when execution is stopped
        if let Ok(ctx) = ExecutionProcess::load_context(&self.db.pool, execution_process.id).await
            && !matches!(
                ctx.execution_process.run_reason,
                ExecutionProcessRunReason::DevServer | ExecutionProcessRunReason::AutoPlan
            )
            && let Err(e) = Task::update_status(&self.db.pool, ctx.task.id, TaskStatus::QA).await
        {
            tracing::error!("Failed to update task status to QA: {e}");
        }

        tracing::debug!(
            "Execution process {} stopped successfully",
            execution_process.id
        );

        // Record after-head commit OID (best-effort)
        self.update_after_head_commits(execution_process.id).await;

        Ok(())
    }

    async fn stream_diff(
        &self,
        workspace: &Workspace,
        stats_only: bool,
    ) -> Result<futures::stream::BoxStream<'static, Result<LogMsg, std::io::Error>>, ContainerError>
    {
        let workspace_repos =
            WorkspaceRepo::find_by_workspace_id(&self.db.pool, workspace.id).await?;
        let target_branches: HashMap<_, _> = workspace_repos
            .iter()
            .map(|wr| (wr.repo_id, wr.target_branch.clone()))
            .collect();

        let repositories =
            WorkspaceRepo::find_repos_for_workspace(&self.db.pool, workspace.id).await?;

        let mut streams = Vec::new();

        let container_ref = self.ensure_container_exists(workspace).await?;
        let workspace_root = PathBuf::from(container_ref);

        for repo in repositories {
            let worktree_path = workspace_root.join(&repo.name);
            let branch = &workspace.branch;

            let Some(target_branch) = target_branches.get(&repo.id) else {
                tracing::warn!(
                    "Skipping diff stream for repo {}: no target branch configured",
                    repo.name
                );
                continue;
            };

            let base_commit = match self
                .git()
                .get_base_commit(&repo.path, branch, target_branch)
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "Skipping diff stream for repo {}: failed to get base commit: {}",
                        repo.name,
                        e
                    );
                    continue;
                }
            };

            let stream = self
                .create_live_diff_stream(diff_stream::DiffStreamArgs {
                    git_service: self.git().clone(),
                    db: self.db().clone(),
                    workspace_id: workspace.id,
                    repo_id: repo.id,
                    repo_path: repo.path.clone(),
                    worktree_path: worktree_path.clone(),
                    branch: branch.to_string(),
                    target_branch: target_branch.clone(),
                    base_commit: base_commit.clone(),
                    stats_only,
                    path_prefix: Some(repo.name.clone()),
                })
                .await?;

            streams.push(Box::pin(stream));
        }

        if streams.is_empty() {
            return Ok(Box::pin(futures::stream::empty()));
        }

        // Merge all streams into one
        Ok(Box::pin(futures::stream::select_all(streams)))
    }

    async fn try_commit_changes(&self, ctx: &ExecutionContext) -> Result<bool, ContainerError> {
        if !matches!(
            ctx.execution_process.run_reason,
            ExecutionProcessRunReason::CodingAgent | ExecutionProcessRunReason::CleanupScript,
        ) {
            return Ok(false);
        }

        let message = self.get_commit_message(ctx).await;

        let container_ref = ctx
            .workspace
            .container_ref
            .as_ref()
            .ok_or_else(|| ContainerError::Other(anyhow!("Container reference not found")))?;
        let workspace_root = PathBuf::from(container_ref);

        let repos_with_changes = self.check_repos_for_changes(&workspace_root, &ctx.repos)?;
        if repos_with_changes.is_empty() {
            tracing::debug!("No changes to commit in any repository");
            return Ok(false);
        }

        Ok(self.commit_repos(repos_with_changes, &message))
    }

    /// Copy files from the original project directory to the worktree.
    /// Skips files that already exist at target with same size.
    async fn copy_project_files(
        &self,
        source_dir: &Path,
        target_dir: &Path,
        copy_files: &str,
    ) -> Result<(), ContainerError> {
        let source_dir = source_dir.to_path_buf();
        let target_dir = target_dir.to_path_buf();
        let copy_files = copy_files.to_string();

        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                copy::copy_project_files_impl(&source_dir, &target_dir, &copy_files)
            }),
        )
        .await
        .map_err(|_| ContainerError::Other(anyhow!("Copy project files timed out after 30s")))?
        .map_err(|e| ContainerError::Other(anyhow!("Copy files task failed: {e}")))?
    }

    async fn kill_all_running_processes(&self) -> Result<(), ContainerError> {
        tracing::info!("Killing all running processes");
        let running_processes = ExecutionProcess::find_running(&self.db.pool).await?;

        tracing::info!(
            "Found {} running processes to kill",
            running_processes.len()
        );

        for process in running_processes {
            tracing::info!(
                "Killing process: id={}, run_reason={:?}",
                process.id,
                process.run_reason
            );
            if let Err(error) = self
                .stop_execution(&process, ExecutionProcessStatus::Killed)
                .await
            {
                tracing::error!(
                    "Failed to cleanly kill running execution process {:?}: {:?}",
                    process,
                    error
                );
            } else {
                tracing::info!("Successfully killed process: id={}", process.id);
            }
        }

        Ok(())
    }
}
fn success_exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatusExt::from_raw(0)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatusExt::from_raw(0)
    }
}

/// Check whether a path is inside any of the known workspace base directories.
/// Extracted as a pure function for testability.
fn is_path_under_known_bases(workspace_dir: &Path, known_bases: &[PathBuf]) -> bool {
    for base in known_bases {
        if workspace_dir.starts_with(base) {
            return true;
        }
    }
    false
}

/// Get the user's home directory from environment.
fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

/// Hard deny-list: returns true if a path must NEVER be deleted.
/// Checks against:
/// - The user's home directory and well-known subdirectories (Desktop, Documents, Projects, etc.)
/// - Any project repo directory (the actual git repos)
/// - Parent directories of any repo (e.g. ~/Desktop/Projects which contains repos)
fn is_path_protected(workspace_dir: &Path, repo_paths: &[PathBuf]) -> bool {
    // Never delete the home directory or its well-known children
    if let Some(home) = home_dir() {
        if workspace_dir == home {
            return true;
        }
        for subdir in &[
            "Desktop",
            "Documents",
            "Downloads",
            "Projects",
            "Developer",
            "repos",
            "src",
            "code",
            "work",
        ] {
            let protected = home.join(subdir);
            if workspace_dir == protected {
                return true;
            }
            // Also protect ~/Desktop/Projects, ~/Documents/Projects, etc.
            let nested_projects = protected.join("Projects");
            if workspace_dir == nested_projects {
                return true;
            }
        }
    }

    // Never delete a repo directory or any of its ancestors
    for repo_path in repo_paths {
        // Exact match: workspace_dir IS a repo
        if workspace_dir == repo_path.as_path() {
            return true;
        }
        // workspace_dir is a parent/ancestor of a repo
        // (e.g. ~/Desktop/Projects when repo is ~/Desktop/Projects/myapp)
        if repo_path.starts_with(workspace_dir) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_user_projects_dir_is_not_safe() {
        // Simulates the exact bug: container_ref pointed to ~/Projects/
        let user_home = TempDir::new().unwrap();
        let projects_dir = user_home.path().join("Projects");
        std::fs::create_dir_all(&projects_dir).unwrap();

        let known_bases = vec![PathBuf::from("/tmp/wickeban/worktrees")];
        assert!(
            !is_path_under_known_bases(&projects_dir, &known_bases),
            "User's Projects directory must NEVER be considered a safe workspace path"
        );
    }

    #[test]
    fn test_repo_parent_dir_is_not_safe() {
        // The original bug: repo at /Users/foo/Projects/myapp,
        // container_ref was set to /Users/foo/Projects/
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("Projects");
        let repo_dir = projects_dir.join("myapp");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let worktree_base = tmp.path().join("myapp-worktrees");
        let known_bases = vec![worktree_base];

        assert!(
            !is_path_under_known_bases(&projects_dir, &known_bases),
            "Repo parent directory must not be considered safe"
        );
    }

    #[test]
    fn test_isolated_plan_workspace_is_safe() {
        // Plan workspaces live under {project}-worktrees/plan-{uuid}/
        let tmp = TempDir::new().unwrap();
        let worktree_base = tmp.path().join("myproject-worktrees");
        let plan_workspace = worktree_base.join("plan-abc123");
        std::fs::create_dir_all(&plan_workspace).unwrap();

        let known_bases = vec![worktree_base];
        assert!(
            is_path_under_known_bases(&plan_workspace, &known_bases),
            "Plan workspace inside worktree base should be safe"
        );
    }

    #[test]
    fn test_regular_worktree_is_safe() {
        let tmp = TempDir::new().unwrap();
        let worktree_base = tmp.path().join("myproject-worktrees");
        let workspace = worktree_base.join("feature-login-a1b2c3");
        std::fs::create_dir_all(&workspace).unwrap();

        let known_bases = vec![worktree_base];
        assert!(is_path_under_known_bases(&workspace, &known_bases));
    }

    #[test]
    fn test_global_worktree_base_is_safe() {
        let tmp = TempDir::new().unwrap();
        let global_base = tmp.path().join("wickeban-worktrees");
        let workspace = global_base.join("workspace-123");
        std::fs::create_dir_all(&workspace).unwrap();

        let known_bases = vec![global_base];
        assert!(is_path_under_known_bases(&workspace, &known_bases));
    }

    #[test]
    fn test_empty_known_bases_rejects_everything() {
        let tmp = TempDir::new().unwrap();
        assert!(!is_path_under_known_bases(tmp.path(), &[]));
    }

    #[test]
    fn test_path_outside_all_bases_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let unrelated_dir = tmp.path().join("unrelated");
        std::fs::create_dir_all(&unrelated_dir).unwrap();

        let known_bases = vec![
            tmp.path().join("project-worktrees"),
            tmp.path().join("other-worktrees"),
        ];
        assert!(
            !is_path_under_known_bases(&unrelated_dir, &known_bases),
            "Path outside all known bases must be rejected"
        );
    }

    #[test]
    fn test_worktree_base_itself_is_safe() {
        // The base dir itself should be safe (starts_with includes exact match)
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("myproject-worktrees");
        std::fs::create_dir_all(&base).unwrap();

        let known_bases = vec![base.clone()];
        assert!(is_path_under_known_bases(&base, &known_bases));
    }

    #[test]
    fn test_partial_name_match_is_not_safe() {
        // "myproject-worktrees-evil" should NOT match "myproject-worktrees"
        // Path::starts_with does component-wise comparison, so this should be safe
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("myproject-worktrees");
        let evil_dir = tmp.path().join("myproject-worktrees-evil");
        std::fs::create_dir_all(&evil_dir).unwrap();

        let known_bases = vec![base];
        assert!(
            !is_path_under_known_bases(&evil_dir, &known_bases),
            "Partial directory name match must not be considered safe"
        );
    }

    #[test]
    fn test_multiple_bases_any_match_is_safe() {
        let tmp = TempDir::new().unwrap();
        let base_a = tmp.path().join("project-a-worktrees");
        let base_b = tmp.path().join("project-b-worktrees");
        let workspace = base_b.join("plan-xyz");
        std::fs::create_dir_all(&workspace).unwrap();

        let known_bases = vec![base_a, base_b];
        assert!(
            is_path_under_known_bases(&workspace, &known_bases),
            "Path matching any known base should be safe"
        );
    }

    /// Regression test: verify that cleanup_workspace refuses to delete a directory
    /// that is outside known worktree bases, even if it has a valid container_ref.
    /// This is the exact scenario that deleted user projects.
    #[tokio::test]
    async fn test_cleanup_workspace_does_not_delete_user_directory() {
        // Set up a fake "user projects" directory with content
        let user_projects = TempDir::new().unwrap();
        let my_project = user_projects.path().join("maisonneptune");
        std::fs::create_dir_all(&my_project).unwrap();
        std::fs::write(my_project.join("README.md"), "# My Project").unwrap();
        std::fs::write(my_project.join("index.ts"), "console.log('hello')").unwrap();

        // Create a workspace struct that points container_ref at the user directory
        // (simulating the old broken behavior)
        let workspace = Workspace {
            id: uuid::Uuid::new_v4(),
            task_id: uuid::Uuid::new_v4(),
            container_ref: Some(user_projects.path().to_string_lossy().to_string()),
            branch: "auto-plan-test".to_string(),
            agent_working_dir: Some("maisonneptune".to_string()),
            setup_completed_at: None,
            created_at: sqlx::types::chrono::Utc::now(),
            updated_at: sqlx::types::chrono::Utc::now(),
            archived: false,
            pinned: false,
            name: None,
        };

        // Verify the safety check rejects this path
        // (empty known_bases since we can't easily set up the global base in tests)
        let known_bases: Vec<PathBuf> = vec![];
        assert!(
            !is_path_under_known_bases(
                &PathBuf::from(workspace.container_ref.as_ref().unwrap()),
                &known_bases
            ),
            "User directory must not pass safety check"
        );

        // Verify the project files still exist
        assert!(my_project.exists(), "Project directory must still exist");
        assert!(
            my_project.join("README.md").exists(),
            "Project files must still exist"
        );
        assert!(
            my_project.join("index.ts").exists(),
            "Project files must still exist"
        );
    }

    /// Regression test: verify that a plan workspace under a worktree base IS cleaned up.
    #[tokio::test]
    async fn test_plan_workspace_under_worktree_base_is_safe_to_delete() {
        let tmp = TempDir::new().unwrap();
        let worktree_base = tmp.path().join("myproject-worktrees");
        let plan_dir = worktree_base.join("plan-abc123");
        std::fs::create_dir_all(&plan_dir).unwrap();
        std::fs::write(plan_dir.join("repo-link"), "symlink placeholder").unwrap();

        let known_bases = vec![worktree_base.clone()];
        assert!(
            is_path_under_known_bases(&plan_dir, &known_bases),
            "Plan workspace under worktree base should be safe to delete"
        );

        // Verify cleanup would proceed (the dir is under a known base)
        // and after removal it's gone
        tokio::fs::remove_dir_all(&plan_dir).await.unwrap();
        assert!(
            !plan_dir.exists(),
            "Plan dir should be removed after cleanup"
        );
        // But the base still exists
        assert!(
            worktree_base.exists(),
            "Worktree base should survive plan cleanup"
        );
    }

    // ── is_path_protected (deny-list) tests ────────────────────────────

    #[test]
    fn test_home_directory_is_protected() {
        if let Some(home) = home_dir() {
            assert!(
                is_path_protected(&home, &[]),
                "Home directory must always be protected"
            );
        }
    }

    #[test]
    fn test_desktop_projects_is_protected() {
        if let Some(home) = home_dir() {
            let desktop_projects = home.join("Desktop").join("Projects");
            assert!(
                is_path_protected(&desktop_projects, &[]),
                "~/Desktop/Projects must always be protected"
            );
        }
    }

    #[test]
    fn test_well_known_home_subdirs_are_protected() {
        if let Some(home) = home_dir() {
            for subdir in &["Desktop", "Documents", "Downloads", "Projects", "Developer"] {
                let path = home.join(subdir);
                assert!(
                    is_path_protected(&path, &[]),
                    "~/{subdir} must be protected"
                );
            }
        }
    }

    #[test]
    fn test_repo_directory_is_protected() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("Projects").join("maisonneptune");
        std::fs::create_dir_all(&repo).unwrap();

        let repo_paths = vec![repo.clone()];
        assert!(
            is_path_protected(&repo, &repo_paths),
            "A project repo directory must be protected"
        );
    }

    #[test]
    fn test_parent_of_repo_is_protected() {
        // This is the exact scenario: ~/Desktop/Projects contains repos,
        // so ~/Desktop/Projects itself must be protected
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("Desktop").join("Projects");
        let repo = projects_dir.join("maisonneptune");
        std::fs::create_dir_all(&repo).unwrap();

        let repo_paths = vec![repo];
        assert!(
            is_path_protected(&projects_dir, &repo_paths),
            "Parent directory of a repo must be protected"
        );
    }

    #[test]
    fn test_grandparent_of_repo_is_protected() {
        let tmp = TempDir::new().unwrap();
        let desktop = tmp.path().join("Desktop");
        let repo = desktop.join("Projects").join("myapp");
        std::fs::create_dir_all(&repo).unwrap();

        let repo_paths = vec![repo];
        assert!(
            is_path_protected(&desktop, &repo_paths),
            "Grandparent directory of a repo must be protected"
        );
    }

    #[test]
    fn test_worktree_dir_is_not_protected() {
        // A proper worktree directory should NOT be on the deny-list
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("Projects").join("myapp");
        let worktree = tmp.path().join("myapp-worktrees").join("plan-abc123");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();

        let repo_paths = vec![repo];
        assert!(
            !is_path_protected(&worktree, &repo_paths),
            "Worktree directories must NOT be on the deny-list"
        );
    }

    #[test]
    fn test_sibling_of_repo_is_not_protected() {
        // A directory that's a sibling of a repo (not parent/ancestor) is fine
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("Projects").join("myapp");
        let sibling = tmp
            .path()
            .join("Projects")
            .join("myapp-worktrees")
            .join("plan-123");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();

        let repo_paths = vec![repo];
        assert!(
            !is_path_protected(&sibling, &repo_paths),
            "Sibling worktree directory of a repo is not protected"
        );
    }

    #[test]
    fn test_child_of_repo_is_not_protected_by_denylist() {
        // A subdirectory inside a repo is not itself protected by the deny-list
        // (the allow-list would catch this — it's not under a worktree base)
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("myapp");
        let child = repo.join("src");
        std::fs::create_dir_all(&child).unwrap();

        let repo_paths = vec![repo];
        assert!(
            !is_path_protected(&child, &repo_paths),
            "Subdirectory of a repo is handled by allow-list, not deny-list"
        );
    }

    #[test]
    fn test_multiple_repos_all_parents_protected() {
        let tmp = TempDir::new().unwrap();
        let projects_dir = tmp.path().join("Projects");
        let repo_a = projects_dir.join("app-a");
        let repo_b = projects_dir.join("app-b");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();

        let repo_paths = vec![repo_a, repo_b];
        // The shared parent is protected
        assert!(
            is_path_protected(&projects_dir, &repo_paths),
            "Shared parent of multiple repos must be protected"
        );
    }

    /// Regression test: the exact scenario that deleted maisonneptune.
    /// container_ref = ~/Desktop/Projects, repo = ~/Desktop/Projects/maisonneptune
    #[test]
    fn test_regression_desktop_projects_with_repo_is_protected() {
        let tmp = TempDir::new().unwrap();
        // Simulate ~/Desktop/Projects/maisonneptune
        let desktop_projects = tmp.path().join("Desktop").join("Projects");
        let maisonneptune = desktop_projects.join("maisonneptune");
        std::fs::create_dir_all(&maisonneptune).unwrap();
        std::fs::write(maisonneptune.join("package.json"), "{}").unwrap();

        let repo_paths = vec![maisonneptune.clone()];

        // The parent directory (where container_ref pointed) must be blocked
        assert!(
            is_path_protected(&desktop_projects, &repo_paths),
            "~/Desktop/Projects must be protected when it contains repos"
        );
        // The repo itself must also be blocked
        assert!(
            is_path_protected(&maisonneptune, &repo_paths),
            "The repo directory itself must be protected"
        );
    }
}
