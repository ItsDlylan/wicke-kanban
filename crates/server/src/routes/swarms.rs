use axum::{
    Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{swarm::Swarm, swarm_agent::SwarmAgent, swarm_succession::SwarmSuccession};
use deployment::Deployment;
use serde::Serialize;
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

#[derive(Debug, Serialize, TS)]
pub struct SwarmWithAgents {
    #[serde(flatten)]
    pub swarm: Swarm,
    pub agents: Vec<SwarmAgent>,
}

#[derive(Debug, Serialize, TS)]
pub struct SwarmOverview {
    #[serde(flatten)]
    pub swarm: Swarm,
    pub agents: Vec<SwarmAgent>,
    pub successions: Vec<SwarmSuccession>,
}

pub async fn get_swarm_by_task_id(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Option<SwarmOverview>>>, ApiError> {
    let pool = &deployment.db().pool;

    let swarm = Swarm::find_active_by_task_id(pool, task_id).await?;

    let overview = match swarm {
        Some(swarm) => {
            let agents = SwarmAgent::find_by_swarm_id(pool, swarm.id).await?;
            let successions = SwarmSuccession::find_by_swarm_id(pool, swarm.id).await?;
            Some(SwarmOverview {
                swarm,
                agents,
                successions,
            })
        }
        None => None,
    };

    Ok(ResponseJson(ApiResponse::success(overview)))
}

pub async fn get_swarm(
    State(deployment): State<DeploymentImpl>,
    Path(swarm_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<SwarmWithAgents>>, ApiError> {
    let pool = &deployment.db().pool;

    let swarm = Swarm::find_by_id(pool, swarm_id)
        .await?
        .ok_or(ApiError::BadRequest("Swarm not found".to_string()))?;

    let agents = SwarmAgent::find_by_swarm_id(pool, swarm.id).await?;

    Ok(ResponseJson(ApiResponse::success(SwarmWithAgents {
        swarm,
        agents,
    })))
}

pub async fn get_swarm_agents(
    State(deployment): State<DeploymentImpl>,
    Path(swarm_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<SwarmAgent>>>, ApiError> {
    let pool = &deployment.db().pool;
    let agents = SwarmAgent::find_by_swarm_id(pool, swarm_id).await?;
    Ok(ResponseJson(ApiResponse::success(agents)))
}

pub async fn get_swarm_successions(
    State(deployment): State<DeploymentImpl>,
    Path(swarm_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<SwarmSuccession>>>, ApiError> {
    let pool = &deployment.db().pool;
    let successions = SwarmSuccession::find_by_swarm_id(pool, swarm_id).await?;
    Ok(ResponseJson(ApiResponse::success(successions)))
}

pub async fn cancel_swarm(
    State(deployment): State<DeploymentImpl>,
    Path(swarm_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    services::services::swarm_coordinator::cancel_swarm(
        &deployment.db().pool,
        deployment.container(),
        swarm_id,
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(ResponseJson(ApiResponse::success(())))
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let swarm_id_router = Router::new()
        .route("/", get(get_swarm))
        .route("/agents", get(get_swarm_agents))
        .route("/successions", get(get_swarm_successions))
        .route("/cancel", post(cancel_swarm));

    Router::new()
        .route("/tasks/{task_id}/swarm", get(get_swarm_by_task_id))
        .nest("/swarms/{swarm_id}", swarm_id_router)
}
