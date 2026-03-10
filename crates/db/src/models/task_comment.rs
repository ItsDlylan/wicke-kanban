use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct TaskComment {
    pub id: Uuid,
    pub task_id: Uuid,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateTaskComment {
    pub content: String,
    #[serde(default)]
    pub image_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UpdateTaskComment {
    pub content: String,
    #[serde(default)]
    pub image_ids: Option<Vec<Uuid>>,
}

/// A comment with its associated image IDs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskCommentWithImages {
    #[serde(flatten)]
    #[ts(flatten)]
    pub comment: TaskComment,
    pub image_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, FromRow)]
pub struct CommentImage {
    pub id: Uuid,
    pub comment_id: Uuid,
    pub image_id: Uuid,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

impl TaskComment {
    pub async fn find_by_task_id(
        pool: &SqlitePool,
        task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskComment,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid", content,
                      created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM task_comments
               WHERE task_id = $1
               ORDER BY created_at ASC"#,
            task_id
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            TaskComment,
            r#"SELECT id as "id!: Uuid", task_id as "task_id!: Uuid", content,
                      created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM task_comments
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        task_id: Uuid,
        data: &CreateTaskComment,
    ) -> Result<Self, sqlx::Error> {
        let id = Uuid::new_v4();
        let comment = sqlx::query_as!(
            TaskComment,
            r#"INSERT INTO task_comments (id, task_id, content)
               VALUES ($1, $2, $3)
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", content,
                         created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            task_id,
            data.content
        )
        .fetch_one(pool)
        .await?;

        // Associate images
        if !data.image_ids.is_empty() {
            CommentImage::associate_many(pool, comment.id, &data.image_ids).await?;
        }

        Ok(comment)
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        data: &UpdateTaskComment,
    ) -> Result<Self, sqlx::Error> {
        let comment = sqlx::query_as!(
            TaskComment,
            r#"UPDATE task_comments
               SET content = $2, updated_at = CURRENT_TIMESTAMP
               WHERE id = $1
               RETURNING id as "id!: Uuid", task_id as "task_id!: Uuid", content,
                         created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            data.content
        )
        .fetch_one(pool)
        .await?;

        // Update image associations if provided
        if let Some(image_ids) = &data.image_ids {
            CommentImage::delete_by_comment_id(pool, id).await?;
            if !image_ids.is_empty() {
                CommentImage::associate_many(pool, id, image_ids).await?;
            }
        }

        Ok(comment)
    }

    pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM task_comments WHERE id = $1", id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete_by_task_id(pool: &SqlitePool, task_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM task_comments WHERE task_id = $1", task_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

impl CommentImage {
    pub async fn associate_many(
        pool: &SqlitePool,
        comment_id: Uuid,
        image_ids: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        for (order, &image_id) in image_ids.iter().enumerate() {
            let id = Uuid::new_v4();
            let sort_order = order as i32;
            sqlx::query!(
                r#"INSERT INTO comment_images (id, comment_id, image_id, sort_order)
                   VALUES ($1, $2, $3, $4)"#,
                id,
                comment_id,
                image_id,
                sort_order
            )
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    pub async fn find_image_ids_by_comment_id(
        pool: &SqlitePool,
        comment_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r#"SELECT image_id as "image_id!: Uuid"
               FROM comment_images
               WHERE comment_id = $1
               ORDER BY sort_order"#,
            comment_id
        )
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_by_comment_id(
        pool: &SqlitePool,
        comment_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM comment_images WHERE comment_id = $1",
            comment_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
