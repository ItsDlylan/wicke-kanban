use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "swarm_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SwarmStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "routing_decision", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RoutingDecision {
    #[default]
    Single,
    SingleVerifier,
    VsShallow,
    VsDeep,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Swarm {
    pub id: Uuid,
    pub task_id: Uuid,
    pub workspace_id: Uuid,
    pub parent_agent_id: Option<Uuid>,
    pub status: SwarmStatus,
    pub depth: i64,
    pub max_depth: i64,
    pub routing_decision: Option<String>,
    pub verifier_model: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Swarm {
    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        task_id: Uuid,
        workspace_id: Uuid,
        parent_agent_id: Option<Uuid>,
        depth: i64,
        max_depth: i64,
        routing_decision: Option<String>,
    ) -> Result<Self, sqlx::Error> {
        let status = SwarmStatus::Pending;
        sqlx::query_as!(
            Swarm,
            r#"INSERT INTO swarms (id, task_id, workspace_id, parent_agent_id, status, depth, max_depth, routing_decision)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid",
                         workspace_id as "workspace_id!: Uuid",
                         parent_agent_id as "parent_agent_id: Uuid",
                         status as "status!: SwarmStatus",
                         depth as "depth!: i64", max_depth as "max_depth!: i64",
                         routing_decision,
                         verifier_model,
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            task_id,
            workspace_id,
            parent_agent_id,
            status,
            depth,
            max_depth,
            routing_decision,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Swarm,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid",
                      workspace_id as "workspace_id!: Uuid",
                      parent_agent_id as "parent_agent_id: Uuid",
                      status as "status!: SwarmStatus",
                      depth as "depth!: i64", max_depth as "max_depth!: i64",
                      routing_decision,
                      verifier_model,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarms
               WHERE id = $1"#,
            id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Swarm,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid",
                      workspace_id as "workspace_id!: Uuid",
                      parent_agent_id as "parent_agent_id: Uuid",
                      status as "status!: SwarmStatus",
                      depth as "depth!: i64", max_depth as "max_depth!: i64",
                      routing_decision,
                      verifier_model,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarms
               WHERE task_id = $1
               ORDER BY created_at DESC"#,
            task_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_active_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Swarm,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid",
                      workspace_id as "workspace_id!: Uuid",
                      parent_agent_id as "parent_agent_id: Uuid",
                      status as "status!: SwarmStatus",
                      depth as "depth!: i64", max_depth as "max_depth!: i64",
                      routing_decision,
                      verifier_model,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarms
               WHERE task_id = $1 AND status IN ('pending', 'running')
               LIMIT 1"#,
            task_id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Swarm,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid",
                      workspace_id as "workspace_id!: Uuid",
                      parent_agent_id as "parent_agent_id: Uuid",
                      status as "status!: SwarmStatus",
                      depth as "depth!: i64", max_depth as "max_depth!: i64",
                      routing_decision,
                      verifier_model,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarms
               WHERE rowid = $1"#,
            rowid,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_parent_agent_id(
        pool: &SqlitePool,
        parent_agent_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Swarm,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid",
                      workspace_id as "workspace_id!: Uuid",
                      parent_agent_id as "parent_agent_id: Uuid",
                      status as "status!: SwarmStatus",
                      depth as "depth!: i64", max_depth as "max_depth!: i64",
                      routing_decision,
                      verifier_model,
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarms
               WHERE parent_agent_id = $1"#,
            parent_agent_id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: SwarmStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarms SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_verifier_model(
        pool: &SqlitePool,
        id: Uuid,
        verifier_model: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarms SET verifier_model = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            verifier_model,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
