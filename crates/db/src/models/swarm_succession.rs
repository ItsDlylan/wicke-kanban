use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "swarm_succession_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SwarmSuccessionStatus {
    #[default]
    Pending,
    Verifying,
    Verified,
    SuccessorRunning,
    Failed,
}

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "recovery_strategy", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RecoveryStrategy {
    #[default]
    Corrective,
    CleanRestart,
    Redecomposition,
    Escalation,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct SwarmSuccession {
    pub id: Uuid,
    pub swarm_id: Uuid,
    pub predecessor_id: Uuid,
    pub verifier_execution_id: Option<Uuid>,
    pub successor_id: Option<Uuid>,
    pub predecessor_self_assessment: Option<String>,
    pub verification_report: Option<String>,
    pub verifier_confidence: Option<f64>,
    pub recovery_strategy: Option<String>,
    pub status: SwarmSuccessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SwarmSuccession {
    pub async fn create(
        pool: &SqlitePool,
        id: Uuid,
        swarm_id: Uuid,
        predecessor_id: Uuid,
        predecessor_self_assessment: Option<String>,
    ) -> Result<Self, sqlx::Error> {
        let status = SwarmSuccessionStatus::Pending;
        sqlx::query_as!(
            SwarmSuccession,
            r#"INSERT INTO swarm_successions (id, swarm_id, predecessor_id, predecessor_self_assessment, status)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                         predecessor_id as "predecessor_id!: Uuid",
                         verifier_execution_id as "verifier_execution_id: Uuid",
                         successor_id as "successor_id: Uuid",
                         predecessor_self_assessment,
                         verification_report,
                         verifier_confidence as "verifier_confidence: f64",
                         recovery_strategy,
                         status as "status!: SwarmSuccessionStatus",
                         created_at as "created_at!: DateTime<Utc>",
                         updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            swarm_id,
            predecessor_id,
            predecessor_self_assessment,
            status,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmSuccession,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      predecessor_id as "predecessor_id!: Uuid",
                      verifier_execution_id as "verifier_execution_id: Uuid",
                      successor_id as "successor_id: Uuid",
                      predecessor_self_assessment,
                      verification_report,
                      verifier_confidence as "verifier_confidence: f64",
                      recovery_strategy,
                      status as "status!: SwarmSuccessionStatus",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_successions
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
            SwarmSuccession,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      predecessor_id as "predecessor_id!: Uuid",
                      verifier_execution_id as "verifier_execution_id: Uuid",
                      successor_id as "successor_id: Uuid",
                      predecessor_self_assessment,
                      verification_report,
                      verifier_confidence as "verifier_confidence: f64",
                      recovery_strategy,
                      status as "status!: SwarmSuccessionStatus",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_successions
               WHERE swarm_id = $1
               ORDER BY created_at ASC"#,
            swarm_id,
        )
        .fetch_all(pool)
        .await
    }

    /// Find the active succession for a given predecessor agent.
    pub async fn find_active_for_agent(
        pool: &SqlitePool,
        predecessor_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmSuccession,
            r#"SELECT id as "id!: Uuid", swarm_id as "swarm_id!: Uuid",
                      predecessor_id as "predecessor_id!: Uuid",
                      verifier_execution_id as "verifier_execution_id: Uuid",
                      successor_id as "successor_id: Uuid",
                      predecessor_self_assessment,
                      verification_report,
                      verifier_confidence as "verifier_confidence: f64",
                      recovery_strategy,
                      status as "status!: SwarmSuccessionStatus",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM swarm_successions
               WHERE predecessor_id = $1 AND status IN ('pending', 'verifying', 'verified', 'successor_running')
               ORDER BY created_at DESC
               LIMIT 1"#,
            predecessor_id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: SwarmSuccessionStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_successions SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_verification(
        pool: &SqlitePool,
        id: Uuid,
        verification_report: &str,
        verifier_confidence: f64,
        recovery_strategy: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE swarm_successions
               SET verification_report = $2, verifier_confidence = $3,
                   recovery_strategy = $4, status = 'verified',
                   updated_at = CURRENT_TIMESTAMP
               WHERE id = $1"#,
            id,
            verification_report,
            verifier_confidence,
            recovery_strategy,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_successor_id(
        pool: &SqlitePool,
        id: Uuid,
        successor_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE swarm_successions SET successor_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            successor_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
