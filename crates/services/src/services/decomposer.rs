use std::{collections::HashMap, path::Path};

use db::models::{
    spec_sheet::SpecSheet,
    task::{CreateTask, Task, TaskStatus},
    task_dependency::TaskDependency,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DecomposerError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("Failed to run decomposition: {0}")]
    ExecutionFailed(String),
    #[error("Failed to parse decomposition output: {0}")]
    ParseFailed(String),
    #[error("No stories returned from decomposition")]
    NoStories,
}

#[derive(Debug, Deserialize)]
pub struct DecompositionStory {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default, rename = "dependsOn")]
    pub depends_on: Vec<String>,
    #[serde(default, rename = "sortOrder")]
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
pub struct DecompositionResult {
    pub stories: Vec<DecompositionStory>,
}

/// Build a prompt that instructs Claude to decompose a spec into granular stories.
pub fn generate_decomposition_prompt(spec: &SpecSheet, task_title: &str) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Story Decomposition: {}\n\n", task_title));
    prompt.push_str(
        "You are a senior software architect. Decompose the following specification into granular implementation stories.\n\n",
    );

    prompt.push_str("## Specification\n\n");

    if let Some(overview) = &spec.overview {
        prompt.push_str("### Overview\n");
        prompt.push_str(overview);
        prompt.push_str("\n\n");
    }

    if let Some(requirements) = &spec.requirements {
        if let Ok(items) = serde_json::from_str::<Vec<String>>(requirements) {
            if !items.is_empty() {
                prompt.push_str("### Requirements\n");
                for item in &items {
                    prompt.push_str(&format!("- {}\n", item));
                }
                prompt.push('\n');
            }
        }
    }

    if let Some(acceptance_criteria) = &spec.acceptance_criteria {
        if let Ok(items) = serde_json::from_str::<Vec<String>>(acceptance_criteria) {
            if !items.is_empty() {
                prompt.push_str("### Acceptance Criteria\n");
                for item in &items {
                    prompt.push_str(&format!("- {}\n", item));
                }
                prompt.push('\n');
            }
        }
    }

    if let Some(constraints) = &spec.constraints {
        if let Ok(items) = serde_json::from_str::<Vec<String>>(constraints) {
            if !items.is_empty() {
                prompt.push_str("### Constraints\n");
                for item in &items {
                    prompt.push_str(&format!("- {}\n", item));
                }
                prompt.push('\n');
            }
        }
    }

    if let Some(tech_notes) = &spec.tech_notes {
        prompt.push_str("### Technical Notes\n");
        prompt.push_str(tech_notes);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Decomposition Rules\n\n");
    prompt.push_str("1. Each story must be small enough to not exceed 40% of a coding agent's context window. If a story feels too large, break it into multiple stories.\n");
    prompt.push_str("2. Each story should be independently implementable (given its dependencies are complete).\n");
    prompt.push_str("3. Use `dependsOn` to express ordering — a story should list the IDs of stories that must complete before it can start.\n");
    prompt.push_str(
        "4. Order stories by `sortOrder` to indicate the preferred execution sequence.\n",
    );
    prompt.push_str("5. Stories should cover the full scope of the specification.\n");
    prompt.push_str("6. Each story's description should contain enough detail for a coding agent to implement it without additional context.\n\n");

    prompt.push_str("## Output Format\n\n");
    prompt.push_str("Respond with ONLY a JSON object in this exact format. No markdown fences, no explanatory text.\n\n");
    prompt.push_str(r#"{ "stories": [{ "id": "story-1", "title": "...", "description": "...", "dependsOn": [], "sortOrder": 1 }] }"#);
    prompt.push('\n');

    prompt
}

/// Find the start of a JSON object in text that may contain bare `{` in URLs.
/// Returns the slice from the first `{` that is followed by optional whitespace
/// then `"` (indicating a JSON key), through the last `}`.
fn find_json_object_start(s: &str) -> Option<&str> {
    let last_brace = s.rfind('}')?;
    let mut search_from = 0;
    while let Some(pos) = s[search_from..].find('{') {
        let abs_pos = search_from + pos;
        // Check if what follows the `{` (after optional whitespace) is `"`
        let after = s[abs_pos + 1..].trim_start();
        if after.starts_with('"') {
            return Some(&s[abs_pos..=last_brace]);
        }
        search_from = abs_pos + 1;
    }
    None
}

/// Parse the raw output from Claude into a DecompositionResult.
pub fn parse_decomposition_output(output: &str) -> Result<DecompositionResult, DecomposerError> {
    let trimmed = output.trim();

    // Strip markdown fences if present
    let json_str = if trimmed.starts_with("```") {
        let without_opening = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
    } else {
        trimmed
    };

    // Try to find JSON object boundaries if there's surrounding text.
    // We need to find the actual JSON object start, skipping bare `{` in URL
    // templates like `{event}/trade-stats`. Look for `{` followed by optional
    // whitespace then `"` which indicates a JSON object with a string key.
    let json_str = if json_str.starts_with('{') {
        json_str
    } else {
        find_json_object_start(json_str).unwrap_or(json_str)
    };

    let result: DecompositionResult = serde_json::from_str(json_str)
        .map_err(|e| DecomposerError::ParseFailed(format!("{e}: input was: {json_str}")))?;

    if result.stories.is_empty() {
        return Err(DecomposerError::NoStories);
    }

    Ok(result)
}

/// Shell out to `claude --print -p <prompt>` to get decomposition output.
pub fn run_decomposition(prompt: &str, working_dir: &Path) -> Result<String, DecomposerError> {
    let output = std::process::Command::new("claude")
        .args(["--print", "-p", prompt])
        .current_dir(working_dir)
        .output()
        .map_err(|e| DecomposerError::ExecutionFailed(format!("Failed to spawn claude: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecomposerError::ExecutionFailed(format!(
            "claude exited with status {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

/// Create child tasks from decomposition result and wire up dependencies.
/// Children are linked to the parent via `parent_task_id` (decomposition relationship).
/// `parent_workspace_id` is left None — it gets assigned later when a sprint starts.
pub async fn create_child_tasks(
    pool: &SqlitePool,
    project_id: Uuid,
    parent_task_id: Uuid,
    result: &DecompositionResult,
) -> Result<Vec<Task>, DecomposerError> {
    // Map temporary story IDs to real UUIDs
    let mut id_map: HashMap<String, Uuid> = HashMap::new();
    for story in &result.stories {
        id_map.insert(story.id.clone(), Uuid::new_v4());
    }

    // Create all tasks
    let mut tasks = Vec::with_capacity(result.stories.len());
    for story in &result.stories {
        let task_id = id_map[&story.id];
        let create_task = CreateTask {
            project_id,
            title: story.title.clone(),
            description: Some(story.description.clone()),
            status: Some(TaskStatus::Ready),
            parent_workspace_id: None,
            parent_task_id: Some(parent_task_id),
            image_ids: None,
            sort_order: Some(story.sort_order),
            plan_status: None,
        };
        let task = Task::create(pool, &create_task, task_id).await?;
        tasks.push(task);
    }

    // Create dependency records
    for story in &result.stories {
        let task_id = id_map[&story.id];
        let dep_ids: Vec<Uuid> = story
            .depends_on
            .iter()
            .filter_map(|dep_id| id_map.get(dep_id).copied())
            .collect();
        if !dep_ids.is_empty() {
            TaskDependency::create_many(pool, task_id, &dep_ids).await?;
        }
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn test_generate_decomposition_prompt() {
        let spec = SpecSheet {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            overview: Some("Build a login page".to_string()),
            requirements: Some(r#"["Email input","Password input"]"#.to_string()),
            acceptance_criteria: Some(r#"["User can log in"]"#.to_string()),
            constraints: Some(r#"["Use existing auth"]"#.to_string()),
            tech_notes: Some("Use React Hook Form".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let prompt = generate_decomposition_prompt(&spec, "Login Page");
        assert!(prompt.contains("# Story Decomposition: Login Page"));
        assert!(prompt.contains("Build a login page"));
        assert!(prompt.contains("40%"));
        assert!(prompt.contains("dependsOn"));
    }

    #[test]
    fn test_parse_decomposition_output_clean_json() {
        let output = r#"{ "stories": [{ "id": "story-1", "title": "Create schema", "description": "Set up the database schema", "dependsOn": [], "sortOrder": 1 }, { "id": "story-2", "title": "Add API", "description": "Create REST endpoints", "dependsOn": ["story-1"], "sortOrder": 2 }] }"#;
        let result = parse_decomposition_output(output).unwrap();
        assert_eq!(result.stories.len(), 2);
        assert_eq!(result.stories[0].title, "Create schema");
        assert_eq!(result.stories[1].depends_on, vec!["story-1"]);
    }

    #[test]
    fn test_parse_decomposition_output_with_markdown_fences() {
        let output = "```json\n{ \"stories\": [{ \"id\": \"s1\", \"title\": \"Test\", \"description\": \"desc\", \"dependsOn\": [], \"sortOrder\": 1 }] }\n```";
        let result = parse_decomposition_output(output).unwrap();
        assert_eq!(result.stories.len(), 1);
    }

    #[test]
    fn test_parse_decomposition_output_empty_stories() {
        let output = r#"{ "stories": [] }"#;
        assert!(parse_decomposition_output(output).is_err());
    }

    #[test]
    fn test_parse_decomposition_output_with_url_braces_before_json() {
        let output = "Route `{event}/trade-stats` needs naming.\n{\"stories\": [{ \"id\": \"s1\", \"title\": \"Test\", \"description\": \"desc\", \"dependsOn\": [], \"sortOrder\": 1 }]}";
        let result = parse_decomposition_output(output).unwrap();
        assert_eq!(result.stories.len(), 1);
        assert_eq!(result.stories[0].title, "Test");
    }
}
