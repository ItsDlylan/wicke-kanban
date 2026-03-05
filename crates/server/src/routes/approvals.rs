use std::collections::HashMap;

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::pending_approval::PendingApprovalRecord;
use deployment::Deployment;
use serde::Deserialize;
use services::services::auto_planner;
use utils::{
    approvals::{ApprovalResponse, ApprovalStatus},
    response::ApiResponse,
};
use uuid::Uuid;

use crate::DeploymentImpl;

pub async fn respond_to_approval(
    State(deployment): State<DeploymentImpl>,
    Path(id): Path<String>,
    ResponseJson(request): ResponseJson<ApprovalResponse>,
) -> Result<ResponseJson<ApiResponse<ApprovalStatus>>, StatusCode> {
    let service = deployment.approvals();
    let pool = &deployment.db().pool;

    match service.respond(pool, &id, request.clone()).await {
        Ok((status, context)) => {
            // Check if this was an orphaned approval that needs re-planning
            if matches!(status, ApprovalStatus::Approved) {
                if let Ok(Some(record)) = PendingApprovalRecord::find_by_id(pool, &id).await {
                    // If the execution process is dead, this is a recovered approval — trigger re-plan
                    if let Ok(Some(ep)) =
                        db::models::execution_process::ExecutionProcess::find_by_id(
                            pool,
                            record.execution_process_id,
                        )
                        .await
                    {
                        use db::models::execution_process::ExecutionProcessStatus;
                        if matches!(
                            ep.status,
                            ExecutionProcessStatus::Failed
                                | ExecutionProcessStatus::Killed
                                | ExecutionProcessStatus::Completed
                        ) {
                            // Parse the original tool_input to extract questions and build answers
                            let answers = build_answers_from_approval(
                                &record,
                                request.updated_input.as_ref(),
                            );

                            if let Ok(Some(task)) =
                                db::models::task::Task::find_by_id(pool, record.task_id).await
                            {
                                // Only re-plan if the task is still in PlanGenerating status
                                use db::models::task::TaskStatus;
                                if task.status == TaskStatus::PlanGenerating {
                                    tracing::info!(
                                        "Re-planning task {} with {} user answers from recovered approval",
                                        task.id,
                                        answers.len()
                                    );
                                    auto_planner::spawn_auto_plan_with_answers(
                                        deployment.container_cloned(),
                                        pool.clone(),
                                        task.id,
                                        task.project_id,
                                        task.title.clone(),
                                        task.description.clone(),
                                        answers,
                                    );
                                } else {
                                    tracing::info!(
                                        "Skipping re-plan for task {} — status is {:?}, not PlanGenerating",
                                        task.id,
                                        task.status
                                    );
                                }
                            }
                        }
                    }
                }
            }

            deployment
                .track_if_analytics_allowed(
                    "approval_responded",
                    serde_json::json!({
                        "approval_id": &id,
                        "status": format!("{:?}", status),
                        "tool_name": context.tool_name,
                        "execution_process_id": context.execution_process_id.to_string(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(status)))
        }
        Err(e) => {
            tracing::error!("Failed to respond to approval: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Build a question→answer map from a recovered approval record.
fn build_answers_from_approval(
    record: &PendingApprovalRecord,
    updated_input: Option<&serde_json::Value>,
) -> HashMap<String, String> {
    let mut answers = HashMap::new();

    // The tool_input contains the AskUserQuestion payload with questions/options
    let tool_input: serde_json::Value =
        serde_json::from_str(&record.tool_input).unwrap_or_default();

    // The updated_input (or response_input) contains the user's answers
    let response = updated_input
        .cloned()
        .or_else(|| {
            record
                .response_input
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
        })
        .unwrap_or_default();

    // Extract questions from tool_input
    if let Some(questions) = tool_input.get("questions").and_then(|q| q.as_array()) {
        // Extract answers from response
        let response_answers = response
            .get("answers")
            .and_then(|a| a.as_object())
            .cloned()
            .unwrap_or_default();

        for question in questions {
            if let Some(q_text) = question.get("question").and_then(|q| q.as_str()) {
                if let Some(answer) = response_answers.get(q_text).and_then(|a| a.as_str()) {
                    answers.insert(q_text.to_string(), answer.to_string());
                }
            }
        }
    }

    answers
}

#[derive(Debug, Deserialize)]
pub struct PendingApprovalsQuery {
    pub task_id: Uuid,
}

pub async fn get_pending_approvals(
    State(deployment): State<DeploymentImpl>,
    Query(query): Query<PendingApprovalsQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<PendingApprovalRecord>>>, StatusCode> {
    let pool = &deployment.db().pool;

    match PendingApprovalRecord::find_pending_by_task_id(pool, query.task_id).await {
        Ok(approvals) => Ok(ResponseJson(ApiResponse::success(approvals))),
        Err(e) => {
            tracing::error!(
                "Failed to fetch pending approvals for task {}: {:?}",
                query.task_id,
                e
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn router() -> Router<DeploymentImpl> {
    Router::new()
        .route("/approvals/{id}/respond", post(respond_to_approval))
        .route("/approvals/pending", get(get_pending_approvals))
}
