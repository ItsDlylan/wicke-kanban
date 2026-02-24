use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, Sqlite, SqlitePool, Type};
use strum_macros::{Display, EnumString};
use ts_rs::TS;
use uuid::Uuid;

use super::{project::Project, workspace::Workspace};

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "task_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Backlog,
    PlanGenerating,
    Ready,
    Ralph,
    InProgress,
    QA,
    Done,
    Cancelled,
    Idea,
    Planning,
    SpecReview,
}

#[derive(
    Debug, Clone, Type, Serialize, Deserialize, PartialEq, TS, EnumString, Display, Default,
)]
#[sqlx(type_name = "task_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TaskType {
    #[default]
    Task,
    Epic,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, TS)]
pub struct Task {
    pub id: Uuid,
    pub project_id: Uuid, // Foreign key to Project
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub task_type: TaskType,
    pub parent_workspace_id: Option<Uuid>, // Foreign key to parent Workspace (sprint execution)
    pub parent_task_id: Option<Uuid>,      // Foreign key to parent Task (decomposition)
    pub sort_order: i32,
    pub plan: Option<String>,
    pub plan_status: Option<String>,
    pub is_human: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskWithAttemptStatus {
    #[serde(flatten)]
    #[ts(flatten)]
    pub task: Task,
    pub has_in_progress_attempt: bool,
    pub last_attempt_failed: bool,
    pub executor: String,
    pub has_spec: bool,
    pub has_children: bool,
}

impl std::ops::Deref for TaskWithAttemptStatus {
    type Target = Task;
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}

impl std::ops::DerefMut for TaskWithAttemptStatus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.task
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TaskRelationships {
    pub parent_task: Option<Task>, // The task that owns the parent workspace
    pub current_workspace: Workspace, // The workspace we're viewing
    pub children: Vec<Task>,       // Tasks created from this workspace
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CreateTask {
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub task_type: Option<TaskType>,
    pub parent_workspace_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
    pub sort_order: Option<i32>,
    pub plan_status: Option<String>,
    pub is_human: Option<bool>,
}

impl CreateTask {
    pub fn from_title_description(
        project_id: Uuid,
        title: String,
        description: Option<String>,
    ) -> Self {
        Self {
            project_id,
            title,
            description,
            status: Some(TaskStatus::Backlog),
            task_type: None,
            parent_workspace_id: None,
            parent_task_id: None,
            image_ids: None,
            sort_order: None,
            plan_status: None,
            is_human: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_workspace_id: Option<Uuid>,
    pub image_ids: Option<Vec<Uuid>>,
}

impl Task {
    pub fn to_prompt(&self) -> String {
        if let Some(description) = self.description.as_ref().filter(|d| !d.trim().is_empty()) {
            format!("{}\n\n{}", &self.title, description)
        } else {
            self.title.clone()
        }
    }

    pub async fn parent_project(&self, pool: &SqlitePool) -> Result<Option<Project>, sqlx::Error> {
        Project::find_by_id(pool, self.project_id).await
    }

    pub async fn find_by_project_id_with_attempt_status(
        pool: &SqlitePool,
        project_id: Uuid,
    ) -> Result<Vec<TaskWithAttemptStatus>, sqlx::Error> {
        let records = sqlx::query!(
            r#"SELECT
  t.id                            AS "id!: Uuid",
  t.project_id                    AS "project_id!: Uuid",
  t.title,
  t.description,
  t.status                        AS "status!: TaskStatus",
  t.task_type                     AS "task_type!: TaskType",
  t.parent_workspace_id           AS "parent_workspace_id: Uuid",
  t.parent_task_id                AS "parent_task_id: Uuid",
  t.created_at                    AS "created_at!: DateTime<Utc>",
  t.updated_at                    AS "updated_at!: DateTime<Utc>",
  t.sort_order                    AS "sort_order!: i32",
  t.plan                          AS "plan: String",
  t.plan_status                   AS "plan_status: String",
  t.is_human                      AS "is_human!: bool",

  CASE WHEN EXISTS (
    SELECT 1
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
       AND ep.status        = 'running'
       AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     LIMIT 1
  ) THEN 1 ELSE 0 END            AS "has_in_progress_attempt!: i64",

  CASE WHEN (
    SELECT ep.status
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      JOIN execution_processes ep ON ep.session_id = s.id
     WHERE w.task_id       = t.id
     AND ep.run_reason IN ('setupscript','cleanupscript','codingagent')
     ORDER BY ep.created_at DESC
     LIMIT 1
  ) IN ('failed','killed') THEN 1 ELSE 0 END
                                 AS "last_attempt_failed!: i64",

  ( SELECT s.executor
      FROM workspaces w
      JOIN sessions s ON s.workspace_id = w.id
      WHERE w.task_id = t.id
     ORDER BY s.created_at DESC
      LIMIT 1
    )                               AS "executor!: String",

  CASE WHEN EXISTS (
    SELECT 1 FROM spec_sheets ss WHERE ss.task_id = t.id LIMIT 1
  ) THEN 1 ELSE 0 END              AS "has_spec!: i64",

  CASE WHEN EXISTS (
    SELECT 1 FROM tasks c WHERE c.parent_task_id = t.id LIMIT 1
  ) THEN 1 ELSE 0 END              AS "has_children!: i64"

FROM tasks t
WHERE t.project_id = $1
ORDER BY t.created_at DESC"#,
            project_id
        )
        .fetch_all(pool)
        .await?;

        let tasks = records
            .into_iter()
            .map(|rec| TaskWithAttemptStatus {
                task: Task {
                    id: rec.id,
                    project_id: rec.project_id,
                    title: rec.title,
                    description: rec.description,
                    status: rec.status,
                    task_type: rec.task_type,
                    parent_workspace_id: rec.parent_workspace_id,
                    parent_task_id: rec.parent_task_id,
                    sort_order: rec.sort_order,
                    plan: rec.plan,
                    plan_status: rec.plan_status,
                    is_human: rec.is_human,
                    created_at: rec.created_at,
                    updated_at: rec.updated_at,
                },
                has_in_progress_attempt: rec.has_in_progress_attempt != 0,
                last_attempt_failed: rec.last_attempt_failed != 0,
                executor: rec.executor,
                has_spec: rec.has_spec != 0,
                has_children: rec.has_children != 0,
            })
            .collect();

        Ok(tasks)
    }

    pub async fn find_all(pool: &SqlitePool) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               ORDER BY created_at ASC"#
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE id = $1"#,
            id
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_rowid(pool: &SqlitePool, rowid: i64) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE rowid = $1"#,
            rowid
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn create(
        pool: &SqlitePool,
        data: &CreateTask,
        task_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let status = data.status.clone().unwrap_or_default();
        let task_type = data.task_type.clone().unwrap_or_default();
        let sort_order = data.sort_order.unwrap_or(0);
        let is_human = data.is_human.unwrap_or(false);
        sqlx::query_as!(
            Task,
            r#"INSERT INTO tasks (id, project_id, title, description, status, task_type, parent_workspace_id, parent_task_id, sort_order, plan_status, is_human)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            task_id,
            data.project_id,
            data.title,
            data.description,
            status,
            task_type,
            data.parent_workspace_id,
            data.parent_task_id,
            sort_order,
            data.plan_status,
            is_human
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        status: TaskStatus,
        parent_workspace_id: Option<Uuid>,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"UPDATE tasks
               SET title = $3, description = $4, status = $5, parent_workspace_id = $6
               WHERE id = $1 AND project_id = $2
               RETURNING id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>""#,
            id,
            project_id,
            title,
            description,
            status,
            parent_workspace_id
        )
        .fetch_one(pool)
        .await
    }

    pub async fn update_status(
        pool: &SqlitePool,
        id: Uuid,
        status: TaskStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            status
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update the parent_workspace_id field for a task
    pub async fn update_parent_workspace_id(
        pool: &SqlitePool,
        task_id: Uuid,
        parent_workspace_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET parent_workspace_id = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            task_id,
            parent_workspace_id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Nullify parent_workspace_id for all tasks that reference the given workspace ID
    /// This breaks parent-child relationships before deleting a parent task
    pub async fn nullify_children_by_workspace_id<'e, E>(
        executor: E,
        workspace_id: Uuid,
    ) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!(
            "UPDATE tasks SET parent_workspace_id = NULL WHERE parent_workspace_id = $1",
            workspace_id
        )
        .execute(executor)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn delete<'e, E>(executor: E, id: Uuid) -> Result<u64, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let result = sqlx::query!("DELETE FROM tasks WHERE id = $1", id)
            .execute(executor)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn find_children_by_workspace_id(
        pool: &SqlitePool,
        workspace_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Find only child tasks that have this workspace as their parent
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE parent_workspace_id = $1
               ORDER BY created_at DESC"#,
            workspace_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_relationships_for_workspace(
        pool: &SqlitePool,
        workspace: &Workspace,
    ) -> Result<TaskRelationships, sqlx::Error> {
        // 1. Get the current task (task that owns this workspace)
        let current_task = Self::find_by_id(pool, workspace.task_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;

        // 2. Get parent task (if current task was created by another workspace)
        let parent_task = if let Some(parent_workspace_id) = current_task.parent_workspace_id {
            // Find the workspace that created the current task
            if let Ok(Some(parent_workspace)) =
                Workspace::find_by_id(pool, parent_workspace_id).await
            {
                // Find the task that owns that parent workspace - THAT's the real parent
                Self::find_by_id(pool, parent_workspace.task_id).await?
            } else {
                None
            }
        } else {
            None
        };

        // 3. Get children tasks (created from this workspace)
        let children = Self::find_children_by_workspace_id(pool, workspace.id).await?;

        Ok(TaskRelationships {
            parent_task,
            current_workspace: workspace.clone(),
            children,
        })
    }

    /// Find next eligible child task for Ralph loop execution.
    /// Returns the first child (by sort_order) that is in Todo status
    /// and has all its dependencies in Done status.
    pub async fn find_next_eligible_child(
        pool: &SqlitePool,
        parent_workspace_id: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT t.id as "id!: Uuid", t.project_id as "project_id!: Uuid", t.title, t.description, t.status as "status!: TaskStatus", t.task_type as "task_type!: TaskType", t.parent_workspace_id as "parent_workspace_id: Uuid", t.parent_task_id as "parent_task_id: Uuid", t.sort_order as "sort_order!: i32", t.plan, t.plan_status, t.is_human as "is_human!: bool", t.created_at as "created_at!: DateTime<Utc>", t.updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks t
               WHERE t.parent_workspace_id = $1
                 AND t.status = 'ready'
                 AND NOT EXISTS (
                     SELECT 1 FROM task_dependencies td
                     JOIN tasks dep ON dep.id = td.depends_on
                     WHERE td.task_id = t.id AND dep.status != 'done'
                 )
               ORDER BY t.sort_order ASC
               LIMIT 1"#,
            parent_workspace_id
        )
        .fetch_optional(pool)
        .await
    }

    /// Check if all children of a parent workspace are done.
    pub async fn all_children_done(
        pool: &SqlitePool,
        parent_workspace_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "total!: i64",
                      SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) as "done!: i64"
               FROM tasks
               WHERE parent_workspace_id = $1"#,
            parent_workspace_id
        )
        .fetch_one(pool)
        .await?;

        Ok(result.total > 0 && result.total == result.done)
    }

    /// Find all child tasks of a parent task (decomposition children), ordered by sort_order.
    pub async fn find_by_parent_task_id(
        pool: &SqlitePool,
        parent_task_id: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description, status as "status!: TaskStatus", task_type as "task_type!: TaskType", parent_workspace_id as "parent_workspace_id: Uuid", parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32", plan, plan_status, is_human as "is_human!: bool", created_at as "created_at!: DateTime<Utc>", updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE parent_task_id = $1
               ORDER BY sort_order ASC"#,
            parent_task_id
        )
        .fetch_all(pool)
        .await
    }

    /// Check if ALL children of a parent task (across all sprints) are done.
    pub async fn all_parent_children_done(
        pool: &SqlitePool,
        parent_task_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "total!: i64",
                      SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) as "done!: i64"
               FROM tasks
               WHERE parent_task_id = $1"#,
            parent_task_id
        )
        .fetch_one(pool)
        .await?;

        Ok(result.total > 0 && result.total == result.done)
    }

    /// Update the plan text and plan_status for a task.
    pub async fn update_plan(
        pool: &SqlitePool,
        id: Uuid,
        plan: &str,
        plan_status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET plan = $2, plan_status = $3, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            plan,
            plan_status
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_routing_decision(
        pool: &SqlitePool,
        id: Uuid,
        decision: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET routing_decision = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            decision
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Update only the plan_status for a task.
    pub async fn update_plan_status(
        pool: &SqlitePool,
        id: Uuid,
        plan_status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "UPDATE tasks SET plan_status = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
            id,
            plan_status
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Find and reset any tasks stuck in 'generating' plan_status back to 'pending'.
    /// Returns the tasks that were reset so callers can re-trigger plan generation.
    pub async fn reset_stuck_generating_plans(pool: &SqlitePool) -> Result<Vec<Task>, sqlx::Error> {
        let tasks = sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description,
                      status as "status!: TaskStatus", task_type as "task_type!: TaskType",
                      parent_workspace_id as "parent_workspace_id: Uuid",
                      parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32",
                      plan, plan_status, is_human as "is_human!: bool",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks WHERE plan_status = 'generating'"#
        )
        .fetch_all(pool)
        .await?;

        if !tasks.is_empty() {
            sqlx::query!(
                "UPDATE tasks SET plan_status = 'pending', updated_at = CURRENT_TIMESTAMP WHERE plan_status = 'generating'"
            )
            .execute(pool)
            .await?;
        }

        Ok(tasks)
    }

    /// Find tasks stuck in PlanGenerating status but with plan_status = 'completed'.
    /// These are tasks where plan generation finished but the server restarted before
    /// auto_prepare_for_ralph and the Ready transition could complete.
    pub async fn find_stuck_plan_completed(pool: &SqlitePool) -> Result<Vec<Task>, sqlx::Error> {
        sqlx::query_as!(
            Task,
            r#"SELECT id as "id!: Uuid", project_id as "project_id!: Uuid", title, description,
                      status as "status!: TaskStatus", task_type as "task_type!: TaskType",
                      parent_workspace_id as "parent_workspace_id: Uuid",
                      parent_task_id as "parent_task_id: Uuid", sort_order as "sort_order!: i32",
                      plan, plan_status, is_human as "is_human!: bool",
                      created_at as "created_at!: DateTime<Utc>",
                      updated_at as "updated_at!: DateTime<Utc>"
               FROM tasks
               WHERE status = 'plangenerating' AND plan_status = 'completed'"#
        )
        .fetch_all(pool)
        .await
    }

    /// Count children tasks and how many are done.
    pub async fn count_children(
        pool: &SqlitePool,
        parent_workspace_id: Uuid,
    ) -> Result<(i64, i64), sqlx::Error> {
        let result = sqlx::query!(
            r#"SELECT COUNT(*) as "total!: i64",
                      SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END) as "done!: i64"
               FROM tasks
               WHERE parent_workspace_id = $1"#,
            parent_workspace_id
        )
        .fetch_one(pool)
        .await?;

        Ok((result.done, result.total))
    }
}
