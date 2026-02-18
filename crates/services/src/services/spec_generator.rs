use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpecGeneratorError {
    #[error("Failed to run spec generation: {0}")]
    ExecutionFailed(String),
    #[error("Failed to parse spec output: {0}")]
    ParseFailed(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeneratedSpec {
    pub overview: String,
    pub requirements: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub constraints: Vec<String>,
    pub tech_notes: String,
}

/// Build a prompt that instructs Claude to analyze the codebase and produce a spec sheet
/// from a task's title, description, and plan.
pub fn build_spec_generation_prompt(
    title: &str,
    description: Option<&str>,
    plan: Option<&str>,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Spec Sheet Generation: {}\n\n", title));

    if let Some(desc) = description {
        if !desc.trim().is_empty() {
            prompt.push_str("## Task Description\n");
            prompt.push_str(desc);
            prompt.push_str("\n\n");
        }
    }

    if let Some(plan_text) = plan {
        if !plan_text.trim().is_empty() {
            prompt.push_str("## Implementation Plan\n");
            prompt.push_str(plan_text);
            prompt.push_str("\n\n");
        }
    }

    prompt.push_str("## Instructions\n\n");
    prompt.push_str("Analyze the codebase and the task context above to produce a detailed spec sheet for this task.\n\n");
    prompt.push_str("The spec sheet should contain:\n");
    prompt.push_str("1. **overview**: A concise summary of what this task accomplishes and why.\n");
    prompt.push_str("2. **requirements**: A list of specific functional requirements.\n");
    prompt.push_str("3. **acceptance_criteria**: A list of verifiable acceptance criteria.\n");
    prompt.push_str("4. **constraints**: A list of technical or business constraints.\n");
    prompt.push_str("5. **tech_notes**: Any technical implementation notes, patterns to follow, or important details.\n\n");

    prompt.push_str("## Output Format\n\n");
    prompt.push_str("Respond with ONLY a JSON object in this exact format. No markdown fences, no explanatory text.\n\n");
    prompt.push_str(r#"{ "overview": "...", "requirements": ["..."], "acceptance_criteria": ["..."], "constraints": ["..."], "tech_notes": "..." }"#);
    prompt.push('\n');

    prompt
}

/// Shell out to `claude --print -p <prompt>` to generate a spec.
/// This is a blocking call — use `spawn_blocking` from async context.
pub fn run_spec_generation(prompt: &str, working_dir: &Path) -> Result<String, SpecGeneratorError> {
    let output = std::process::Command::new("claude")
        .args(["--print", "-p", prompt])
        .current_dir(working_dir)
        .output()
        .map_err(|e| SpecGeneratorError::ExecutionFailed(format!("Failed to spawn claude: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SpecGeneratorError::ExecutionFailed(format!(
            "claude exited with status {}: {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

/// Find the start of a JSON object in text that may contain bare `{` in URLs.
/// Returns the slice from the first `{` that is followed by optional whitespace
/// then `"` (indicating a JSON key), through the last `}`.
fn find_json_object_start(s: &str) -> Option<&str> {
    let last_brace = s.rfind('}')?;
    let mut search_from = 0;
    while let Some(pos) = s[search_from..].find('{') {
        let abs_pos = search_from + pos;
        let after = s[abs_pos + 1..].trim_start();
        if after.starts_with('"') {
            return Some(&s[abs_pos..=last_brace]);
        }
        search_from = abs_pos + 1;
    }
    None
}

/// Parse the raw output from Claude into a GeneratedSpec.
pub fn parse_spec_output(output: &str) -> Result<GeneratedSpec, SpecGeneratorError> {
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

    let result: GeneratedSpec = serde_json::from_str(json_str)
        .map_err(|e| SpecGeneratorError::ParseFailed(format!("{e}: input was: {json_str}")))?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_spec_generation_prompt() {
        let prompt = build_spec_generation_prompt(
            "Add login page",
            Some("Create a login page with email and password"),
            Some("1. Create LoginForm component\n2. Add validation"),
        );
        assert!(prompt.contains("# Spec Sheet Generation: Add login page"));
        assert!(prompt.contains("Create a login page"));
        assert!(prompt.contains("Create LoginForm component"));
        assert!(prompt.contains("overview"));
        assert!(prompt.contains("requirements"));
    }

    #[test]
    fn test_build_spec_generation_prompt_no_optional_fields() {
        let prompt = build_spec_generation_prompt("Simple task", None, None);
        assert!(prompt.contains("# Spec Sheet Generation: Simple task"));
        assert!(!prompt.contains("## Task Description"));
        assert!(!prompt.contains("## Implementation Plan"));
    }

    #[test]
    fn test_parse_spec_output_clean_json() {
        let output = r#"{ "overview": "Build a login page", "requirements": ["Email input", "Password input"], "acceptance_criteria": ["User can log in"], "constraints": ["Use existing auth"], "tech_notes": "Use React Hook Form" }"#;
        let result = parse_spec_output(output).unwrap();
        assert_eq!(result.overview, "Build a login page");
        assert_eq!(result.requirements.len(), 2);
        assert_eq!(result.acceptance_criteria, vec!["User can log in"]);
        assert_eq!(result.constraints, vec!["Use existing auth"]);
        assert_eq!(result.tech_notes, "Use React Hook Form");
    }

    #[test]
    fn test_parse_spec_output_with_markdown_fences() {
        let output = "```json\n{ \"overview\": \"Test\", \"requirements\": [], \"acceptance_criteria\": [], \"constraints\": [], \"tech_notes\": \"\" }\n```";
        let result = parse_spec_output(output).unwrap();
        assert_eq!(result.overview, "Test");
    }

    #[test]
    fn test_parse_spec_output_with_surrounding_text() {
        let output = "Here is the spec:\n{ \"overview\": \"Test\", \"requirements\": [], \"acceptance_criteria\": [], \"constraints\": [], \"tech_notes\": \"\" }\nDone!";
        let result = parse_spec_output(output).unwrap();
        assert_eq!(result.overview, "Test");
    }

    #[test]
    fn test_parse_spec_output_with_url_braces_before_json() {
        let output = "Route `{event}/trade-stats` needs naming.\n{\"overview\": \"Test\", \"requirements\": [], \"acceptance_criteria\": [], \"constraints\": [], \"tech_notes\": \"\"}";
        let result = parse_spec_output(output).unwrap();
        assert_eq!(result.overview, "Test");
    }

    #[test]
    fn test_parse_spec_output_invalid_json() {
        let output = "not json at all";
        assert!(parse_spec_output(output).is_err());
    }
}
