use std::path::PathBuf;

use anyhow;
use axum::{
    Extension, Json, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware::from_fn_with_state,
    response::{IntoResponse, Json as ResponseJson},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use db::models::{
    project::{CreateProject, Project, ProjectError, SearchResult, UpdateProject},
    project_repo::{CreateProjectRepo, ProjectRepo},
    repo::Repo,
};
use deployment::Deployment;
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use serde::Serialize;
use services::services::{file_search::SearchQuery, project::ProjectServiceError};
use ts_rs::TS;
use utils::response::ApiResponse;
use uuid::Uuid;

use crate::{DeploymentImpl, error::ApiError, middleware::load_project_middleware};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ProjectStats {
    pub project_id: Uuid,
    pub commits_this_month: u32,
    pub lines_added_this_month: u32,
    pub lines_removed_this_month: u32,
    pub last_commit_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AllProjectStats {
    pub stats: Vec<ProjectStats>,
}

pub async fn get_projects(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Project>>>, ApiError> {
    let projects = Project::find_all(&deployment.db().pool).await?;
    Ok(ResponseJson(ApiResponse::success(projects)))
}

pub async fn stream_projects_ws(
    ws: WebSocketUpgrade,
    State(deployment): State<DeploymentImpl>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_projects_ws(socket, deployment).await {
            tracing::warn!("projects WS closed: {}", e);
        }
    })
}

async fn handle_projects_ws(socket: WebSocket, deployment: DeploymentImpl) -> anyhow::Result<()> {
    let mut stream = deployment
        .events()
        .stream_projects_raw()
        .await?
        .map_ok(|msg| msg.to_ws_message_unchecked());

    // Split socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Drain (and ignore) any client->server messages so pings/pongs work
    tokio::spawn(async move { while let Some(Ok(_)) = receiver.next().await {} });

    // Forward server messages
    while let Some(item) = stream.next().await {
        match item {
            Ok(msg) => {
                if sender.send(msg).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                tracing::error!("stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

pub async fn get_project(
    Extension(project): Extension<Project>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    Ok(ResponseJson(ApiResponse::success(project)))
}

pub async fn create_project(
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, ApiError> {
    tracing::debug!("Creating project '{}'", payload.name);
    let repo_count = payload.repositories.len();

    match deployment
        .project()
        .create_project(&deployment.db().pool, deployment.repo(), payload)
        .await
    {
        Ok(project) => {
            // Track project creation event
            deployment
                .track_if_analytics_allowed(
                    "project_created",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "repository_count": repo_count,
                        "trigger": "manual",
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(project)))
        }
        Err(ProjectServiceError::DuplicateGitRepoPath) => Ok(ResponseJson(ApiResponse::error(
            "Duplicate repository path provided",
        ))),
        Err(ProjectServiceError::DuplicateRepositoryName) => Ok(ResponseJson(ApiResponse::error(
            "Duplicate repository name provided",
        ))),
        Err(ProjectServiceError::PathNotFound(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified path does not exist",
        ))),
        Err(ProjectServiceError::PathNotDirectory(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified path is not a directory",
        ))),
        Err(ProjectServiceError::NotGitRepository(_)) => Ok(ResponseJson(ApiResponse::error(
            "The specified directory is not a git repository",
        ))),
        Err(e) => Err(ProjectError::CreateFailed(e.to_string()).into()),
    }
}

pub async fn update_project(
    Extension(existing_project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<UpdateProject>,
) -> Result<ResponseJson<ApiResponse<Project>>, StatusCode> {
    match deployment
        .project()
        .update_project(&deployment.db().pool, &existing_project, payload)
        .await
    {
        Ok(project) => Ok(ResponseJson(ApiResponse::success(project))),
        Err(e) => {
            tracing::error!("Failed to update project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn delete_project(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match deployment
        .project()
        .delete_project(&deployment.db().pool, project.id)
        .await
    {
        Ok(rows_affected) => {
            if rows_affected == 0 {
                Err(StatusCode::NOT_FOUND)
            } else {
                deployment
                    .track_if_analytics_allowed(
                        "project_deleted",
                        serde_json::json!({
                            "project_id": project.id.to_string(),
                        }),
                    )
                    .await;

                Ok(ResponseJson(ApiResponse::success(())))
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete project: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(serde::Deserialize)]
pub struct OpenEditorRequest {
    pub editor_type: Option<String>,
    pub git_repo_path: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize, ts_rs::TS)]
pub struct OpenEditorResponse {
    pub url: Option<String>,
}

pub async fn open_project_in_editor(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<Option<OpenEditorRequest>>,
) -> Result<ResponseJson<ApiResponse<OpenEditorResponse>>, ApiError> {
    let path = if let Some(ref req) = payload
        && let Some(ref specified_path) = req.git_repo_path
    {
        specified_path.clone()
    } else {
        let repositories = deployment
            .project()
            .get_repositories(&deployment.db().pool, project.id)
            .await?;

        repositories
            .first()
            .map(|r| r.path.clone())
            .ok_or_else(|| ApiError::BadRequest("Project has no repositories".to_string()))?
    };

    let editor_config = {
        let config = deployment.config().read().await;
        let editor_type_str = payload.as_ref().and_then(|req| req.editor_type.as_deref());
        config.editor.with_override(editor_type_str)
    };

    match editor_config.open_file(&path).await {
        Ok(url) => {
            tracing::info!(
                "Opened editor for project {} at path: {}{}",
                project.id,
                path.to_string_lossy(),
                if url.is_some() { " (remote mode)" } else { "" }
            );

            deployment
                .track_if_analytics_allowed(
                    "project_editor_opened",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "editor_type": payload.as_ref().and_then(|req| req.editor_type.as_ref()),
                        "remote_mode": url.is_some(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(OpenEditorResponse {
                url,
            })))
        }
        Err(e) => {
            tracing::error!("Failed to open editor for project {}: {:?}", project.id, e);
            Err(ApiError::EditorOpen(e))
        }
    }
}

pub async fn search_project_files(
    State(deployment): State<DeploymentImpl>,
    Extension(project): Extension<Project>,
    Query(search_query): Query<SearchQuery>,
) -> Result<ResponseJson<ApiResponse<Vec<SearchResult>>>, StatusCode> {
    if search_query.q.trim().is_empty() {
        return Ok(ResponseJson(ApiResponse::error(
            "Query parameter 'q' is required and cannot be empty",
        )));
    }

    let repositories = match deployment
        .project()
        .get_repositories(&deployment.db().pool, project.id)
        .await
    {
        Ok(repos) => repos,
        Err(e) => {
            tracing::error!("Failed to get repositories: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match deployment
        .project()
        .search_files(
            deployment.file_search_cache().as_ref(),
            &repositories,
            &search_query,
        )
        .await
    {
        Ok(results) => Ok(ResponseJson(ApiResponse::success(results))),
        Err(e) => {
            tracing::error!("Failed to search files: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_project_repositories(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<Vec<Repo>>>, ApiError> {
    let repositories = deployment
        .project()
        .get_repositories(&deployment.db().pool, project.id)
        .await?;
    Ok(ResponseJson(ApiResponse::success(repositories)))
}

pub async fn add_project_repository(
    Extension(project): Extension<Project>,
    State(deployment): State<DeploymentImpl>,
    Json(payload): Json<CreateProjectRepo>,
) -> Result<ResponseJson<ApiResponse<Repo>>, ApiError> {
    tracing::debug!(
        "Adding repository '{}' to project {} (path: {})",
        payload.display_name,
        project.id,
        payload.git_repo_path
    );

    match deployment
        .project()
        .add_repository(
            &deployment.db().pool,
            deployment.repo(),
            project.id,
            &payload,
        )
        .await
    {
        Ok(repository) => {
            deployment
                .track_if_analytics_allowed(
                    "project_repository_added",
                    serde_json::json!({
                        "project_id": project.id.to_string(),
                        "repository_id": repository.id.to_string(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(repository)))
        }
        Err(ProjectServiceError::PathNotFound(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: path does not exist",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified path does not exist",
            )))
        }
        Err(ProjectServiceError::PathNotDirectory(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: path is not a directory",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified path is not a directory",
            )))
        }
        Err(ProjectServiceError::NotGitRepository(_)) => {
            tracing::warn!(
                "Failed to add repository to project {}: not a git repository",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "The specified directory is not a git repository",
            )))
        }
        Err(ProjectServiceError::DuplicateRepositoryName) => {
            tracing::warn!(
                "Failed to add repository to project {}: duplicate repository name",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "A repository with this name already exists in the project",
            )))
        }
        Err(ProjectServiceError::DuplicateGitRepoPath) => {
            tracing::warn!(
                "Failed to add repository to project {}: duplicate repository path",
                project.id
            );
            Ok(ResponseJson(ApiResponse::error(
                "A repository with this path already exists in the project",
            )))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete_project_repository(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, repo_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<()>>, ApiError> {
    tracing::debug!(
        "Removing repository {} from project {}",
        repo_id,
        project_id
    );

    match deployment
        .project()
        .delete_repository(&deployment.db().pool, project_id, repo_id)
        .await
    {
        Ok(()) => {
            deployment
                .track_if_analytics_allowed(
                    "project_repository_removed",
                    serde_json::json!({
                        "project_id": project_id.to_string(),
                        "repository_id": repo_id.to_string(),
                    }),
                )
                .await;

            Ok(ResponseJson(ApiResponse::success(())))
        }
        Err(ProjectServiceError::RepositoryNotFound) => {
            tracing::warn!(
                "Failed to remove repository {} from project {}: not found",
                repo_id,
                project_id
            );
            Ok(ResponseJson(ApiResponse::error("Repository not found")))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn get_project_repository(
    State(deployment): State<DeploymentImpl>,
    Path((project_id, repo_id)): Path<(Uuid, Uuid)>,
) -> Result<ResponseJson<ApiResponse<ProjectRepo>>, ApiError> {
    match ProjectRepo::find_by_project_and_repo(&deployment.db().pool, project_id, repo_id).await {
        Ok(Some(project_repo)) => Ok(ResponseJson(ApiResponse::success(project_repo))),
        Ok(None) => Err(ApiError::BadRequest(
            "Repository not found in project".to_string(),
        )),
        Err(e) => Err(e.into()),
    }
}

pub async fn get_all_project_stats(
    State(deployment): State<DeploymentImpl>,
) -> Result<ResponseJson<ApiResponse<AllProjectStats>>, ApiError> {
    let projects = Project::find_all(&deployment.db().pool).await?;
    let git = deployment.git().clone();
    let pool = deployment.db().pool.clone();

    let stats = tokio::task::spawn_blocking(move || {
        let mut result: Vec<ProjectStats> = Vec::new();

        for project in &projects {
            let repos = tokio::runtime::Handle::current()
                .block_on(ProjectRepo::find_repos_for_project(&pool, project.id));

            let repos = match repos {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Failed to get repos for project {}: {}", project.id, e);
                    result.push(ProjectStats {
                        project_id: project.id,
                        commits_this_month: 0,
                        lines_added_this_month: 0,
                        lines_removed_this_month: 0,
                        last_commit_date: None,
                    });
                    continue;
                }
            };

            let mut total_commits: u32 = 0;
            let mut total_added: u32 = 0;
            let mut total_removed: u32 = 0;
            let mut latest_date: Option<DateTime<Utc>> = None;

            for repo in &repos {
                let path = std::path::PathBuf::from(&repo.path);
                match git.collect_monthly_stats(&path) {
                    Ok((commits, added, removed, last_date)) => {
                        total_commits += commits;
                        total_added += added;
                        total_removed += removed;
                        if let Some(d) = last_date {
                            latest_date = Some(match latest_date {
                                Some(existing) if existing > d => existing,
                                _ => d,
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to collect stats for repo {} ({}): {}",
                            repo.display_name,
                            repo.path.display(),
                            e
                        );
                    }
                }
            }

            result.push(ProjectStats {
                project_id: project.id,
                commits_this_month: total_commits,
                lines_added_this_month: total_added,
                lines_removed_this_month: total_removed,
                last_commit_date: latest_date,
            });
        }

        result
    })
    .await
    .map_err(|e| ApiError::BadRequest(format!("Stats computation failed: {}", e)))?;

    Ok(ResponseJson(ApiResponse::success(AllProjectStats {
        stats,
    })))
}

pub fn router(deployment: &DeploymentImpl) -> Router<DeploymentImpl> {
    let project_id_router = Router::new()
        .route(
            "/",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/search", get(search_project_files))
        .route("/open-editor", post(open_project_in_editor))
        .route(
            "/repositories",
            get(get_project_repositories).post(add_project_repository),
        )
        .layer(from_fn_with_state(
            deployment.clone(),
            load_project_middleware,
        ));

    let projects_router = Router::new()
        .route("/", get(get_projects).post(create_project))
        .route("/stats", get(get_all_project_stats))
        .route(
            "/{project_id}/repositories/{repo_id}",
            get(get_project_repository).delete(delete_project_repository),
        )
        .route("/stream/ws", get(stream_projects_ws))
        .nest("/{id}", project_id_router);

    Router::new().nest("/projects", projects_router)
}
