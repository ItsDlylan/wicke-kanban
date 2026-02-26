use axum::{Router, extract::Extension, response::Json as ResponseJson, routing::get};
use services::services::usage_poller::{ClaudeUsageData, UsageCache};
use utils::response::ApiResponse;

use crate::DeploymentImpl;

async fn get_usage(
    Extension(cache): Extension<UsageCache>,
) -> ResponseJson<ApiResponse<ClaudeUsageData>> {
    let guard = cache.read().await;
    let data = guard.clone().unwrap_or_default();
    ResponseJson(ApiResponse::success(data))
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/usage", get(get_usage))
}
