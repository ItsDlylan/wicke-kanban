use axum::{
    Router,
    extract::{
        Path, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use db::models::{
    swarm::Swarm, swarm_agent::SwarmAgent, swarm_agent_dependency::SwarmAgentDependency,
    swarm_succession::SwarmSuccession,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
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
    pub dependencies: Vec<SwarmAgentDependency>,
}

#[derive(Debug, Serialize, TS)]
pub struct SwarmSummary {
    pub id: Uuid,
    pub status: String,
    pub routing_decision: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub agent_count: i64,
}

/// Build a full SwarmOverview for a given swarm.
pub async fn build_swarm_overview(
    pool: &sqlx::SqlitePool,
    swarm: Swarm,
) -> Result<SwarmOverview, sqlx::Error> {
    let agents = SwarmAgent::find_by_swarm_id(pool, swarm.id).await?;
    let successions = SwarmSuccession::find_by_swarm_id(pool, swarm.id).await?;
    let dependencies = SwarmAgentDependency::find_by_swarm_agents(pool, swarm.id).await?;
    Ok(SwarmOverview {
        swarm,
        agents,
        successions,
        dependencies,
    })
}

pub async fn get_swarm_by_task_id(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Option<SwarmOverview>>>, ApiError> {
    let pool = &deployment.db().pool;

    let swarm = Swarm::find_active_by_task_id(pool, task_id).await?;

    let overview = match swarm {
        Some(swarm) => Some(build_swarm_overview(pool, swarm).await?),
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

/// GET /api/tasks/{task_id}/swarms — list all swarms for a task (history)
pub async fn list_swarms_for_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<SwarmSummary>>>, ApiError> {
    let pool = &deployment.db().pool;
    let swarms = Swarm::find_by_task_id(pool, task_id).await?;

    let mut summaries = Vec::with_capacity(swarms.len());
    for s in swarms {
        let agent_count = SwarmAgent::count_by_swarm_id(pool, s.id).await?;
        summaries.push(SwarmSummary {
            id: s.id,
            status: s.status.to_string(),
            routing_decision: s.routing_decision,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            agent_count,
        });
    }

    Ok(ResponseJson(ApiResponse::success(summaries)))
}

/// WS /api/tasks/{task_id}/swarm/ws — real-time swarm overview stream
pub async fn stream_swarm_overview_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_swarm_overview_ws(socket, deployment, task_id).await {
            tracing::warn!("swarm overview WS closed: {}", e);
        }
    })
}

async fn handle_swarm_overview_ws(
    socket: WebSocket,
    deployment: DeploymentImpl,
    task_id: Uuid,
) -> anyhow::Result<()> {
    let mut stream = deployment
        .events()
        .stream_swarm_overview_raw(task_id)
        .await?
        .map_ok(|msg| msg.to_ws_message_unchecked());

    let (mut sender, mut receiver) = socket.split();

    // Drain client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::error!("swarm overview stream error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

pub fn router(_deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let swarm_id_router = Router::new()
        .route("/", get(get_swarm))
        .route("/agents", get(get_swarm_agents))
        .route("/successions", get(get_swarm_successions))
        .route("/cancel", post(cancel_swarm));

    Router::new()
        .route("/tasks/{task_id}/swarm", get(get_swarm_by_task_id))
        .route("/tasks/{task_id}/swarms", get(list_swarms_for_task))
        .route("/tasks/{task_id}/swarm/ws", get(stream_swarm_overview_ws))
        .nest("/swarms/{swarm_id}", swarm_id_router)
}
