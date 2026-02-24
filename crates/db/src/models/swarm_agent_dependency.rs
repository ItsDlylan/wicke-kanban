use sqlx::SqlitePool;
use uuid::Uuid;

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
}
