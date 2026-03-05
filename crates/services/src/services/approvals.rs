pub mod executor_approvals;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration as StdDuration,
};

use dashmap::DashMap;
use db::models::{
    execution_process::ExecutionProcess,
    pending_approval::PendingApprovalRecord,
    task::{Task, TaskStatus},
};
use executors::{
    approvals::ToolCallMetadata,
    logs::{
        NormalizedEntry, NormalizedEntryType, ToolStatus,
        utils::patch::{ConversationPatch, extract_normalized_entry_from_patch},
    },
};
use futures::future::{BoxFuture, FutureExt, Shared};
use serde_json::Value;
use sqlx::{Error as SqlxError, SqlitePool};
use thiserror::Error;
use tokio::sync::{RwLock, oneshot};
use utils::{
    approvals::{ApprovalRequest, ApprovalResponse, ApprovalStatus},
    log_msg::LogMsg,
    msg_store::MsgStore,
};
use uuid::Uuid;

#[derive(Debug)]
struct PendingApproval {
    entry_index: usize,
    entry: NormalizedEntry,
    execution_process_id: Uuid,
    tool_name: String,
    response_tx: oneshot::Sender<(ApprovalStatus, Option<Value>)>,
}

type ApprovalWaiter = Shared<BoxFuture<'static, (ApprovalStatus, Option<Value>)>>;

#[derive(Debug)]
pub struct ToolContext {
    pub tool_name: String,
    pub execution_process_id: Uuid,
}

#[derive(Clone)]
pub struct Approvals {
    pending: Arc<DashMap<String, PendingApproval>>,
    completed: Arc<DashMap<String, ApprovalStatus>>,
    msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>,
    pool: SqlitePool,
}

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("approval request not found")]
    NotFound,
    #[error("approval request already completed")]
    AlreadyCompleted,
    #[error("no executor session found for session_id: {0}")]
    NoExecutorSession(String),
    #[error("corresponding tool use entry not found for approval request")]
    NoToolUseEntry,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
    #[error(transparent)]
    Sqlx(#[from] SqlxError),
}

impl Approvals {
    pub fn new(msg_stores: Arc<RwLock<HashMap<Uuid, Arc<MsgStore>>>>, pool: SqlitePool) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            completed: Arc::new(DashMap::new()),
            msg_stores,
            pool,
        }
    }

    pub async fn create_with_waiter(
        &self,
        request: ApprovalRequest,
    ) -> Result<(ApprovalRequest, ApprovalWaiter), ApprovalError> {
        let (tx, rx) = oneshot::channel();
        let waiter: ApprovalWaiter = rx
            .map(|result| result.unwrap_or((ApprovalStatus::TimedOut, None)))
            .boxed()
            .shared();
        let req_id = request.id.clone();

        if let Some(store) = self.msg_store_by_id(&request.execution_process_id).await {
            // Find the matching tool use entry by name and input
            let matching_tool = find_matching_tool_use(store.clone(), &request.tool_call_id);

            if let Some((idx, matching_tool)) = matching_tool {
                let approval_entry = matching_tool
                    .with_tool_status(ToolStatus::PendingApproval {
                        approval_id: req_id.clone(),
                        requested_at: request.created_at,
                        timeout_at: request.timeout_at,
                    })
                    .ok_or(ApprovalError::NoToolUseEntry)?;
                store.push_patch(ConversationPatch::replace(idx, approval_entry));

                self.pending.insert(
                    req_id.clone(),
                    PendingApproval {
                        entry_index: idx,
                        entry: matching_tool,
                        execution_process_id: request.execution_process_id,
                        tool_name: request.tool_name.clone(),
                        response_tx: tx,
                    },
                );
                tracing::debug!(
                    "Created approval {} for tool '{}' at entry index {}",
                    req_id,
                    request.tool_name,
                    idx
                );
            } else {
                tracing::warn!(
                    "No matching tool use entry found for approval request: tool='{}', execution_process_id={}",
                    request.tool_name,
                    request.execution_process_id
                );
            }
        } else {
            tracing::warn!(
                "No msg_store found for execution_process_id: {}",
                request.execution_process_id
            );
        }

        // Persist to DB for crash recovery
        let pool = self.pool.clone();
        let db_id = request.id.clone();
        let db_ep_id = request.execution_process_id;
        let db_tool_name = request.tool_name.clone();
        let db_tool_input = serde_json::to_string(&request.tool_input).unwrap_or_default();
        let db_tool_call_id = request.tool_call_id.clone();
        let db_timeout_at = request.timeout_at.to_rfc3339();
        tokio::spawn(async move {
            // Look up task_id from execution context
            let task_id = match ExecutionProcess::load_context(&pool, db_ep_id).await {
                Ok(ctx) => ctx.task.id,
                Err(e) => {
                    tracing::warn!(
                        "Failed to load context for pending approval DB persist (ep={}): {}",
                        db_ep_id,
                        e
                    );
                    return;
                }
            };
            if let Err(e) = PendingApprovalRecord::create(
                &pool,
                &db_id,
                db_ep_id,
                task_id,
                &db_tool_name,
                &db_tool_input,
                Some(&db_tool_call_id),
                &db_timeout_at,
            )
            .await
            {
                tracing::warn!("Failed to persist pending approval to DB: {}", e);
            }
        });

        self.spawn_timeout_watcher(req_id.clone(), request.timeout_at, waiter.clone());
        Ok((request, waiter))
    }

    #[tracing::instrument(skip(self, id, req))]
    pub async fn respond(
        &self,
        pool: &SqlitePool,
        id: &str,
        req: ApprovalResponse,
    ) -> Result<(ApprovalStatus, ToolContext), ApprovalError> {
        let db_status = match &req.status {
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Denied { .. } => "denied",
            ApprovalStatus::TimedOut => "timed_out",
            ApprovalStatus::Pending => "pending",
        };
        let db_response_input = req
            .updated_input
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        if let Some((_, p)) = self.pending.remove(id) {
            // Fast path: in-memory approval still exists
            // Update DB
            if let Err(e) =
                PendingApprovalRecord::respond(pool, id, db_status, db_response_input.as_deref())
                    .await
            {
                tracing::warn!("Failed to update pending approval in DB: {}", e);
            }

            self.completed.insert(id.to_string(), req.status.clone());
            let _ = p.response_tx.send((req.status.clone(), req.updated_input));

            if let Some(store) = self.msg_store_by_id(&p.execution_process_id).await {
                let status = ToolStatus::from_approval_status(&req.status).ok_or(
                    ApprovalError::Custom(anyhow::anyhow!("Invalid approval status")),
                )?;
                let updated_entry = p
                    .entry
                    .with_tool_status(status)
                    .ok_or(ApprovalError::NoToolUseEntry)?;

                store.push_patch(ConversationPatch::replace(p.entry_index, updated_entry));
            } else {
                tracing::warn!(
                    "No msg_store found for execution_process_id: {}",
                    p.execution_process_id
                );
            }

            let tool_ctx = ToolContext {
                tool_name: p.tool_name,
                execution_process_id: p.execution_process_id,
            };

            // If approved or denied, and task is still InReview, move back to InProgress
            if matches!(
                req.status,
                ApprovalStatus::Approved | ApprovalStatus::Denied { .. }
            ) && let Ok(ctx) =
                ExecutionProcess::load_context(pool, tool_ctx.execution_process_id).await
                && ctx.task.status == TaskStatus::QA
                && let Err(e) = Task::update_status(pool, ctx.task.id, TaskStatus::Ralph).await
            {
                tracing::warn!(
                    "Failed to update task status to Ralph after approval response: {}",
                    e
                );
            }

            Ok((req.status, tool_ctx))
        } else if self.completed.contains_key(id) {
            Err(ApprovalError::AlreadyCompleted)
        } else {
            // Orphaned approval: not in memory but may exist in DB
            // This handles the recovery case after a server restart.
            // Check DB BEFORE updating so we can verify it's still pending.
            match PendingApprovalRecord::find_by_id(pool, id).await {
                Ok(Some(record)) if record.status == "pending" => {
                    // Now update DB with the response
                    if let Err(e) = PendingApprovalRecord::respond(
                        pool,
                        id,
                        db_status,
                        db_response_input.as_deref(),
                    )
                    .await
                    {
                        tracing::warn!("Failed to update orphaned pending approval in DB: {}", e);
                    }

                    tracing::info!(
                        "Responding to orphaned approval {} for task {}",
                        id,
                        record.task_id
                    );
                    let tool_ctx = ToolContext {
                        tool_name: record.tool_name,
                        execution_process_id: record.execution_process_id,
                    };
                    Ok((req.status, tool_ctx))
                }
                _ => Err(ApprovalError::NotFound),
            }
        }
    }

    #[tracing::instrument(skip(self, id, timeout_at, waiter))]
    fn spawn_timeout_watcher(
        &self,
        id: String,
        timeout_at: chrono::DateTime<chrono::Utc>,
        waiter: ApprovalWaiter,
    ) {
        let pending = self.pending.clone();
        let completed = self.completed.clone();
        let msg_stores = self.msg_stores.clone();

        let now = chrono::Utc::now();
        let to_wait = (timeout_at - now)
            .to_std()
            .unwrap_or_else(|_| StdDuration::from_secs(0));
        let deadline = tokio::time::Instant::now() + to_wait;

        tokio::spawn(async move {
            let (status, _updated_input) = tokio::select! {
                biased;

                resolved = waiter.clone() => resolved,
                _ = tokio::time::sleep_until(deadline) => (ApprovalStatus::TimedOut, None),
            };

            let is_timeout = matches!(&status, ApprovalStatus::TimedOut);
            completed.insert(id.clone(), status.clone());

            if is_timeout && let Some((_, pending_approval)) = pending.remove(&id) {
                if pending_approval
                    .response_tx
                    .send((status.clone(), None))
                    .is_err()
                {
                    tracing::debug!("approval '{}' timeout notification receiver dropped", id);
                }

                let store = {
                    let map = msg_stores.read().await;
                    map.get(&pending_approval.execution_process_id).cloned()
                };

                if let Some(store) = store {
                    if let Some(updated_entry) = pending_approval
                        .entry
                        .with_tool_status(ToolStatus::TimedOut)
                    {
                        store.push_patch(ConversationPatch::replace(
                            pending_approval.entry_index,
                            updated_entry,
                        ));
                    } else {
                        tracing::warn!(
                            "Timed out approval '{}' but couldn't update tool status (no tool-use entry).",
                            id
                        );
                    }
                } else {
                    tracing::warn!(
                        "No msg_store found for execution_process_id: {}",
                        pending_approval.execution_process_id
                    );
                }
            }
        });
    }

    async fn msg_store_by_id(&self, execution_process_id: &Uuid) -> Option<Arc<MsgStore>> {
        let map = self.msg_stores.read().await;
        map.get(execution_process_id).cloned()
    }

    pub(crate) async fn cancel(&self, id: &str) {
        if let Some((_, pending_approval)) = self.pending.remove(id) {
            self.completed.insert(
                id.to_string(),
                ApprovalStatus::Denied {
                    reason: Some("Cancelled".to_string()),
                },
            );

            if let Some(store) = self
                .msg_store_by_id(&pending_approval.execution_process_id)
                .await
                && let Some(entry) = pending_approval.entry.with_tool_status(ToolStatus::Denied {
                    reason: Some("Cancelled".to_string()),
                })
            {
                store.push_patch(ConversationPatch::replace(
                    pending_approval.entry_index,
                    entry,
                ));
            }

            // Update DB status to cancelled
            if let Err(e) = PendingApprovalRecord::respond(&self.pool, id, "cancelled", None).await
            {
                tracing::warn!("Failed to cancel pending approval in DB: {}", e);
            }

            tracing::debug!("Cancelled approval '{}'", id);
        }
    }

    /// Check which execution processes have pending approvals.
    /// Returns a set of execution_process_ids that have at least one pending approval.
    pub fn get_pending_execution_process_ids(
        &self,
        execution_process_ids: &[Uuid],
    ) -> HashSet<Uuid> {
        let id_set: HashSet<_> = execution_process_ids.iter().collect();
        self.pending
            .iter()
            .filter_map(|entry| {
                let ep_id = entry.value().execution_process_id;
                if id_set.contains(&ep_id) {
                    Some(ep_id)
                } else {
                    None
                }
            })
            .collect()
    }
}

pub(crate) async fn ensure_task_in_review(pool: &SqlitePool, execution_process_id: Uuid) {
    if let Ok(ctx) = ExecutionProcess::load_context(pool, execution_process_id).await
        && ctx.task.status == TaskStatus::Ralph
        && let Err(e) = Task::update_status(pool, ctx.task.id, TaskStatus::QA).await
    {
        tracing::warn!(
            "Failed to update task status to InReview for approval request: {}",
            e
        );
    }
}

/// Find a matching tool use entry that hasn't been assigned to an approval yet
/// Matches by tool call id from tool metadata
fn find_matching_tool_use(
    store: Arc<MsgStore>,
    tool_call_id: &str,
) -> Option<(usize, NormalizedEntry)> {
    let history = store.get_history();

    // Single loop through history
    for msg in history.iter().rev() {
        if let LogMsg::JsonPatch(patch) = msg
            && let Some((idx, entry)) = extract_normalized_entry_from_patch(patch)
            && let NormalizedEntryType::ToolUse { status, .. } = &entry.entry_type
        {
            // Only match tools that are in Created state
            if !matches!(status, ToolStatus::Created) {
                continue;
            }

            // Match by tool call id from metadata
            if let Some(metadata) = &entry.metadata
                && let Ok(ToolCallMetadata {
                    tool_call_id: entry_call_id,
                    ..
                }) = serde_json::from_value::<ToolCallMetadata>(metadata.clone())
                && entry_call_id == tool_call_id
            {
                tracing::debug!(
                    "Matched tool use entry at index {idx} for tool call id '{tool_call_id}'"
                );
                return Some((idx, entry));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use executors::logs::{ActionType, NormalizedEntry, NormalizedEntryType, ToolStatus};
    use utils::msg_store::MsgStore;

    use super::*;

    fn create_tool_use_entry(
        tool_name: &str,
        file_path: &str,
        id: &str,
        status: ToolStatus,
    ) -> NormalizedEntry {
        NormalizedEntry {
            timestamp: None,
            entry_type: NormalizedEntryType::ToolUse {
                tool_name: tool_name.to_string(),
                action_type: ActionType::FileRead {
                    path: file_path.to_string(),
                },
                status,
            },
            content: format!("Reading {file_path}"),
            metadata: Some(
                serde_json::to_value(ToolCallMetadata {
                    tool_call_id: id.to_string(),
                })
                .unwrap(),
            ),
        }
    }

    #[test]
    fn test_parallel_tool_call_approval_matching() {
        let store = Arc::new(MsgStore::new());

        // Setup: Simulate 3 parallel Read tool calls with different files
        let read_foo = create_tool_use_entry("Read", "foo.rs", "foo-id", ToolStatus::Created);
        let read_bar = create_tool_use_entry("Read", "bar.rs", "bar-id", ToolStatus::Created);
        let read_baz = create_tool_use_entry("Read", "baz.rs", "baz-id", ToolStatus::Created);

        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(0, read_foo),
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(1, read_bar),
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(2, read_baz),
        );

        let (idx_foo, _) =
            find_matching_tool_use(store.clone(), "foo-id").expect("Should match foo.rs");
        let (idx_bar, _) =
            find_matching_tool_use(store.clone(), "bar-id").expect("Should match bar.rs");
        let (idx_baz, _) =
            find_matching_tool_use(store.clone(), "baz-id").expect("Should match baz.rs");

        assert_eq!(idx_foo, 0, "foo.rs should match first entry");
        assert_eq!(idx_bar, 1, "bar.rs should match second entry");
        assert_eq!(idx_baz, 2, "baz.rs should match third entry");

        // Test 2: Already pending tools are skipped
        let read_pending = create_tool_use_entry(
            "Read",
            "pending.rs",
            "pending-id",
            ToolStatus::PendingApproval {
                approval_id: "test-id".to_string(),
                requested_at: chrono::Utc::now(),
                timeout_at: chrono::Utc::now(),
            },
        );
        store.push_patch(
            executors::logs::utils::patch::ConversationPatch::add_normalized_entry(3, read_pending),
        );

        assert!(
            find_matching_tool_use(store.clone(), "pending-id").is_none(),
            "Should not match tools in PendingApproval state"
        );

        // Test 3: Wrong tool id returns None
        assert!(
            find_matching_tool_use(store.clone(), "wrong-id").is_none(),
            "Should not match different tool ids"
        );
    }
}
