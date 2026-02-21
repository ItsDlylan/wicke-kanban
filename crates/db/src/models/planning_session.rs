use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct PlanningSession {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_ref: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreatePlanningSession {
    pub session_ref: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdatePlanningSession {
    pub session_ref: Option<String>,
    pub notes: Option<String>,
}

impl PlanningSession {
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            PlanningSession,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid", session_ref, notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM planning_sessions
               WHERE task_id = $1
               ORDER BY created_at DESC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &CreatePlanningSession,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            PlanningSession,
            r#"INSERT INTO planning_sessions (id, task_id, session_ref, notes)
               VALUES ($1, $2, $3, $4)
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", session_ref, notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            task_id,
            data.session_ref,
            data.notes
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdatePlanningSession,
    ) -> Result<Self, sqlx::Error> {
        // Build update dynamically based on provided fields
        let current = sqlx::query_as!(
            PlanningSession,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid", session_ref, notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM planning_sessions WHERE id = $1"#,
            id
        )
        .fetch_one(pool)
        .await?;

        let session_ref = data.session_ref.as_deref().unwrap_or(&current.session_ref);
        let notes = match &data.notes {
            Some(n) => Some(n.as_str()),
            None => current.notes.as_deref(),
        };

        sqlx::query_as!(
            PlanningSession,
            r#"UPDATE planning_sessions
               SET session_ref = $2, notes = $3, updated_at = CURRENT_TIMESTAMP
               WHERE id = $1
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", session_ref, notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            session_ref,
            notes
        )
        .fetch_one(pool)
        .await
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM planning_sessions WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
