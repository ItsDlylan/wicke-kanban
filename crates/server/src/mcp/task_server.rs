use api_types::{
    Issue, IssueComment, ListIssuesResponse, ListOrganizationsResponse,
    ListProjectStatusesResponse, MutationResponse, ProjectStatus,
};
use db::models::{tag::Tag, workspace::WorkspaceContext};
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateIssueRequest {
    #[schemars(
        description = "The ID of the project to create the issue in. Optional if running inside a workspace linked to a remote project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "The title of the issue")]
    pub title: String,
    #[schemars(description = "Optional description of the issue")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpCreateIssueResponse {
    pub issue_id: String,
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
    fn from_remote_project(project: api_types::Project) -> Self {
        Self {
            id: project.id.to_string(),
            name: project.name,
            created_at: project.created_at.to_rfc3339(),
            updated_at: project.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpListProjectsRequest {
    #[schemars(description = "The ID of the organization to list projects from")]
    pub organization_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpListProjectsResponse {
    pub projects: Vec<ProjectSummary>,
    pub count: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct OrganizationSummary {
    #[schemars(description = "The unique identifier of the organization")]
    pub id: String,
    #[schemars(description = "The name of the organization")]
    pub name: String,
    #[schemars(description = "The slug of the organization")]
    pub slug: String,
    #[schemars(description = "Whether this is a personal organization")]
    pub is_personal: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpListOrganizationsResponse {
    pub organizations: Vec<OrganizationSummary>,
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpListIssuesRequest {
    #[schemars(
        description = "The ID of the project to list issues from. Optional if running inside a workspace linked to a remote project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Maximum number of issues to return (default: 50)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IssueSummary {
    #[schemars(description = "The unique identifier of the issue")]
    pub id: String,
    #[schemars(description = "The title of the issue")]
    pub title: String,
    #[schemars(description = "Current status of the issue")]
    pub status: String,
    #[schemars(description = "When the issue was created")]
    pub created_at: String,
    #[schemars(description = "When the issue was last updated")]
    pub updated_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct IssueDetails {
    #[schemars(description = "The unique identifier of the issue")]
    pub id: String,
    #[schemars(description = "The title of the issue")]
    pub title: String,
    #[schemars(description = "Optional description of the issue")]
    pub description: Option<String>,
    #[schemars(description = "Current status of the issue")]
    pub status: String,
    #[schemars(description = "The status ID (UUID)")]
    pub status_id: String,
    #[schemars(description = "When the issue was created")]
    pub created_at: String,
    #[schemars(description = "When the issue was last updated")]
    pub updated_at: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpListIssuesResponse {
    pub issues: Vec<IssueSummary>,
    pub count: usize,
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpUpdateIssueRequest {
    #[schemars(description = "The ID of the issue to update")]
    pub issue_id: Uuid,
    #[schemars(description = "New title for the issue")]
    pub title: Option<String>,
    #[schemars(description = "New description for the issue")]
    pub description: Option<String>,
    #[schemars(description = "New status name for the issue (must match a project status name)")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpUpdateIssueResponse {
    pub issue: IssueDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpDeleteIssueRequest {
    #[schemars(description = "The ID of the issue to delete")]
    pub issue_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpDeleteIssueResponse {
    pub deleted_issue_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpGetIssueRequest {
    #[schemars(description = "The ID of the issue to retrieve")]
    pub issue_id: Uuid,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpGetIssueResponse {
    pub issue: IssueDetails,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpSearchIssuesRequest {
    #[schemars(
        description = "The ID of the project to search issues in. Optional if running inside a workspace linked to a remote project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Search query to match against issue titles")]
    pub query: String,
    #[schemars(description = "Max results (default 10)")]
    pub limit: Option<i32>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpSearchIssuesResponse {
    pub issues: Vec<IssueSummary>,
    pub count: usize,
    pub project_id: String,
    pub query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpAddCommentRequest {
    #[schemars(description = "The ID of the issue to comment on")]
    pub issue_id: Uuid,
    #[schemars(description = "The comment message (supports markdown)")]
    pub message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpAddCommentResponse {
    pub comment_id: String,
    pub issue_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct McpCreateIssueWithContextRequest {
    #[schemars(
        description = "The ID of the project to create the issue in. Optional if running inside a workspace linked to a remote project."
    )]
    pub project_id: Option<Uuid>,
    #[schemars(description = "Issue title")]
    pub title: String,
    #[schemars(description = "High-level summary of the issue")]
    pub summary: Option<String>,
    #[schemars(description = "Repository name where the issue was found")]
    pub repo_name: Option<String>,
    #[schemars(description = "File path relevant to the issue")]
    pub file_path: Option<String>,
    #[schemars(description = "Line number in the file")]
    pub line_number: Option<u32>,
    #[schemars(description = "Git ref (commit hash or branch)")]
    pub git_ref: Option<String>,
    #[schemars(description = "Error output or stack trace")]
    pub error_output: Option<String>,
    #[schemars(description = "Command that was run when the issue occurred")]
    pub command: Option<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct McpCreateIssueWithContextResponse {
    pub issue_id: String,
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
    #[schemars(description = "The organization ID (if workspace is linked to remote)")]
    pub organization_id: Option<Uuid>,
    #[schemars(description = "The remote project ID (if workspace is linked to remote)")]
    pub project_id: Option<Uuid>,
    #[schemars(description = "The remote issue ID (if workspace is linked to a remote issue)")]
    pub issue_id: Option<Uuid>,
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

        // Look up remote workspace to get remote project_id, issue_id, and organization_id
        let (project_id, issue_id, organization_id) = self
            .fetch_remote_workspace_context(workspace_id)
            .await
            .unwrap_or((None, None, None));

        Some(McpContext {
            organization_id,
            project_id,
            issue_id,
            workspace_id,
            workspace_branch,
            workspace_repos,
        })
    }

    async fn fetch_remote_workspace_context(
        &self,
        local_workspace_id: Uuid,
    ) -> Option<(Option<Uuid>, Option<Uuid>, Option<Uuid>)> {
        let url = self.url(&format!(
            "/api/remote/workspaces/by-local-id/{}",
            local_workspace_id
        ));

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(2000),
            self.client.get(&url).send(),
        )
        .await
        .ok()?
        .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<api_types::Workspace> = response.json().await.ok()?;

        if !api_response.success {
            return None;
        }

        let remote_ws = api_response.data?;
        let project_id = remote_ws.project_id;

        // Fetch the project to get organization_id
        let org_id = self.fetch_remote_organization_id(project_id).await;

        Some((Some(project_id), remote_ws.issue_id, org_id))
    }

    async fn fetch_remote_organization_id(&self, project_id: Uuid) -> Option<Uuid> {
        let url = self.url(&format!("/api/remote/projects/{}", project_id));

        let response = tokio::time::timeout(
            std::time::Duration::from_millis(2000),
            self.client.get(&url).send(),
        )
        .await
        .ok()?
        .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let api_response: ApiResponseEnvelope<api_types::Project> = response.json().await.ok()?;
        let project = api_response.data?;
        Some(project.organization_id)
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

        let api_response = resp.json::<ApiResponseEnvelope<T>>().await.map_err(|e| {
            Self::err("Failed to parse VK API response", Some(&e.to_string())).unwrap()
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

    /// Fetches project statuses for a project, returning a map of status name → status.
    async fn fetch_project_statuses(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectStatus>, CallToolResult> {
        let url = self.url(&format!(
            "/api/remote/project-statuses?project_id={}",
            project_id
        ));
        let response: ListProjectStatusesResponse = self.send_json(self.client.get(&url)).await?;
        Ok(response.project_statuses)
    }

    /// Resolves a status name to a status_id UUID using project statuses.
    async fn resolve_status_id(
        &self,
        project_id: Uuid,
        status_name: &str,
    ) -> Result<Uuid, CallToolResult> {
        let statuses = self.fetch_project_statuses(project_id).await?;
        statuses
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(status_name))
            .map(|s| s.id)
            .ok_or_else(|| {
                let available: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
                Self::err(
                    format!(
                        "Unknown status '{}'. Available statuses: {:?}",
                        status_name, available
                    ),
                    None::<String>,
                )
                .unwrap()
            })
    }

    /// Gets the default status_id for a project (first non-hidden status by sort_order).
    async fn default_status_id(&self, project_id: Uuid) -> Result<Uuid, CallToolResult> {
        let statuses = self.fetch_project_statuses(project_id).await?;
        statuses
            .iter()
            .filter(|s| !s.hidden)
            .min_by_key(|s| s.sort_order)
            .map(|s| s.id)
            .ok_or_else(|| {
                Self::err("No visible statuses found for project", None::<&str>).unwrap()
            })
    }

    /// Resolves a status_id to its display name. Falls back to UUID string if lookup fails.
    async fn resolve_status_name(&self, project_id: Uuid, status_id: Uuid) -> String {
        match self.fetch_project_statuses(project_id).await {
            Ok(statuses) => statuses
                .iter()
                .find(|s| s.id == status_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| status_id.to_string()),
            Err(_) => status_id.to_string(),
        }
    }

    /// Converts an Issue to IssueSummary using a pre-fetched status map when available.
    fn issue_to_summary(
        &self,
        issue: &Issue,
        status_names_by_id: Option<&std::collections::HashMap<Uuid, String>>,
    ) -> IssueSummary {
        let status = status_names_by_id
            .and_then(|status_map| status_map.get(&issue.status_id).cloned())
            .unwrap_or_else(|| issue.status_id.to_string());
        IssueSummary {
            id: issue.id.to_string(),
            title: issue.title.clone(),
            status,
            created_at: issue.created_at.to_rfc3339(),
            updated_at: issue.updated_at.to_rfc3339(),
        }
    }

    /// Converts an Issue to IssueDetails, resolving status_id to name.
    async fn issue_to_details(&self, issue: &Issue) -> IssueDetails {
        let status = self
            .resolve_status_name(issue.project_id, issue.status_id)
            .await;
        IssueDetails {
            id: issue.id.to_string(),
            title: issue.title.clone(),
            description: issue.description.clone(),
            status,
            status_id: issue.status_id.to_string(),
            created_at: issue.created_at.to_rfc3339(),
            updated_at: issue.updated_at.to_rfc3339(),
        }
    }
}

// ── MCP Tools ───────────────────────────────────────────────────────────────

#[tool_router]
impl TaskServer {
    #[tool(
        description = "Return project, issue, and workspace metadata for the current workspace session context."
    )]
    async fn get_context(&self) -> Result<CallToolResult, ErrorData> {
        let context = self.context.as_ref().expect("VK context should exist");
        TaskServer::success(context)
    }

    #[tool(
        description = "Create a new issue in a project. `project_id` is optional if running inside a workspace linked to a remote project."
    )]
    async fn create_issue(
        &self,
        Parameters(McpCreateIssueRequest {
            project_id,
            title,
            description,
        }): Parameters<McpCreateIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let expanded_description = match description {
            Some(desc) => Some(self.expand_tags(&desc).await),
            None => None,
        };

        let status_id = match self.default_status_id(project_id).await {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let payload = api_types::CreateIssueRequest {
            id: None,
            project_id,
            status_id,
            title,
            description: expanded_description,
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata: serde_json::json!({}),
        };

        let url = self.url("/api/remote/issues");
        let response: MutationResponse<Issue> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        TaskServer::success(&McpCreateIssueResponse {
            issue_id: response.data.id.to_string(),
        })
    }

    #[tool(description = "List all the available projects")]
    async fn list_projects(
        &self,
        Parameters(McpListProjectsRequest { organization_id }): Parameters<McpListProjectsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!(
            "/api/remote/projects?organization_id={}",
            organization_id
        ));
        let response: api_types::ListProjectsResponse =
            match self.send_json(self.client.get(&url)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        let project_summaries: Vec<ProjectSummary> = response
            .projects
            .into_iter()
            .map(ProjectSummary::from_remote_project)
            .collect();

        TaskServer::success(&McpListProjectsResponse {
            count: project_summaries.len(),
            projects: project_summaries,
        })
    }

    #[tool(description = "List all the available organizations")]
    async fn list_organizations(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/api/organizations");
        let response: ListOrganizationsResponse = match self.send_json(self.client.get(&url)).await
        {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let org_summaries: Vec<OrganizationSummary> = response
            .organizations
            .into_iter()
            .map(|o| OrganizationSummary {
                id: o.id.to_string(),
                name: o.name,
                slug: o.slug,
                is_personal: o.is_personal,
            })
            .collect();

        TaskServer::success(&McpListOrganizationsResponse {
            count: org_summaries.len(),
            organizations: org_summaries,
        })
    }

    #[tool(
        description = "List all the issues in a project. `project_id` is optional if running inside a workspace linked to a remote project."
    )]
    async fn list_issues(
        &self,
        Parameters(McpListIssuesRequest { project_id, limit }): Parameters<McpListIssuesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let url = self.url(&format!("/api/remote/issues?project_id={}", project_id));
        let response: ListIssuesResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let issue_limit = limit.unwrap_or(50).max(0) as usize;
        let limited: Vec<&Issue> = response.issues.iter().take(issue_limit).collect();
        let status_names_by_id =
            self.fetch_project_statuses(project_id)
                .await
                .ok()
                .map(|statuses| {
                    statuses
                        .into_iter()
                        .map(|status| (status.id, status.name))
                        .collect::<std::collections::HashMap<_, _>>()
                });

        let mut summaries = Vec::with_capacity(limited.len());
        for issue in &limited {
            summaries.push(self.issue_to_summary(issue, status_names_by_id.as_ref()));
        }

        TaskServer::success(&McpListIssuesResponse {
            count: summaries.len(),
            issues: summaries,
            project_id: project_id.to_string(),
        })
    }

    #[tool(
        description = "Update an existing issue's title, description, or status. `issue_id` is required. `title`, `description`, and `status` are optional."
    )]
    async fn update_issue(
        &self,
        Parameters(McpUpdateIssueRequest {
            issue_id,
            title,
            description,
            status,
        }): Parameters<McpUpdateIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // First get the issue to know its project_id for status resolution
        let get_url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let existing_issue: Issue = match self.send_json(self.client.get(&get_url)).await {
            Ok(i) => i,
            Err(e) => return Ok(e),
        };

        // Resolve status name to status_id if provided
        let status_id = if let Some(ref status_name) = status {
            match self
                .resolve_status_id(existing_issue.project_id, status_name)
                .await
            {
                Ok(id) => Some(id),
                Err(e) => return Ok(e),
            }
        } else {
            None
        };

        // Expand @tagname references in description
        let expanded_description = match description {
            Some(desc) => Some(Some(self.expand_tags(&desc).await)),
            None => None,
        };

        let payload = api_types::UpdateIssueRequest {
            status_id,
            title,
            description: expanded_description,
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: None,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata: None,
        };

        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let response: MutationResponse<Issue> =
            match self.send_json(self.client.patch(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        let details = self.issue_to_details(&response.data).await;
        TaskServer::success(&McpUpdateIssueResponse { issue: details })
    }

    #[tool(description = "Delete an issue. `issue_id` is required.")]
    async fn delete_issue(
        &self,
        Parameters(McpDeleteIssueRequest { issue_id }): Parameters<McpDeleteIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        if let Err(e) = self.send_empty_json(self.client.delete(&url)).await {
            return Ok(e);
        }

        TaskServer::success(&McpDeleteIssueResponse {
            deleted_issue_id: Some(issue_id.to_string()),
        })
    }

    #[tool(
        description = "Get detailed information about a specific issue. You can use `list_issues` to find issue IDs. `issue_id` is required."
    )]
    async fn get_issue(
        &self,
        Parameters(McpGetIssueRequest { issue_id }): Parameters<McpGetIssueRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let url = self.url(&format!("/api/remote/issues/{}", issue_id));
        let issue: Issue = match self.send_json(self.client.get(&url)).await {
            Ok(i) => i,
            Err(e) => return Ok(e),
        };

        let details = self.issue_to_details(&issue).await;
        TaskServer::success(&McpGetIssueResponse { issue: details })
    }

    #[tool(
        description = "Search for existing issues by title. Use this before creating a new issue to check for duplicates. Returns issues whose titles contain the search query (case-insensitive)."
    )]
    async fn search_issues(
        &self,
        Parameters(McpSearchIssuesRequest {
            project_id,
            query,
            limit,
        }): Parameters<McpSearchIssuesRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let url = self.url(&format!("/api/remote/issues?project_id={}", project_id));
        let response: ListIssuesResponse = match self.send_json(self.client.get(&url)).await {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };

        let query_lower = query.to_lowercase();
        let result_limit = limit.unwrap_or(10).max(0) as usize;

        let status_names_by_id =
            self.fetch_project_statuses(project_id)
                .await
                .ok()
                .map(|statuses| {
                    statuses
                        .into_iter()
                        .map(|status| (status.id, status.name))
                        .collect::<std::collections::HashMap<_, _>>()
                });

        let matched: Vec<IssueSummary> = response
            .issues
            .iter()
            .filter(|issue| issue.title.to_lowercase().contains(&query_lower))
            .take(result_limit)
            .map(|issue| self.issue_to_summary(issue, status_names_by_id.as_ref()))
            .collect();

        TaskServer::success(&McpSearchIssuesResponse {
            count: matched.len(),
            issues: matched,
            project_id: project_id.to_string(),
            query,
        })
    }

    #[tool(
        description = "Add a comment to an existing issue. Use this to append additional context, stack traces, or findings to an issue instead of creating a duplicate."
    )]
    async fn add_comment(
        &self,
        Parameters(McpAddCommentRequest { issue_id, message }): Parameters<McpAddCommentRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let expanded_message = self.expand_tags(&message).await;

        let payload = serde_json::json!({
            "issue_id": issue_id,
            "message": expanded_message,
        });

        let url = self.url("/api/remote/issue_comments");
        let response: MutationResponse<IssueComment> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        TaskServer::success(&McpAddCommentResponse {
            comment_id: response.data.id.to_string(),
            issue_id: issue_id.to_string(),
        })
    }

    #[tool(
        description = "Create a new issue with structured context. Provide file paths, error output, git refs, etc. as separate fields — they'll be formatted into a readable description and stored as structured metadata for the planning agent."
    )]
    async fn create_issue_with_context(
        &self,
        Parameters(McpCreateIssueWithContextRequest {
            project_id,
            title,
            summary,
            repo_name,
            file_path,
            line_number,
            git_ref,
            error_output,
            command,
        }): Parameters<McpCreateIssueWithContextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let project_id = match self.resolve_project_id(project_id) {
            Ok(id) => id,
            Err(e) => return Ok(e),
        };

        let status_id = match self.default_status_id(project_id).await {
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

        // Build extension_metadata with agent context
        let mut agent_context = serde_json::Map::new();
        if let Some(r) = repo_name {
            agent_context.insert("repo_name".into(), serde_json::json!(r));
        }
        if let Some(f) = file_path {
            agent_context.insert("file_path".into(), serde_json::json!(f));
        }
        if let Some(ln) = line_number {
            agent_context.insert("line_number".into(), serde_json::json!(ln));
        }
        if let Some(g) = git_ref {
            agent_context.insert("git_ref".into(), serde_json::json!(g));
        }
        if let Some(e) = error_output {
            agent_context.insert("error_output".into(), serde_json::json!(e));
        }
        if let Some(c) = command {
            agent_context.insert("command".into(), serde_json::json!(c));
        }

        let extension_metadata = serde_json::json!({ "agent_context": agent_context });

        let payload = api_types::CreateIssueRequest {
            id: None,
            project_id,
            status_id,
            title,
            description: expanded_description,
            priority: None,
            start_date: None,
            target_date: None,
            completed_at: None,
            sort_order: 0.0,
            parent_issue_id: None,
            parent_issue_sort_order: None,
            extension_metadata,
        };

        let url = self.url("/api/remote/issues");
        let response: MutationResponse<Issue> =
            match self.send_json(self.client.post(&url).json(&payload)).await {
                Ok(r) => r,
                Err(e) => return Ok(e),
            };

        TaskServer::success(&McpCreateIssueWithContextResponse {
            issue_id: response.data.id.to_string(),
        })
    }

    #[tool(
        description = "Check if the Wickeban server is running and reachable. Call this before creating issues to ensure the server is available."
    )]
    async fn ping(&self) -> Result<CallToolResult, ErrorData> {
        let url = self.url("/health");
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
        let mut instruction = "A task and project management server. If you need to create or update tickets or issues then use these tools. Most of them absolutely require that you pass the `project_id` of the project that you are currently working on. You can get project ids by using `list_projects`. Call `list_issues` to fetch the `issue_ids` of all the issues in a project. TOOLS: 'ping', 'list_organizations', 'list_projects', 'list_issues', 'search_issues', 'create_issue', 'create_issue_with_context', 'get_issue', 'update_issue', 'delete_issue', 'add_comment'. Before creating an issue, use 'search_issues' to check for duplicates. Use 'add_comment' to append context to existing issues. Use 'create_issue_with_context' to provide structured metadata (file paths, error output, git refs). Use 'ping' to verify the server is running. Make sure to pass `project_id` or `issue_id` where required.".to_string();
        if self.context.is_some() {
            let context_instruction = "Use 'get_context' to fetch project/issue/workspace metadata for the active Wickeban workspace session when available.";
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
