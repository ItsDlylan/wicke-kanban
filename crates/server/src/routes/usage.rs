use axum::{Router, extract::State, response::Json as ResponseJson, routing::get};
use deployment::Deployment;
use services::services::usage_poller::ClaudeUsageData;
use utils::response::ApiResponse;

use crate::DeploymentImpl;

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/usage", get(get_usage))
}

async fn get_usage(
    State(deployment): State<DeploymentImpl>,
) -> ResponseJson<ApiResponse<Option<ClaudeUsageData>>> {
    let guard = deployment.usage_data().read().await;
    ResponseJson(ApiResponse::success(guard.clone()))
}
