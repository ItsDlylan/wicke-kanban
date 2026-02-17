use axum::{
    Json, Router,
    extract::{Path, State},
    response::Json as ResponseJson,
    routing::{get, post},
};
use db::models::{
    project_repo::ProjectRepo,
    repo::Repo,
    spec_sheet::{CreateSpecSheet, SpecSheet},
    task::{Task, TaskStatus},
    task_dependency::TaskDependency,
    workspace::{CreateWorkspace, Workspace},
    workspace_repo::{CreateWorkspaceRepo, WorkspaceRepo},
};
use deployment::Deployment;
use executors::profile::ExecutorProfileId;
use serde::{Deserialize, Serialize};
use services::services::{
    container::ContainerService,
    decomposer, plan_generator,
    ralph_loop::{self, WorkspaceRepoInput},
};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError};

pub async fn get_spec(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Option<SpecSheet>>>, ApiError> {
    let spec = SpecSheet::find_by_task_id(&deployment.db().pool, task_id).await?;
    Ok(ResponseJson(ApiResponse::success(spec)))
}

pub async fn create_or_update_spec(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<CreateSpecSheet>,
) -> Result<ResponseJson<ApiResponse<SpecSheet>>, ApiError> {
    // Verify task exists
    Task::find_by_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let spec = SpecSheet::upsert(&deployment.db().pool, task_id, &payload).await?;

    // Auto-advance task status to Spec if currently in Backlog or Todo
    if let Some(task) = Task::find_by_id(&deployment.db().pool, task_id).await? {
        if task.status == TaskStatus::Backlog || task.status == TaskStatus::Todo {
            Task::update_status(&deployment.db().pool, task_id, TaskStatus::Spec).await?;
        }
    }

    Ok(ResponseJson(ApiResponse::success(spec)))
}

pub async fn delete_spec(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    let rows_affected = SpecSheet::delete_by_task_id(&deployment.db().pool, task_id).await?;
    if rows_affected == 0 {
        Err(ApiError::Database(sqlx::Error::RowNotFound))
    } else {
        Ok(ResponseJson(ApiResponse::success(())))
    }
}

pub async fn generate_plan(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<String>>, ApiError> {
    let task = Task::find_by_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let spec = SpecSheet::find_by_task_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let plan_prompt = plan_generator::generate_plan_prompt(&spec, &task.title);

    // Auto-advance task status to Plan if currently in Spec
    if task.status == TaskStatus::Spec {
        Task::update_status(&deployment.db().pool, task_id, TaskStatus::Plan).await?;
    }

    Ok(ResponseJson(ApiResponse::success(plan_prompt)))
}

pub async fn start_ralph(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<String>>, ApiError> {
    let task = Task::find_by_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let spec = SpecSheet::find_by_task_id(&deployment.db().pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    // Generate the plan prompt and build the ralph execution prompt
    let plan_prompt = plan_generator::generate_plan_prompt(&spec, &task.title);
    let ralph_prompt = ralph_loop::build_ralph_prompt(&plan_prompt, &task.title);

    // Advance task status to Ralph
    ralph_loop::start_ralph_loop(&deployment.db().pool, task_id).await?;

    Ok(ResponseJson(ApiResponse::success(ralph_prompt)))
}

pub async fn complete_ralph(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    ralph_loop::complete_ralph_loop(&deployment.db().pool, task_id).await?;
    Ok(ResponseJson(ApiResponse::success(())))
}

// --- Decompose endpoint (decompose only, no execution) ---

pub async fn decompose_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let pool = &deployment.db().pool;

    // 1. Load task (must be in Spec or Plan status)
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    if task.status != TaskStatus::Spec && task.status != TaskStatus::Plan {
        return Err(ApiError::BadRequest(format!(
            "Task must be in 'spec' or 'plan' status to decompose, current status: {}",
            task.status
        )));
    }

    // 2. Load spec sheet
    let spec = SpecSheet::find_by_task_id(pool, task_id)
        .await?
        .ok_or(ApiError::BadRequest("Task has no spec sheet".to_string()))?;

    // 3. Generate decomposition prompt
    let prompt = decomposer::generate_decomposition_prompt(&spec, &task.title);

    // 4. Get repo path for working directory
    let repos = ProjectRepo::find_repos_for_project(pool, task.project_id).await?;
    let working_dir = repos.first().map(|r| r.path.clone()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // 5. Run decomposition (blocking call to claude --print)
    let raw_output =
        tokio::task::spawn_blocking(move || decomposer::run_decomposition(&prompt, &working_dir))
            .await
            .map_err(|e| ApiError::BadRequest(format!("Decomposition task panicked: {e}")))?
            .map_err(|e| ApiError::BadRequest(format!("Decomposition failed: {e}")))?;

    // 6. Parse response
    let decomposition = decomposer::parse_decomposition_output(&raw_output)
        .map_err(|e| ApiError::BadRequest(format!("Failed to parse decomposition: {e}")))?;

    // 7. Create child tasks with parent_task_id (no workspace yet)
    let child_tasks =
        decomposer::create_child_tasks(pool, task.project_id, task_id, &decomposition)
            .await
            .map_err(|e| ApiError::BadRequest(format!("Failed to create child tasks: {e}")))?;

    // 8. Return created child tasks
    Ok(ResponseJson(ApiResponse::success(child_tasks)))
}

// --- Get children with dependency info ---

#[derive(Debug, Serialize, TS)]
pub struct ChildTaskWithDeps {
    #[serde(flatten)]
    #[ts(flatten)]
    pub task: Task,
    pub dependencies: Vec<Uuid>,
    pub is_ready: bool,
    pub sprint_workspace_id: Option<Uuid>,
}

pub async fn get_children(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<ChildTaskWithDeps>>>, ApiError> {
    let pool = &deployment.db().pool;

    let children = Task::find_by_parent_task_id(pool, task_id).await?;

    let mut result = Vec::with_capacity(children.len());
    for child in children {
        let deps = TaskDependency::find_dependencies(pool, child.id).await?;

        // Check if all dependencies are done
        let is_ready = if deps.is_empty() {
            true
        } else {
            let mut all_done = true;
            for dep_id in &deps {
                if let Some(dep_task) = Task::find_by_id(pool, *dep_id).await? {
                    if dep_task.status != TaskStatus::Done {
                        all_done = false;
                        break;
                    }
                }
            }
            all_done
        };

        let sprint_workspace_id = child.parent_workspace_id;

        result.push(ChildTaskWithDeps {
            task: child,
            dependencies: deps,
            is_ready,
            sprint_workspace_id,
        });
    }

    Ok(ResponseJson(ApiResponse::success(result)))
}

// --- Start sprint endpoint ---

#[derive(Debug, Deserialize, TS)]
pub struct SprintRepoInput {
    pub repo_id: Uuid,
    pub target_branch: String,
}

#[derive(Debug, Deserialize, TS)]
pub struct StartSprintRequest {
    pub task_ids: Vec<Uuid>,
    pub executor_profile_id: Option<ExecutorProfileId>,
    pub repos: Vec<SprintRepoInput>,
}

pub async fn start_sprint(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<StartSprintRequest>,
) -> Result<ResponseJson<ApiResponse<String>>, ApiError> {
    let pool = &deployment.db().pool;

    // 1. Load parent task
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    if task.status != TaskStatus::Spec
        && task.status != TaskStatus::Plan
        && task.status != TaskStatus::Ralph
    {
        return Err(ApiError::BadRequest(format!(
            "Task must be in 'spec', 'plan', or 'ralph' status to start a sprint, current status: {}",
            task.status
        )));
    }

    // 2. Validate all task_ids are children of this parent
    let children = Task::find_by_parent_task_id(pool, task_id).await?;
    let child_ids: Vec<Uuid> = children.iter().map(|c| c.id).collect();
    for tid in &payload.task_ids {
        if !child_ids.contains(tid) {
            return Err(ApiError::BadRequest(format!(
                "Task {} is not a child of parent task {}",
                tid, task_id
            )));
        }
    }

    // 3. Validate selected children are in Todo status
    for tid in &payload.task_ids {
        if let Some(child) = children.iter().find(|c| c.id == *tid) {
            if child.status != TaskStatus::Todo {
                return Err(ApiError::BadRequest(format!(
                    "Child task {} ({}) must be in 'todo' status, current: {}",
                    tid, child.title, child.status
                )));
            }
        }
    }

    // 4. Create workspace for the sprint
    let workspace_id = Uuid::new_v4();
    let short_uuid = &workspace_id.to_string()[..8];
    let branch = deployment
        .container()
        .git_branch_from_workspace(&workspace_id, &format!("sprint-{}", short_uuid))
        .await;

    let agent_working_dir = if payload.repos.len() == 1 {
        let repo = Repo::find_by_id(pool, payload.repos[0].repo_id)
            .await?
            .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;
        match repo.default_working_dir {
            Some(subdir) => {
                let path = std::path::PathBuf::from(&repo.name).join(&subdir);
                Some(path.to_string_lossy().to_string())
            }
            None => Some(repo.name),
        }
    } else {
        None
    };

    let ws = Workspace::create(
        pool,
        &CreateWorkspace {
            branch,
            agent_working_dir,
        },
        workspace_id,
        task_id,
    )
    .await?;

    // 5. Create workspace repos
    let workspace_repos: Vec<CreateWorkspaceRepo> = payload
        .repos
        .iter()
        .map(|r| CreateWorkspaceRepo {
            repo_id: r.repo_id,
            target_branch: r.target_branch.clone(),
        })
        .collect();
    WorkspaceRepo::create_many(pool, ws.id, &workspace_repos).await?;

    // 6. Create container
    deployment.container().create(&ws).await?;

    // 7. Assign selected children to this sprint workspace
    for tid in &payload.task_ids {
        Task::update_parent_workspace_id(pool, *tid, Some(ws.id)).await?;
    }

    // 8. Advance parent to Ralph status (if not already)
    ralph_loop::start_ralph_loop(pool, task_id).await?;

    // 9. Start first eligible child
    let executor_profile_id = payload.executor_profile_id.unwrap_or_else(|| {
        ExecutorProfileId::new(executors::executors::BaseCodingAgent::ClaudeCode)
    });

    let ralph_repos: Vec<WorkspaceRepoInput> = payload
        .repos
        .iter()
        .map(|r| WorkspaceRepoInput {
            repo_id: r.repo_id,
            target_branch: r.target_branch.clone(),
        })
        .collect();

    if let Err(e) = ralph_loop::start_next_child(
        pool,
        deployment.container(),
        ws.id,
        &executor_profile_id,
        &ralph_repos,
    )
    .await
    {
        tracing::error!("Failed to start first child task in sprint: {e}");
    }

    // 10. Return sprint workspace ID
    Ok(ResponseJson(ApiResponse::success(ws.id.to_string())))
}

pub fn router() -> Router<DeploymentImpl> {
    let spec_routes = Router::new().route(
        "/",
        get(get_spec)
            .post(create_or_update_spec)
            .delete(delete_spec),
    );

    let plan_routes = Router::new().route("/", post(generate_plan));

    let ralph_routes = Router::new()
        .route("/start", post(start_ralph))
        .route("/complete", post(complete_ralph))
        .route("/start-sprint", post(start_sprint));

    let decompose_routes = Router::new().route("/", post(decompose_task));

    let children_routes = Router::new().route("/", get(get_children));

    Router::new()
        .nest("/tasks/{task_id}/spec", spec_routes)
        .nest("/tasks/{task_id}/plan", plan_routes)
        .nest("/tasks/{task_id}/ralph", ralph_routes)
        .nest("/tasks/{task_id}/decompose", decompose_routes)
        .nest("/tasks/{task_id}/children", children_routes)
}
