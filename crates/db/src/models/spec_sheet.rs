use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct SpecSheet {
    pub id: Uuid,
    pub task_id: Uuid,
    pub overview: Option<String>,
    pub requirements: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub constraints: Option<String>,
    pub tech_notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateSpecSheet {
    pub overview: Option<String>,
    pub requirements: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub constraints: Option<String>,
    pub tech_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateSpecSheet {
    pub overview: Option<String>,
    pub requirements: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub constraints: Option<String>,
    pub tech_notes: Option<String>,
}

impl SpecSheet {
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            SpecSheet,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid", overview, requirements, acceptance_criteria, constraints, tech_notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM spec_sheets
               WHERE task_id = $1"#,
            task_id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &CreateSpecSheet,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as!(
            SpecSheet,
            r#"INSERT INTO spec_sheets (id, task_id, overview, requirements, acceptance_criteria, constraints, tech_notes)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", overview, requirements, acceptance_criteria, constraints, tech_notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            task_id,
            data.overview,
            data.requirements,
            data.acceptance_criteria,
            data.constraints,
            data.tech_notes
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &UpdateSpecSheet,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            SpecSheet,
            r#"UPDATE spec_sheets
               SET overview = $2, requirements = $3, acceptance_criteria = $4, constraints = $5, tech_notes = $6, updated_at = CURRENT_TIMESTAMP
               WHERE task_id = $1
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", overview, requirements, acceptance_criteria, constraints, tech_notes, created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            task_id,
            data.overview,
            data.requirements,
            data.acceptance_criteria,
            data.constraints,
            data.tech_notes
        )
        .fetch_one(pool)
        .await
    }

    pub async fn upsert(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &CreateSpecSheet,
    ) -> Result<Self, sqlx::Error> {
        let existing = Self::find_by_task_id(pool, task_id).await?;
        if existing.is_some() {
            tracing::debug!(task_id = %task_id, "Updating existing spec sheet");
            let update_data = UpdateSpecSheet {
                overview: data.overview.clone(),
                requirements: data.requirements.clone(),
                acceptance_criteria: data.acceptance_criteria.clone(),
                constraints: data.constraints.clone(),
                tech_notes: data.tech_notes.clone(),
            };
            Self::update(pool, task_id, &update_data).await
        } else {
            tracing::debug!(task_id = %task_id, "Creating new spec sheet");
            Self::create(pool, task_id, data).await
        }
    }

    pub async fn delete_by_task_id(pool: &SqlitePool, task_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM spec_sheets WHERE task_id = $1", task_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}
