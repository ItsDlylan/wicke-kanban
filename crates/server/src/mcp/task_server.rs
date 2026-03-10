use std::str::FromStr;

use db::models::{
    project::Project,
    tag::Tag,
    task::{CreateTask, Task, TaskStatus, TaskType, TaskWithAttemptStatus},
    workspace::WorkspaceContext,
};
use regex::Regex;
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{Parameters, ToolRouter},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json;
use uuid::Uuid;

use crate::routes::containers::ContainerQuery;

// ── MCP request/response types ──────────────────────────────────────────────

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskSummary {
    #[schemars(description = "The unique identifier of the task")]
    pub id: String,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Current status of the task")]
    pub status: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(description = "When the task was created")]
    pub created_at: String,
    #[schemars(description = "When the task was last updated")]
    pub updated_at: String,
}

/// Slim task representation for list endpoints — no description to save tokens.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TaskListItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateTaskRequest {
    #[schemars(
        description = "The ID of the project to create the task in. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "The title of the task")]
    pub title: String,
    #[schemars(description = "Optional description of the task")]
    pub description: Option<String>,
    #[schemars(
        description = "Optional status for the task. Valid values: backlog, idea, planning, plangenerating, specreview, ready, ralph, inprogress, qa, done, cancelled. Defaults to backlog."
    )]
    pub status: Option<String>,
    #[schemars(
        description = "Optional parent task ID. Set this to make the new task a child/story of the specified parent task."
    )]
    pub parent_task_id: Option<Uuid>,
    #[schemars(
        description = "Optional task type. Valid values: 'task' (default), 'epic'. Use 'epic' for planning/feature tickets."
    )]
    pub task_type: Option<String>,
    #[schemars(
        description = "If true, marks this as a human-assigned task (not for AI agents). Set task_type='epic' and is_human=true to create a human epic."
    )]
    pub is_human: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpCreateTaskResponse {
    pub task_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    #[schemars(description = "The unique identifier of the project")]
    pub id: String,
    #[schemars(description = "The name of the project")]
    pub name: String,
    #[schemars(description = "When the project was created")]
    pub created_at: String,
    #[schemars(description = "When the project was last updated")]
    pub updated_at: String,
}

impl ProjectSummary {
    fn from_project(project: Project) -> Self {
        Self {
            id: project.id.to_string(),
            name: project.name,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpListProjectsResponse {
    pub projects: Vec<ProjectSummary>,
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpListTasksRequest {
    #[schemars(
        description = "The ID of the project to list tasks from. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Maximum number of tasks to return (default: 50)")]
    pub limit: Option<i32>,
    #[schemars(
        description = "Filter to only return child tasks of this parent task ID. Useful for listing stories/subtasks."
    )]
    pub parent_task_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpListTasksResponse {
    pub tasks: Vec<TaskListItem>,
    pub count: usize,
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpGetTaskRequest {
    #[schemars(description = "The ID of the task to retrieve")]
    pub task_id: Uuid,
    #[schemars(
        description = "The ID of the project the task belongs to. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpGetTaskResponse {
    pub task: TaskSummary,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpUpdateTaskRequest {
    #[schemars(description = "The ID of the task to update")]
    pub task_id: Uuid,
    #[schemars(
        description = "The ID of the project the task belongs to. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "New title for the task")]
    pub title: Option<String>,
    #[schemars(description = "New description for the task")]
    pub description: Option<String>,
    #[schemars(
        description = "New status for the task. Valid values: backlog, idea, planning, plangenerating, specreview, ready, ralph, inprogress, qa, done, cancelled."
    )]
    pub status: Option<String>,
    #[schemars(
        description = "Optional parent task ID. Set this to make the task a child/story of the specified parent task."
    )]
    pub parent_task_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpUpdateTaskResponse {
    pub task: TaskSummary,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpDeleteTaskRequest {
    #[schemars(description = "The ID of the task to delete")]
    pub task_id: Uuid,
    #[schemars(
        description = "The ID of the project the task belongs to. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpDeleteTaskResponse {
    pub deleted_task_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpSearchTasksRequest {
    #[schemars(
        description = "The ID of the project to search tasks in. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Search query to match against task titles")]
    pub query: String,
    #[schemars(description = "Max results (default 10)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpSearchTasksResponse {
    pub tasks: Vec<TaskListItem>,
    pub count: usize,
    pub project_id: String,
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateTaskWithContextRequest {
    #[schemars(
        description = "The ID of the project to create the task in. Optional if running inside a workspace linked to a project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Task title")]
    pub title: String,
    #[schemars(description = "High-level summary of the task")]
    pub summary: Option<String>,
    #[schemars(description = "Repository name where the issue was found")]
    pub repo_name: Option<String>,
    #[schemars(description = "File path relevant to the task")]
    pub file_path: Option<String>,
    #[schemars(description = "Line number in the file")]
    pub line_number: Option<u32>,
    #[schemars(description = "Git ref (commit hash or branch)")]
    pub git_ref: Option<String>,
    #[schemars(description = "Error output or stack trace")]
    pub error_output: Option<String>,
    #[schemars(description = "Command that was run when the issue occurred")]
    pub command: Option<String>,
    #[schemars(
        description = "Optional parent task ID. Set this to make the new task a child/story of the specified parent task."
    )]
    pub parent_task_id: Option<Uuid>,
    #[schemars(
        description = "Optional task type. Valid values: 'task' (default), 'epic'. Use 'epic' for planning/feature tickets."
    )]
    pub task_type: Option<String>,
    #[schemars(
        description = "If true, marks this as a human-assigned task (not for AI agents). Set task_type='epic' and is_human=true to create a human epic."
    )]
    pub is_human: Option<bool>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpCreateTaskWithContextResponse {
    pub task_id: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpPingResponse {
    pub status: String,
    pub version: String,
}

// ── Server struct ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TaskServer {
    client: reqwest::Client,
    base_url: String,
    tool_router: ToolRouter<TaskServer>,
    context: Option<McpContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpRepoContext {
    #[schemars(description = "The unique identifier of the repository")]
    pub repo_id: Uuid,
    #[schemars(description = "The name of the repository")]
    pub repo_name: String,
    #[schemars(description = "The target branch for this repository in this workspace")]
    pub target_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct McpContext {
    #[schemars(description = "The project ID (if workspace is linked to a project)")]
    pub project_id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub workspace_branch: String,
    #[schemars(
        description = "Repository info and target branches for each repo in this workspace"
    )]
    pub workspace_repos: Vec<McpRepoContext>,
}

impl TaskServer {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            tool_router: Self::tool_router(),
            context: None,
        }
    }

    pub async fn init(mut self) -> Self {
        let context = self.fetch_context_at_startup().await;

        if context.is_none() {
            self.tool_router.map.remove("get_context");
            tracing::debug!("VK context not available, get_context tool will not be registered");
        } else {
            tracing::info!("VK context loaded, get_context tool available");
        }

        self.context = context;
        self
    }

    async fn fetch_context_at_startup(&self) -> Option<McpContext> {
        let current_dir = std::env::current_dir().ok()?;
        let canonical_path = current_dir.canonicalize().unwrap_or(current_dir);
        let normalized_path = utils::path::normalize_macos_private_alias(&canonical_path);

        let url = self.url("/api/containers/attempt-context");
        let query = ContainerQuery {
            container_ref: normalized_path.to_string_lossy().to_string(),
        };

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.client.get(&url).query(&query).send(),
        )
        .await
        .ok()?
        .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<WorkspaceContext> = response.json().await.ok()?;

        if !api_response.success {
            return None;
        }

        let ctx = api_response.data?;

        let workspace_repos: Vec<McpRepoContext> = ctx
            .workspace_repos
            .into_iter()
            .map(|rwb| McpRepoContext {
                repo_id: rwb.repo.id,
                repo_name: rwb.repo.name,
                target_branch: rwb.target_branch,
            })
            .collect();

        let workspace_id = ctx.workspace.id;
        let workspace_branch = ctx.workspace.branch.clone();
        let project_id = Some(ctx.project.id);

        Some(McpContext {
            project_id,
            workspace_id,
            workspace_branch,
            workspace_repos,
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ApiResponseEnvelope<T> {
    success: bool,
    data: Option<T>,
    message: Option<String>,
}

impl TaskServer {
    fn success<T: Serialize>(data: &T) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(data)
                .unwrap_or_else(|_| "Failed to serialize response".to_string()),
        )]))
    }

    fn err_value(v: serde_json::Value) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::error(vec![Content::text(
            serde_json::to_string_pretty(&v)
                .unwrap_or_else(|_| "Failed to serialize error".to_string()),
        )]))
    }

    fn err<S: Into<String>>(msg: S, details: Option<S>) -> Result<CallToolResult, ErrorData> {
        let mut v = serde_json::json!({"success": false, "error": msg.into()});
        if let Some(d) = details {
            v["details"] = serde_json::json!(d.into());
        };
        Self::err_value(v)
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        rb: reqwest::RequestBuilder,
    ) -> Result<T, CallToolResult> {
        let resp = rb
            .send()
            .await
            .map_err(|e| Self::err("Failed to connect to VK API", Some(&e.to_string())).unwrap())?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(
                Self::err(format!("VK API returned error status: {}", status), None).unwrap(),
            );
        }

        let body = resp.text().await.map_err(|e| {
            Self::err("Failed to read VK API response body", Some(&e.to_string())).unwrap()
        })?;

        let api_response: ApiResponseEnvelope<T> = serde_json::from_str(&body).map_err(|e| {
            let preview = if body.len() > 500 {
                format!("{}...", &body[..500])
            } else {
                body.clone()
            };
            Self::err(
                format!("Failed to parse VK API response: {}", e),
                Some(preview),
            )
            .unwrap()
        })?;

        if !api_response.success {
            let msg = api_response.message.as_deref().unwrap_or("Unknown error");
            return Err(Self::err("VK API returned error", Some(msg)).unwrap());
        }

        api_response
            .data
            .ok_or_else(|| Self::err("VK API response missing data field", None).unwrap())
    }

    async fn send_empty_json(&self, rb: reqwest::RequestBuilder) -> Result<(), CallToolResult> {
        let resp = rb
            .send()
            .await
            .map_err(|e| Self::err("Failed to connect to VK API", Some(&e.to_string())).unwrap())?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(
                Self::err(format!("VK API returned error status: {}", status), None).unwrap(),
            );
        }

        #[derive(Deserialize)]
        struct EmptyApiResponse {
            success: bool,
            message: Option<String>,
        }

        let api_response = resp.json::<EmptyApiResponse>().await.map_err(|e| {
            Self::err("Failed to parse VK API response", Some(&e.to_string())).unwrap()
        })?;

        if !api_response.success {
            let msg = api_response.message.as_deref().unwrap_or("Unknown error");
            return Err(Self::err("VK API returned error", Some(msg)).unwrap());
        }

        Ok(())
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    /// Expands @tagname references in text by replacing them with tag content.
    async fn expand_tags(&self, text: &str) -> String {
        let tag_pattern = match Regex::new(r"@([^\s@]+)") {
            Ok(re) => re,
            Err(_) => return text.to_string(),
        };

        let tag_names: Vec<String> = tag_pattern
            .captures_iter(text)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if tag_names.is_empty() {
            return text.to_string();
        }

        let url = self.url("/api/tags");
        let tags: Vec<Tag> = match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<ApiResponseEnvelope<Vec<Tag>>>().await {
                    Ok(envelope) if envelope.success => envelope.data.unwrap_or_default(),
                    _ => return text.to_string(),
                }
            }
            _ => return text.to_string(),
        };

        let tag_map: std::collections::HashMap<&str, &str> = tags
            .iter()
            .map(|t| (t.tag_name.as_str(), t.content.as_str()))
            .collect();

        let result = tag_pattern.replace_all(text, |caps: &regex::Captures| {
            let tag_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            match tag_map.get(tag_name) {
                Some(content) => (*content).to_string(),
                None => caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string(),
            }
        });

        result.into_owned()
    }

    /// Resolves a project_id from an explicit parameter or falls back to context.
    fn resolve_project_id(&self, explicit: Option<Uuid>) -> Result<Uuid, CallToolResult> {
        if let Some(id) = explicit {
            return Ok(id);
        }
        if let Some(ctx) = &self.context
            && let Some(id) = ctx.project_id
        {
            return Ok(id);
        }
        Err(Self::err(
            "project_id is required (not available from workspace context)",
            None::<&str>,
        )
        .unwrap())
    }

    /// Converts a Task to TaskSummary.
    fn task_to_summary(task: &Task) -> TaskSummary {
        TaskSummary {
            id: task.id.to_string(),
            title: task.title.clone(),
            status: task.status.to_string(),
            description: task.description.clone(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
        }
    }

    fn task_to_list_item(task: &Task) -> TaskListItem {
        TaskListItem {
            id: task.id.to_string(),
            title: task.title.clone(),
            status: task.status.to_string(),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
        }
    }

    /// Parses a status string to TaskStatus, returning a helpful error listing valid values.
    fn resolve_task_status(s: &str) -> Result<TaskStatus, CallToolResult> {
        TaskStatus::from_str(s).map_err(|_| {
            Self::err(
                format!(
                    "Unknown status '{}'. Valid statuses: backlog, idea, planning, plangenerating, specreview, ready, ralph, inprogress, qa, done, cancelled",
                    s
                ),
                None::<String>,
            )
            .unwrap()
        })
    }

    fn resolve_task_type(s: &str) -> Result<TaskType, CallToolResult> {
        match s.to_lowercase().as_str() {
            "task" => Ok(TaskType::Task),
            "epic" => Ok(TaskType::Epic),
            _ => Err(Self::err(
                format!("Invalid task_type '{}'. Valid values: task, epic", s),
                None::<String>,
            )
            .unwrap()),
        }
    }
}

// ── MCP Tools ───────────────────────────────────────────────────────────────

#[tool_router]
impl TaskServer {
    #[tool(
        description = "Return project and workspace metadata for the current workspace session context."
    )]
    async fn get_context(&self) -> Result<CallToolResult, ErrorData> {
        let context = self.context.as_ref().expect("VK context should exist");
        TaskServer::success(context)
    }

    #[tool(
        description = "Create a new task in a project. `project_id` is optional if running inside a workspace linked to a project."
    )]
    async fn create_task(
        &self,
        Parameters(McpCreateTaskRequest {
            project_id,
            title,
            description,
            status,
            parent_task_id,
            task_type,
            is_human,
        }): Parameters<McpCreateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let resolved_task_type = if let Some(ref s) = task_type {
            match Self::resolve_task_type(s) {
                Ok(tt) => Some(tt),
                Err(e) => return Ok(e),
            }
        } else {
            None
        };

        let is_epic = resolved_task_type == Some(TaskType::Epic);

        let task_status = if let Some(ref s) = status {
            match Self::resolve_task_status(s) {
                Ok(ts) => Some(ts),
                Err(e) => return Ok(e),
            }
        } else if is_epic {
            Some(TaskStatus::Idea)
        } else {
            Some(TaskStatus::Backlog)
        };

        let payload = CreateTask {
            project_id,
            title,
            description: expanded_description,
            status: task_status,
            task_type: resolved_task_type,
            parent_workspace_id: None,
            parent_task_id,
            image_ids: None,
            sort_order: None,
            plan_status: None,
            is_human,
        };

        let url = self.url("/api/tasks");
        let task: Task = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&McpCreateTaskResponse {
            task_id: task.id.to_string(),
        })
    }

    #[tool(description = "List all the available projects")]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/projects");
        let projects: Vec<Project> = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let project_summaries: Vec<ProjectSummary> = projects
            .into_iter()
            .map(ProjectSummary::from_project)
            .collect();

        TaskServer::success(&McpListProjectsResponse {
            count: project_summaries.len(),
            projects: project_summaries,
        })
    }

    #[tool(
        description = "List all the tasks in a project. `project_id` is optional if running inside a workspace linked to a project."
    )]
    async fn list_tasks(
        &self,
        Parameters(McpListTasksRequest {
            project_id,
            limit,
            parent_task_id,
        }): Parameters<McpListTasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let url = self.url(&format!("/api/tasks?project_id={}", project_id));
        let tasks: Vec<TaskWithAttemptStatus> = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let task_limit = limit.unwrap_or(50).max(0) as usize;
        let items: Vec<TaskListItem> = tasks
            .iter()
            .filter(|t| match parent_task_id {
                Some(pid) => t.task.parent_task_id == Some(pid),
                None => true,
            })
            .take(task_limit)
            .map(|t| Self::task_to_list_item(&t.task))
            .collect();

        TaskServer::success(&McpListTasksResponse {
            count: items.len(),
            tasks: items,
            project_id: project_id.to_string(),
        })
    }

    #[tool(
        description = "Update an existing task's title, description, or status. `task_id` is required. `title`, `description`, and `status` are optional. Valid statuses: backlog, idea, planning, plangenerating, specreview, ready, ralph, inprogress, qa, done, cancelled."
    )]
    async fn update_task(
        &self,
        Parameters(McpUpdateTaskRequest {
            task_id,
            project_id: _,
            title,
            description,
            status,
            parent_task_id,
        }): Parameters<McpUpdateTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Resolve status name to TaskStatus if provided
        let task_status = if let Some(ref status_name) = status {
            match Self::resolve_task_status(status_name) {
                Ok(ts) => Some(ts),
                Err(e) => return Ok(e),
            }
        } else {
            None
        };

        // Expand @tagname references in description
        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let payload = serde_json::json!({
            "title": title,
            "description": expanded_description,
            "status": task_status,
            "parent_task_id": parent_task_id,
        });

        let url = self.url(&format!("/api/tasks/{}", task_id));
        let task: Task = match self.send_json(self.client.put(&url).json(&payload)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&McpUpdateTaskResponse {
            task: Self::task_to_summary(&task),
        })
    }

    #[tool(description = "Delete a task. `task_id` is required.")]
    async fn delete_task(
        &self,
        Parameters(McpDeleteTaskRequest {
            task_id,
            project_id: _,
        }): Parameters<McpDeleteTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        if let Err(e) = self.send_empty_json(self.client.delete(&url)).await {
            return Ok(e);
        }

        TaskServer::success(&McpDeleteTaskResponse {
            deleted_task_id: task_id.to_string(),
        })
    }

    #[tool(
        description = "Get detailed information about a specific task. You can use `list_tasks` to find task IDs. `task_id` is required."
    )]
    async fn get_task(
        &self,
        Parameters(McpGetTaskRequest {
            task_id,
            project_id: _,
        }): Parameters<McpGetTaskRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/tasks/{}", task_id));
        let task: Task = match self.send_json(self.client.get(&url)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&McpGetTaskResponse {
            task: Self::task_to_summary(&task),
        })
    }

    #[tool(
        description = "Search for existing tasks by title. Use this before creating a new task to check for duplicates. Returns tasks whose titles contain the search query (case-insensitive)."
    )]
    async fn search_tasks(
        &self,
        Parameters(McpSearchTasksRequest {
            project_id,
            query,
            limit,
        }): Parameters<McpSearchTasksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let url = self.url(&format!("/api/tasks?project_id={}", project_id));
        let tasks: Vec<TaskWithAttemptStatus> = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let query_lower = query.to_lowercase();
        let result_limit = limit.unwrap_or(10).max(0) as usize;

        let matched: Vec<TaskListItem> = tasks
            .iter()
            .filter(|t| t.task.title.to_lowercase().contains(&query_lower))
            .take(result_limit)
            .map(|t| Self::task_to_list_item(&t.task))
            .collect();

        TaskServer::success(&McpSearchTasksResponse {
            count: matched.len(),
            tasks: matched,
            project_id: project_id.to_string(),
            query,
        })
    }

    #[tool(
        description = "Create a new task with structured context. Provide file paths, error output, git refs, etc. as separate fields — they'll be formatted into a readable description."
    )]
    async fn create_task_with_context(
        &self,
        Parameters(McpCreateTaskWithContextRequest {
            project_id,
            title,
            summary,
            repo_name,
            file_path,
            line_number,
            git_ref,
            error_output,
            command,
            parent_task_id,
            task_type,
            is_human,
        }): Parameters<McpCreateTaskWithContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        // Build markdown description
        let mut desc = String::new();
        if let Some(ref s) = summary {
            desc.push_str(s);
            desc.push_str("\n\n");
        }

        let mut context_lines = Vec::new();
        if let Some(ref r) = repo_name {
            context_lines.push(format!("- **Repository**: {}", r));
        }
        if let Some(ref f) = file_path {
            if let Some(ln) = line_number {
                context_lines.push(format!("- **File**: {}:{}", f, ln));
            } else {
                context_lines.push(format!("- **File**: {}", f));
            }
        }
        if let Some(ref g) = git_ref {
            context_lines.push(format!("- **Git ref**: {}", g));
        }
        if let Some(ref c) = command {
            context_lines.push(format!("- **Command**: `{}`", c));
        }
        if !context_lines.is_empty() {
            desc.push_str("## Context\n");
            desc.push_str(&context_lines.join("\n"));
            desc.push_str("\n\n");
        }

        if let Some(ref e) = error_output {
            desc.push_str("## Error Output\n```\n");
            desc.push_str(e);
            desc.push_str("\n```\n");
        }

        let expanded_description = if desc.is_empty() {
            None
        } else {
            Some(self.expand_tags(&desc).await)
        };

        let resolved_task_type = if let Some(ref s) = task_type {
            match Self::resolve_task_type(s) {
                Ok(tt) => Some(tt),
                Err(e) => return Ok(e),
            }
        } else {
            None
        };

        let is_epic = resolved_task_type == Some(TaskType::Epic);

        let default_status = if is_epic {
            TaskStatus::Idea
        } else {
            TaskStatus::Backlog
        };

        let payload = CreateTask {
            project_id,
            title,
            description: expanded_description,
            status: Some(default_status),
            task_type: resolved_task_type,
            parent_workspace_id: None,
            parent_task_id,
            image_ids: None,
            sort_order: None,
            plan_status: None,
            is_human,
        };

        let url = self.url("/api/tasks");
        let task: Task = match self.send_json(self.client.post(&url).json(&payload)).await {
            Ok(t) => t,
            Err(e) => return Ok(e),
        };

        TaskServer::success(&McpCreateTaskWithContextResponse {
            task_id: task.id.to_string(),
        })
    }

    #[tool(
        description = "Check if the Wickeban server is running and reachable. Call this before creating tasks to ensure the server is available."
    )]
    async fn ping(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/health");
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => TaskServer::success(&McpPingResponse {
                status: "ok".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            Ok(resp) => Self::err(
                format!(
                    "Wickeban server returned status {} at {}",
                    resp.status(),
                    self.base_url
                ),
                None::<String>,
            ),
            Err(e) => Self::err(
                format!("Wickeban server is not reachable at {}", self.base_url),
                Some(e.to_string()),
            ),
        }
    }
}

#[tool_handler]
impl ServerHandler for TaskServer {
    fn get_info(&self) -> ServerInfo {
        let mut instruction = "A task and project management server. If you need to create or update tasks then use these tools. Most of them require that you pass the `project_id` of the project that you are currently working on. You can get project ids by using `list_projects`. Call `list_tasks` to fetch the task IDs. TOOLS: 'ping', 'list_projects', 'list_tasks', 'search_tasks', 'create_task', 'create_task_with_context', 'get_task', 'update_task', 'delete_task'. Before creating a task, use 'search_tasks' to check for duplicates. Use 'create_task_with_context' to provide structured metadata (file paths, error output, git refs). Use 'ping' to verify the server is running. Valid task statuses: backlog, idea, planning, plangenerating, specreview, ready, ralph, inprogress, qa, done, cancelled.".to_string();
        if self.context.is_some() {
            let context_instruction = "Use 'get_context' to fetch project/workspace metadata for the active Wickeban workspace session when available.";
            instruction = format!("{} {}", context_instruction, instruction);
        }

        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "wickeban".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some(instruction),
        }
    }
}
