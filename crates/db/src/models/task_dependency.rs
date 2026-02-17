use sqlx::SqlitePool;
use uuid::Uuid;

pub struct TaskDependency {
    pub task_id: Uuid,
    pub depends_on: Uuid,
}

impl TaskDependency {
    pub async fn create_many(
        pool: &SqlitePool,
        task_id: Uuid,
        depends_on_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        if depends_on_ids.is_empty() {
            return Ok(());
        }

        let mut tx = pool.begin().await?;
        for dep_id in depends_on_ids {
            sqlx::query!(
                "INSERT INTO task_dependencies (task_id, depends_on) VALUES ($1, $2)",
                task_id,
                dep_id
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn find_dependencies(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT depends_on as "depends_on!: Uuid" FROM task_dependencies WHERE task_id = $1"#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.depends_on).collect())
    }

    pub async fn delete_by_task_id(pool: &SqlitePool, task_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM task_dependencies WHERE task_id = $1", task_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
