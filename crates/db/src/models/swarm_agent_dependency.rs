use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct SwarmAgentDependency {
    pub agent_id: Uuid,
    pub depends_on_agent_id: Uuid,
}

impl SwarmAgentDependency {
    pub async fn create(
        pool: &SqlitePool,
        agent_id: Uuid,
        depends_on_agent_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO swarm_agent_dependencies (agent_id, depends_on_agent_id) VALUES ($1, $2)",
            agent_id,
            depends_on_agent_id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn create_batch(
        pool: &SqlitePool,
        agent_id: Uuid,
        depends_on_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        if depends_on_ids.is_empty() {
            return Ok(());
        }

        let mut tx = pool.begin().await?;
        for dep_id in depends_on_ids {
            sqlx::query!(
                "INSERT INTO swarm_agent_dependencies (agent_id, depends_on_agent_id) VALUES ($1, $2)",
                agent_id,
                dep_id,
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn find_dependencies(
        pool: &SqlitePool,
        agent_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT depends_on_agent_id as "depends_on_agent_id!: Uuid"
               FROM swarm_agent_dependencies
               WHERE agent_id = $1"#,
            agent_id,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.depends_on_agent_id).collect())
    }

    pub async fn find_dependents(
        pool: &SqlitePool,
        depends_on_agent_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT agent_id as "agent_id!: Uuid"
               FROM swarm_agent_dependencies
               WHERE depends_on_agent_id = $1"#,
            depends_on_agent_id,
        )
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.agent_id).collect())
    }

    /// Find all dependencies for agents belonging to the given swarm.
    pub async fn find_by_swarm_agents(
        pool: &SqlitePool,
        swarm_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            SwarmAgentDependency,
            r#"SELECT sad.agent_id as "agent_id!: Uuid",
                      sad.depends_on_agent_id as "depends_on_agent_id!: Uuid"
               FROM swarm_agent_dependencies sad
               JOIN swarm_agents sa ON sa.id = sad.agent_id
               WHERE sa.swarm_id = $1"#,
            swarm_id,
        )
        .fetch_all(pool)
        .await
    }
}
