use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, put},
};
use db::models::{
    task::Task,
    task_comment::{
        CommentImage, CreateTaskComment, TaskComment, TaskCommentWithImages, UpdateTaskComment,
    },
};
use deployment::Deployment;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub async fn list_comments(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<TaskCommentWithImages>>>, ApiError> {
    let pool = &deployment.db().pool;

    Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let comments = TaskComment::find_by_task_id(pool, task_id).await?;

    let mut result = Vec::with_capacity(comments.len());
    for comment in comments {
        let image_ids = CommentImage::find_image_ids_by_comment_id(pool, comment.id).await?;
        result.push(TaskCommentWithImages { comment, image_ids });
    }

    Ok(ResponseJson(ApiResponse::success(result)))
}

pub async fn create_comment(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<CreateTaskComment>,
) -> Result<ResponseJson<ApiResponse<TaskCommentWithImages>>, ApiError> {
    let pool = &deployment.db().pool;

    Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let comment = TaskComment::create(pool, task_id, &payload).await?;
    let image_ids = CommentImage::find_image_ids_by_comment_id(pool, comment.id).await?;

    Ok(ResponseJson(ApiResponse::success(TaskCommentWithImages {
        comment,
        image_ids,
    })))
}

pub async fn update_comment(
    State(deployment): State<DeploymentImpl>,
    Path((task_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateTaskComment>,
) -> Result<ResponseJson<ApiResponse<TaskCommentWithImages>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify comment belongs to this task
    let existing = TaskComment::find_by_id(pool, comment_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    if existing.task_id != task_id {
        return Err(ApiError::BadRequest(
            "Comment does not belong to this task".to_string(),
        ));
    }

    let comment = TaskComment::update(pool, comment_id, &payload).await?;
    let image_ids = CommentImage::find_image_ids_by_comment_id(pool, comment.id).await?;

    Ok(ResponseJson(ApiResponse::success(TaskCommentWithImages {
        comment,
        image_ids,
    })))
}

pub async fn delete_comment(
    State(deployment): State<DeploymentImpl>,
    Path((task_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let pool = &deployment.db().pool;

    // Verify comment belongs to this task
    let existing = TaskComment::find_by_id(pool, comment_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    if existing.task_id != task_id {
        return Err(ApiError::BadRequest(
            "Comment does not belong to this task".to_string(),
        ));
    }

    TaskComment::delete(pool, comment_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route(
            "/tasks/{task_id}/comments",
            get(list_comments).post(create_comment),
        )
        .route(
            "/tasks/{task_id}/comments/{comment_id}",
            put(update_comment).delete(delete_comment),
        )
}
