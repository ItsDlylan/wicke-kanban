use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::get,
};
use db::models::{
    planning_session::{CreatePlanningSession, PlanningSession, UpdatePlanningSession},
    task::Task,
};
use deployment::Deployment;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub async fn list_sessions(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<PlanningSession>>>, ApiError> {
    // Verify task exists
    Task::find_by_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let sessions = PlanningSession::find_by_task_id(&deployment.db().pool, task_id).await?;
    Ok(ResponseJson(ApiResponse::success(sessions)))
}

pub async fn create_session(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<CreatePlanningSession>,
) -> Result<ResponseJson<ApiResponse<PlanningSession>>, ApiError> {
    // Verify task exists
    Task::find_by_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let session = PlanningSession::create(&deployment.db().pool, task_id, &payload).await?;
    Ok(ResponseJson(ApiResponse::success(session)))
}

pub async fn update_session(
    State(deployment): State<DeploymentImpl>,
    Path((_task_id, session_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdatePlanningSession>,
) -> Result<ResponseJson<ApiResponse<PlanningSession>>, ApiError> {
    let session = PlanningSession::update(&deployment.db().pool, session_id, &payload).await?;
    Ok(ResponseJson(ApiResponse::success(session)))
}

pub async fn delete_session(
    State(deployment): State<DeploymentImpl>,
    Path((_task_id, session_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let rows_affected = PlanningSession::delete(&deployment.db().pool, session_id).await?;
    if rows_affected == 0 {
        Err(ApiError::Database(sqlx::Error::RowNotFound))
    } else {
        Ok(ResponseJson(ApiResponse::success(())))
    }
}

pub fn router() -> Router<DeploymentImpl> {
    let session_routes = Router::new()
        .route("/", get(list_sessions).post(create_session))
        .route(
            "/{session_id}",
            axum::routing::put(update_session).delete(delete_session),
        );

    Router::new().nest("/tasks/{task_id}/planning-sessions", session_routes)
}
