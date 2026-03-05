use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "ralph_session_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum RalphSessionStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct RalphSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub status: RalphSessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct RalphSessionTask {
    pub ralph_session_id: Uuid,
    pub task_id: Uuid,
    pub execution_order: i32,
}

impl RalphSession {
    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
        workspace_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        let status = RalphSessionStatus::Pending;
        sqlx::query_as!(
            RalphSession,
            r#"INSERT INTO ralph_sessions (id, project_id, workspace_id, status)
               VALUES ($1, $2, $3, $4)
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid",
                         workspace_id as "workspace_id: Uuid",
                         status as "status!: RalphSessionStatus",
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            workspace_id,
            status,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            RalphSession,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid",
                      workspace_id as "workspace_id: Uuid",
                      status as "status!: RalphSessionStatus",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM ralph_sessions
               WHERE id = $1"#,
            id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: RalphSessionStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE ralph_sessions SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_workspace_id(
        pool: &SqlitePool,
        id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE ralph_sessions SET workspace_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            workspace_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Find the active ralph session that contains a given task.
    pub async fn find_active_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            RalphSession,
            r#"SELECT rs.id as "id!: Uuid", rs.project_id as "project_id!: Uuid",
                      rs.workspace_id as "workspace_id: Uuid",
                      rs.status as "status!: RalphSessionStatus",
                      rs.created_at as "created_at!: DateTime<Utc>",
                      rs.updated_at as "updated_at!: DateTime<Utc>"
               FROM ralph_sessions rs
               JOIN ralph_session_tasks rst ON rst.ralph_session_id = rs.id
               WHERE rst.task_id = $1
                 AND rs.status IN ('pending', 'running')
               LIMIT 1"#,
            task_id,
        )
        .fetch_optional(pool)
        .await
    }
}

impl RalphSessionTask {
    pub async fn create_many(
        pool: &SqlitePool,
        session_id: Uuid,
        task_ids: &[(Uuid, i32)],
    ) -> Result<(), sqlx::Error> {
        for (task_id, order) in task_ids {
            sqlx::query!(
                "INSERT INTO ralph_session_tasks (ralph_session_id, task_id, execution_order)
                 VALUES ($1, $2, $3)",
                session_id,
                task_id,
                order,
            )
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    pub async fn find_by_session_id(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            RalphSessionTask,
            r#"SELECT ralph_session_id as "ralph_session_id!: Uuid",
                      task_id as "task_id!: Uuid",
                      execution_order as "execution_order!: i32"
               FROM ralph_session_tasks
               WHERE ralph_session_id = $1
               ORDER BY execution_order ASC"#,
            session_id,
        )
        .fetch_all(pool)
        .await
    }

    /// Get the next task in the session that hasn't been completed yet.
    pub async fn find_next_pending_task(
        pool: &SqlitePool,
        session_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            RalphSessionTask,
            r#"SELECT rst.ralph_session_id as "ralph_session_id!: Uuid",
                      rst.task_id as "task_id!: Uuid",
                      rst.execution_order as "execution_order!: i32"
               FROM ralph_session_tasks rst
               JOIN tasks t ON t.id = rst.task_id
               WHERE rst.ralph_session_id = $1
                 AND t.status NOT IN ('done', 'qa', 'cancelled')
               ORDER BY rst.execution_order ASC
               LIMIT 1"#,
            session_id,
        )
        .fetch_optional(pool)
        .await
    }

    /// Get the next task in the session after the given task (by execution order),
    /// excluding tasks that are already done/qa/cancelled.
    pub async fn find_next_task_after(
        pool: &SqlitePool,
        session_id: Uuid,
        current_task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        // Get the execution order of the current task
        let current = sqlx::query_as!(
            RalphSessionTask,
            r#"SELECT ralph_session_id as "ralph_session_id!: Uuid",
                      task_id as "task_id!: Uuid",
                      execution_order as "execution_order!: i32"
               FROM ralph_session_tasks
               WHERE ralph_session_id = $1 AND task_id = $2"#,
            session_id,
            current_task_id,
        )
        .fetch_optional(pool)
        .await?;

        let current_order = match current {
            Some(c) => c.execution_order,
            None => return Ok(None),
        };

        sqlx::query_as!(
            RalphSessionTask,
            r#"SELECT rst.ralph_session_id as "ralph_session_id!: Uuid",
                      rst.task_id as "task_id!: Uuid",
                      rst.execution_order as "execution_order!: i32"
               FROM ralph_session_tasks rst
               JOIN tasks t ON t.id = rst.task_id
               WHERE rst.ralph_session_id = $1
                 AND rst.execution_order > $2
                 AND t.status NOT IN ('done', 'qa', 'cancelled')
               ORDER BY rst.execution_order ASC
               LIMIT 1"#,
            session_id,
            current_order,
        )
        .fetch_optional(pool)
        .await
    }
}
