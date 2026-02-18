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
    spec_generator,
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

// --- Generate spec with AI ---

#[derive(Debug, Serialize, TS)]
pub struct GenerateSpecResponse {
    pub overview: String,
    pub requirements: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub constraints: Vec<String>,
    pub tech_notes: String,
}

pub async fn generate_spec(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<GenerateSpecResponse>>, ApiError> {
    let pool = &deployment.db().pool;

    // Load the task
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    // Get repo path for working directory
    let repos = ProjectRepo::find_repos_for_project(pool, task.project_id).await?;
    let working_dir = repos.first().map(|r| r.path.clone()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // Build prompt with title, description, and plan
    let prompt = spec_generator::build_spec_generation_prompt(
        &task.title,
        task.description.as_deref(),
        task.plan.as_deref(),
    );

    // Run claude --print in a blocking thread
    let raw_output = tokio::task::spawn_blocking(move || {
        spec_generator::run_spec_generation(&prompt, &working_dir)
    })
    .await
    .map_err(|e| ApiError::BadRequest(format!("Spec generation task panicked: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("Spec generation failed: {e}")))?;

    // Parse response
    let spec = spec_generator::parse_spec_output(&raw_output)
        .map_err(|e| ApiError::BadRequest(format!("Failed to parse spec output: {e}")))?;

    Ok(ResponseJson(ApiResponse::success(GenerateSpecResponse {
        overview: spec.overview,
        requirements: spec.requirements,
        acceptance_criteria: spec.acceptance_criteria,
        constraints: spec.constraints,
        tech_notes: spec.tech_notes,
    })))
}

// --- Decompose endpoint (decompose only, no execution) ---

pub async fn decompose_task(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let pool = &deployment.db().pool;

    // 1. Load task (must be in Ready status)
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    if task.status != TaskStatus::Ready {
        return Err(ApiError::BadRequest(format!(
            "Task must be in 'ready' status to decompose, current status: {}",
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

    if task.status != TaskStatus::Ready && task.status != TaskStatus::Ralph {
        return Err(ApiError::BadRequest(format!(
            "Task must be in 'ready' or 'ralph' status to start a sprint, current status: {}",
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
            if child.status != TaskStatus::Ready {
                return Err(ApiError::BadRequest(format!(
                    "Child task {} ({}) must be in 'ready' status, current: {}",
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

    // Auto-create a draft PR so diffs accumulate as each story pushes
    deployment
        .container()
        .auto_create_pr(&ws, &task, true)
        .await;

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

// --- Make Ralph Ready endpoint (upsert spec + decompose in one call) ---

#[derive(Debug, Deserialize, TS)]
pub struct MakeRalphReadyRequest {
    pub overview: Option<String>,
    pub requirements: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub constraints: Option<String>,
    pub tech_notes: Option<String>,
}

pub async fn make_ralph_ready(
    State(deployment): State<DeploymentImpl>,
    Path(task_id): Path<Uuid>,
    Json(payload): Json<MakeRalphReadyRequest>,
) -> Result<ResponseJson<ApiResponse<Vec<Task>>>, ApiError> {
    let pool = &deployment.db().pool;

    // 1. Load task
    let task = Task::find_by_id(pool, task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    // 2. Upsert spec sheet
    let spec_data = CreateSpecSheet {
        overview: payload.overview,
        requirements: payload.requirements,
        acceptance_criteria: payload.acceptance_criteria,
        constraints: payload.constraints,
        tech_notes: payload.tech_notes,
    };
    let spec = SpecSheet::upsert(pool, task_id, &spec_data).await?;

    // 3. Generate decomposition prompt
    let prompt = decomposer::generate_decomposition_prompt(&spec, &task.title);

    // 4. Get repo path for working directory
    let repos = ProjectRepo::find_repos_for_project(pool, task.project_id).await?;
    let working_dir = repos.first().map(|r| r.path.clone()).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // 5. Run decomposition
    let raw_output =
        tokio::task::spawn_blocking(move || decomposer::run_decomposition(&prompt, &working_dir))
            .await
            .map_err(|e| ApiError::BadRequest(format!("Decomposition task panicked: {e}")))?
            .map_err(|e| ApiError::BadRequest(format!("Decomposition failed: {e}")))?;

    // 6. Parse response
    let decomposition = decomposer::parse_decomposition_output(&raw_output)
        .map_err(|e| ApiError::BadRequest(format!("Failed to parse decomposition: {e}")))?;

    // 7. Create child tasks
    let child_tasks =
        decomposer::create_child_tasks(pool, task.project_id, task_id, &decomposition)
            .await
            .map_err(|e| ApiError::BadRequest(format!("Failed to create child tasks: {e}")))?;

    // Task stays in current status (Ready)
    Ok(ResponseJson(ApiResponse::success(child_tasks)))
}

// --- Ralph Session endpoints ---

#[derive(Debug, Deserialize, TS)]
pub struct CreateRalphSessionRequest {
    pub task_ids: Vec<Uuid>,
    pub repos: Vec<SprintRepoInput>,
    pub executor_profile_id: Option<ExecutorProfileId>,
}

#[derive(Debug, Serialize, TS)]
pub struct RalphSessionResponse {
    pub session: db::models::ralph_session::RalphSession,
    pub tasks: Vec<db::models::ralph_session::RalphSessionTask>,
}

pub async fn create_ralph_session(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateRalphSessionRequest>,
) -> Result<ResponseJson<ApiResponse<RalphSessionResponse>>, ApiError> {
    use db::models::ralph_session::{RalphSession, RalphSessionStatus, RalphSessionTask};

    let pool = &deployment.db().pool;

    if payload.task_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "At least one task must be selected".to_string(),
        ));
    }

    // Validate all tasks exist and are in Ready status with spec + children
    let mut project_id = None;
    for (order, task_id) in payload.task_ids.iter().enumerate() {
        let task = Task::find_by_id(pool, *task_id)
            .await?
            .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

        if task.status != TaskStatus::Ready {
            return Err(ApiError::BadRequest(format!(
                "Task '{}' must be in 'ready' status, current: {}",
                task.title, task.status
            )));
        }

        // Verify has spec
        if SpecSheet::find_by_task_id(pool, *task_id).await?.is_none() {
            return Err(ApiError::BadRequest(format!(
                "Task '{}' has no spec sheet",
                task.title
            )));
        }

        // Verify has children
        let children = Task::find_by_parent_task_id(pool, *task_id).await?;
        if children.is_empty() {
            return Err(ApiError::BadRequest(format!(
                "Task '{}' has no children (needs decomposition first)",
                task.title
            )));
        }

        if order == 0 {
            project_id = Some(task.project_id);
        }
    }

    let project_id = project_id.unwrap();
    let session_id = Uuid::new_v4();

    // Create ralph session
    let session = RalphSession::create(pool, session_id, project_id, None).await?;

    // Create session tasks with execution order
    let task_orders: Vec<(Uuid, i32)> = payload
        .task_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i as i32))
        .collect();
    RalphSessionTask::create_many(pool, session_id, &task_orders).await?;

    // Create shared workspace for the first task
    let first_task_id = payload.task_ids[0];
    let workspace_id = Uuid::new_v4();
    let short_uuid = &workspace_id.to_string()[..8];
    let branch = deployment
        .container()
        .git_branch_from_workspace(&workspace_id, &format!("ralph-session-{}", short_uuid))
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
        first_task_id,
    )
    .await?;

    // Create workspace repos
    let workspace_repos: Vec<CreateWorkspaceRepo> = payload
        .repos
        .iter()
        .map(|r| CreateWorkspaceRepo {
            repo_id: r.repo_id,
            target_branch: r.target_branch.clone(),
        })
        .collect();
    WorkspaceRepo::create_many(pool, ws.id, &workspace_repos).await?;

    // Create container
    deployment.container().create(&ws).await?;

    // Auto-create a draft PR so diffs accumulate as each story pushes
    let first_task = Task::find_by_id(pool, first_task_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;
    deployment
        .container()
        .auto_create_pr(&ws, &first_task, true)
        .await;

    // Update session with workspace
    RalphSession::update_workspace_id(pool, session_id, ws.id).await?;
    RalphSession::update_status(pool, session_id, RalphSessionStatus::Running).await?;

    // Move all session tasks to Ralph status and assign first task's children
    for task_id in &payload.task_ids {
        Task::update_status(pool, *task_id, TaskStatus::Ralph).await?;
    }

    // Assign first task's children to the workspace
    let first_children = Task::find_by_parent_task_id(pool, first_task_id).await?;
    for child in &first_children {
        Task::update_parent_workspace_id(pool, child.id, Some(ws.id)).await?;
    }

    // Start first eligible child
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
        tracing::error!("Failed to start first child task in ralph session: {e}");
    }

    let session_tasks = RalphSessionTask::find_by_session_id(pool, session_id).await?;

    Ok(ResponseJson(ApiResponse::success(RalphSessionResponse {
        session,
        tasks: session_tasks,
    })))
}

pub async fn get_ralph_session(
    State(deployment): State<DeploymentImpl>,
    Path(session_id): Path<Uuid>,
) -> Result<ResponseJson<ApiResponse<RalphSessionResponse>>, ApiError> {
    use db::models::ralph_session::{RalphSession, RalphSessionTask};

    let pool = &deployment.db().pool;

    let session = RalphSession::find_by_id(pool, session_id)
        .await?
        .ok_or(ApiError::Database(sqlx::Error::RowNotFound))?;

    let tasks = RalphSessionTask::find_by_session_id(pool, session_id).await?;

    Ok(ResponseJson(ApiResponse::success(RalphSessionResponse {
        session,
        tasks,
    })))
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

    let make_ralph_ready_routes = Router::new().route("/", post(make_ralph_ready));

    let generate_spec_routes = Router::new().route("/", post(generate_spec));

    let ralph_session_routes = Router::new()
        .route("/", post(create_ralph_session))
        .route("/{session_id}", get(get_ralph_session));

    Router::new()
        .nest("/tasks/{task_id}/spec", spec_routes)
        .nest("/tasks/{task_id}/plan", plan_routes)
        .nest("/tasks/{task_id}/ralph", ralph_routes)
        .nest("/tasks/{task_id}/decompose", decompose_routes)
        .nest("/tasks/{task_id}/children", children_routes)
        .nest("/tasks/{task_id}/make-ralph-ready", make_ralph_ready_routes)
        .nest("/tasks/{task_id}/generate-spec", generate_spec_routes)
        .nest("/ralph-sessions", ralph_session_routes)
}
