use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct PendingApprovalRecord {
    pub id: String,
    pub execution_process_id: Uuid,
    pub task_id: Uuid,
    pub tool_name: String,
    pub tool_input: String,
    pub tool_call_id: Option<String>,
    pub status: String,
    pub response_input: Option<String>,
    pub created_at: String,
    pub timeout_at: String,
    pub responded_at: Option<String>,
}

impl PendingApprovalRecord {
    pub async fn create(
        pool: &SqlitePool,
        id: &str,
        execution_process_id: Uuid,
        task_id: Uuid,
        tool_name: &str,
        tool_input: &str,
        tool_call_id: Option<&str>,
        timeout_at: &str,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            PendingApprovalRecord,
            r#"INSERT INTO pending_approvals (id, execution_process_id, task_id, tool_name, tool_input, tool_call_id, timeout_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING
                   id as "id!",
                   execution_process_id as "execution_process_id!: Uuid",
                   task_id as "task_id!: Uuid",
                   tool_name as "tool_name!",
                   tool_input as "tool_input!",
                   tool_call_id,
                   status as "status!",
                   response_input,
                   created_at as "created_at!",
                   timeout_at as "timeout_at!",
                   responded_at"#,
            id,
            execution_process_id,
            task_id,
            tool_name,
            tool_input,
            tool_call_id,
            timeout_at,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn respond(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        response_input: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE pending_approvals
               SET status = $2, response_input = $3, responded_at = datetime('now', 'subsec')
               WHERE id = $1"#,
            id,
            status,
            response_input,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn find_pending_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            PendingApprovalRecord,
            r#"SELECT
                   id as "id!",
                   execution_process_id as "execution_process_id!: Uuid",
                   task_id as "task_id!: Uuid",
                   tool_name as "tool_name!",
                   tool_input as "tool_input!",
                   tool_call_id,
                   status as "status!",
                   response_input,
                   created_at as "created_at!",
                   timeout_at as "timeout_at!",
                   responded_at
               FROM pending_approvals
               WHERE task_id = $1 AND status = 'pending'
               ORDER BY created_at DESC"#,
            task_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            PendingApprovalRecord,
            r#"SELECT
                   id as "id!",
                   execution_process_id as "execution_process_id!: Uuid",
                   task_id as "task_id!: Uuid",
                   tool_name as "tool_name!",
                   tool_input as "tool_input!",
                   tool_call_id,
                   status as "status!",
                   response_input,
                   created_at as "created_at!",
                   timeout_at as "timeout_at!",
                   responded_at
               FROM pending_approvals
               WHERE id = $1"#,
            id,
        )
        .fetch_optional(pool)
        .await
    }

    /// Find pending approvals whose execution process is dead (failed/killed/completed).
    /// These are orphaned approvals that survived a server restart.
    pub async fn find_pending_orphaned(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            PendingApprovalRecord,
            r#"SELECT
                   pa.id as "id!",
                   pa.execution_process_id as "execution_process_id!: Uuid",
                   pa.task_id as "task_id!: Uuid",
                   pa.tool_name as "tool_name!",
                   pa.tool_input as "tool_input!",
                   pa.tool_call_id,
                   pa.status as "status!",
                   pa.response_input,
                   pa.created_at as "created_at!",
                   pa.timeout_at as "timeout_at!",
                   pa.responded_at
               FROM pending_approvals pa
               JOIN execution_processes ep ON ep.id = pa.execution_process_id
               WHERE pa.status = 'pending'
                 AND ep.status IN ('failed', 'killed', 'completed')"#,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn cancel_by_execution_process_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE pending_approvals
               SET status = 'cancelled', responded_at = datetime('now', 'subsec')
               WHERE execution_process_id = $1 AND status = 'pending'"#,
            execution_process_id,
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn timeout_expired(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"UPDATE pending_approvals
               SET status = 'timed_out', responded_at = datetime('now', 'subsec')
               WHERE status = 'pending' AND timeout_at < datetime('now', 'subsec')"#,
        )
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
