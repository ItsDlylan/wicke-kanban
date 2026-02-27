use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "swarm_agent_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum SwarmAgentStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Threshold,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct SwarmAgent {
    pub id: Uuid,
    pub swarm_id: Uuid,
    pub execution_process_id: Option<Uuid>,
    pub subtask_description: String,
    pub generation: i64,
    pub predecessor_id: Option<Uuid>,
    pub status: SwarmAgentStatus,
    pub context_tokens_used: Option<i64>,
    pub context_window_size: Option<i64>,
    pub context_threshold: f64,
    pub sort_order: i64,
    pub git_branch: Option<String>,
    pub succession_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SwarmAgent {
    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        swarm_id: Uuid,
        subtask_description: String,
        generation: i64,
        predecessor_id: Option<Uuid>,
        context_threshold: f64,
        sort_order: i64,
    ) -> Result<Self, sqlx::Error> {
        let status = SwarmAgentStatus::Pending;
        sqlx::query_as!(
            SwarmAgent,
            r#"INSERT INTO swarm_agents (id, swarm_id, subtask_description, generation, predecessor_id, status, context_threshold, sort_order)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                         execution_process_id as "execution_process_id: Uuid",
                         subtask_description, generation as "generation!: i64",
                         predecessor_id as "predecessor_id: Uuid",
                         status as "status!: SwarmAgentStatus",
                         context_tokens_used as "context_tokens_used: i64",
                         context_window_size as "context_window_size: i64",
                         context_threshold as "context_threshold!: f64",
                         sort_order as "sort_order!: i64",
                         git_branch,
                         succession_count as "succession_count!: i64",
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            swarm_id,
            subtask_description,
            generation,
            predecessor_id,
            status,
            context_threshold,
            sort_order,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      execution_process_id as "execution_process_id: Uuid",
                      subtask_description, generation as "generation!: i64",
                      predecessor_id as "predecessor_id: Uuid",
                      status as "status!: SwarmAgentStatus",
                      context_tokens_used as "context_tokens_used: i64",
                      context_window_size as "context_window_size: i64",
                      context_threshold as "context_threshold!: f64",
                      sort_order as "sort_order!: i64",
                      git_branch,
                      succession_count as "succession_count!: i64",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents
               WHERE id = $1"#,
            id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_swarm_id(
        pool: &SqlitePool,
        swarm_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      execution_process_id as "execution_process_id: Uuid",
                      subtask_description, generation as "generation!: i64",
                      predecessor_id as "predecessor_id: Uuid",
                      status as "status!: SwarmAgentStatus",
                      context_tokens_used as "context_tokens_used: i64",
                      context_window_size as "context_window_size: i64",
                      context_threshold as "context_threshold!: f64",
                      sort_order as "sort_order!: i64",
                      git_branch,
                      succession_count as "succession_count!: i64",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents
               WHERE swarm_id = $1
               ORDER BY sort_order ASC, created_at ASC"#,
            swarm_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      execution_process_id as "execution_process_id: Uuid",
                      subtask_description, generation as "generation!: i64",
                      predecessor_id as "predecessor_id: Uuid",
                      status as "status!: SwarmAgentStatus",
                      context_tokens_used as "context_tokens_used: i64",
                      context_window_size as "context_window_size: i64",
                      context_threshold as "context_threshold!: f64",
                      sort_order as "sort_order!: i64",
                      git_branch,
                      succession_count as "succession_count!: i64",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents
               WHERE rowid = $1"#,
            rowid,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_execution_process_id(
        pool: &SqlitePool,
        execution_process_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      execution_process_id as "execution_process_id: Uuid",
                      subtask_description, generation as "generation!: i64",
                      predecessor_id as "predecessor_id: Uuid",
                      status as "status!: SwarmAgentStatus",
                      context_tokens_used as "context_tokens_used: i64",
                      context_window_size as "context_window_size: i64",
                      context_threshold as "context_threshold!: f64",
                      sort_order as "sort_order!: i64",
                      git_branch,
                      succession_count as "succession_count!: i64",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents
               WHERE execution_process_id = $1"#,
            execution_process_id,
        )
        .fetch_optional(pool)
        .await
    }

    /// Find the next eligible agent in a swarm: status='pending' with all dependencies completed.
    pub async fn find_next_eligible(
        pool: &SqlitePool,
        swarm_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT sa.id as "id!: Uuid", sa.swarm_id as "swarm_id!: Uuid",
                      sa.execution_process_id as "execution_process_id: Uuid",
                      sa.subtask_description, sa.generation as "generation!: i64",
                      sa.predecessor_id as "predecessor_id: Uuid",
                      sa.status as "status!: SwarmAgentStatus",
                      sa.context_tokens_used as "context_tokens_used: i64",
                      sa.context_window_size as "context_window_size: i64",
                      sa.context_threshold as "context_threshold!: f64",
                      sa.sort_order as "sort_order!: i64",
                      sa.git_branch,
                      sa.succession_count as "succession_count!: i64",
                      sa.created_at as "created_at!: DateTime<Utc>",
                      sa.updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents sa
               WHERE sa.swarm_id = $1 AND sa.status = 'pending'
               AND NOT EXISTS (
                   SELECT 1 FROM swarm_agent_dependencies sad
                   JOIN swarm_agents dep ON dep.id = sad.depends_on_agent_id
                   WHERE sad.agent_id = sa.id AND dep.status != 'completed'
               )
               ORDER BY sa.sort_order ASC, sa.created_at ASC
               LIMIT 1"#,
            swarm_id,
        )
        .fetch_optional(pool)
        .await
    }

    /// Find ALL eligible agents in a swarm: status='pending' with all dependencies completed.
    /// Used for concurrent top-level agent spawning (section 3.2.1).
    pub async fn find_all_eligible(
        pool: &SqlitePool,
        swarm_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgent,
            r#"SELECT sa.id as "id!: Uuid", sa.swarm_id as "swarm_id!: Uuid",
                      sa.execution_process_id as "execution_process_id: Uuid",
                      sa.subtask_description, sa.generation as "generation!: i64",
                      sa.predecessor_id as "predecessor_id: Uuid",
                      sa.status as "status!: SwarmAgentStatus",
                      sa.context_tokens_used as "context_tokens_used: i64",
                      sa.context_window_size as "context_window_size: i64",
                      sa.context_threshold as "context_threshold!: f64",
                      sa.sort_order as "sort_order!: i64",
                      sa.git_branch,
                      sa.succession_count as "succession_count!: i64",
                      sa.created_at as "created_at!: DateTime<Utc>",
                      sa.updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_agents sa
               WHERE sa.swarm_id = $1 AND sa.status = 'pending'
               AND NOT EXISTS (
                   SELECT 1 FROM swarm_agent_dependencies sad
                   JOIN swarm_agents dep ON dep.id = sad.depends_on_agent_id
                   WHERE sad.agent_id = sa.id AND dep.status != 'completed'
               )
               ORDER BY sa.sort_order ASC, sa.created_at ASC"#,
            swarm_id,
        )
        .fetch_all(pool)
        .await
    }

    /// Check if all agents in a swarm are completed.
    pub async fn all_complete(pool: &SqlitePool, swarm_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "total!: i64",
                      SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as "done!: i64"
               FROM swarm_agents
               WHERE swarm_id = $1"#,
            swarm_id,
        )
        .fetch_one(pool)
        .await?;

        Ok(result.total > 0 && result.total == result.done)
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: SwarmAgentStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_agents SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_context_tokens(
        pool: &SqlitePool,
        id: Uuid,
        context_tokens_used: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_agents SET context_tokens_used = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            context_tokens_used,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_execution_process_id(
        pool: &SqlitePool,
        id: Uuid,
        execution_process_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_agents SET execution_process_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            execution_process_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_git_branch(
        pool: &SqlitePool,
        id: Uuid,
        git_branch: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_agents SET git_branch = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            git_branch,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn increment_succession_count(
        pool: &SqlitePool,
        id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_agents SET succession_count = succession_count + 1, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn count_by_swarm_id(pool: &SqlitePool, swarm_id: Uuid) -> Result<i64, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "count!: i64" FROM swarm_agents WHERE swarm_id = $1"#,
            swarm_id,
        )
        .fetch_one(pool)
        .await?;
        Ok(result.count)
    }
}
