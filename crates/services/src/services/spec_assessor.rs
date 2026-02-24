use std::path::Path;

use db::models::spec_sheet::SpecSheet;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum SpecAssessorError {
    #[error("Failed to run assessment: {0}")]
    ExecutionFailed(String),
    #[error("Failed to parse assessment output: {0}")]
    ParseFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SpecAssessment {
    pub complexity_score: u8,
    pub files_estimated: u32,
    pub subsystems: Vec<String>,
    pub context_estimate: f64,
    pub decomposability: String,
    pub recommended: String,
    pub confidence: f64,
}

/// Build a prompt that instructs Claude to evaluate a spec sheet's complexity and recommend
/// a routing decision.
pub fn build_assessment_prompt(spec: &SpecSheet, task_title: &str) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Complexity Assessment: {}\n\n", task_title));
    prompt.push_str(
        "You are a senior software architect. Evaluate the following specification's complexity and recommend a routing decision for how it should be executed.\n\n",
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

    prompt.push_str("## Assessment Criteria\n\n");
    prompt.push_str("Evaluate the specification on these dimensions:\n");
    prompt.push_str("1. **complexity_score** (1-10): Overall implementation complexity.\n");
    prompt.push_str(
        "2. **files_estimated**: Number of files that will need to be created or modified.\n",
    );
    prompt.push_str("3. **subsystems**: List of distinct subsystems or modules involved (e.g. \"database\", \"API\", \"frontend\", \"auth\").\n");
    prompt.push_str("4. **context_estimate**: Estimated fraction of a 200K token context window needed to hold all relevant code. Use decimal (e.g. 0.3 = 30%).\n");
    prompt.push_str("5. **decomposability**: How easily the task can be split into independent sub-tasks (\"high\", \"medium\", or \"low\").\n");
    prompt.push_str("6. **recommended**: Routing decision based on your assessment:\n");
    prompt.push_str("   - \"single\" — fits in one agent's context (complexity 1-3)\n");
    prompt.push_str("   - \"single_verifier\" — one agent + verification pass (complexity 4-5)\n");
    prompt.push_str(
        "   - \"vs_shallow\" — verified succession with shallow depth (complexity 6-8)\n",
    );
    prompt.push_str(
        "   - \"vs_deep\" — verified succession with deep multi-agent (complexity 9-10)\n",
    );
    prompt.push_str("7. **confidence**: Your confidence in this assessment (0.0-1.0).\n\n");

    prompt.push_str("## Output Format\n\n");
    prompt.push_str("Respond with ONLY a JSON object in this exact format. No markdown fences, no explanatory text.\n\n");
    prompt.push_str(r#"{ "complexity_score": 5, "files_estimated": 8, "subsystems": ["database", "API"], "context_estimate": 0.4, "decomposability": "high", "recommended": "single_verifier", "confidence": 0.85 }"#);
    prompt.push('\n');

    prompt
}

/// Shell out to `claude --print -p <prompt>` to get assessment output.
/// This is a blocking call -- use `spawn_blocking` from async context.
pub fn run_assessment(prompt: &str, working_dir: &Path) -> Result<String, SpecAssessorError> {
    let output = std::process::Command::new("claude")
        .args(["--print", "-p", prompt])
        .current_dir(working_dir)
        .output()
        .map_err(|e| SpecAssessorError::ExecutionFailed(format!("Failed to spawn claude: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SpecAssessorError::ExecutionFailed(format!(
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

/// Parse the raw output from Claude into a SpecAssessment.
pub fn parse_assessment_output(output: &str) -> Result<SpecAssessment, SpecAssessorError> {
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
    let json_str = if json_str.starts_with('{') {
        json_str
    } else {
        find_json_object_start(json_str).unwrap_or(json_str)
    };

    let result: SpecAssessment = serde_json::from_str(json_str)
        .map_err(|e| SpecAssessorError::ParseFailed(format!("{e}: input was: {json_str}")))?;

    Ok(result)
}

/// Apply the routing table to determine the execution strategy from an assessment.
/// Uses `complexity_score` as primary, `context_estimate` as tiebreaker.
pub fn route_from_score(assessment: &SpecAssessment) -> String {
    match assessment.complexity_score {
        1..=3 => {
            if assessment.context_estimate >= 0.4 {
                "single_verifier".to_string()
            } else {
                "single".to_string()
            }
        }
        4..=5 => {
            if assessment.context_estimate > 0.7 {
                "vs_shallow".to_string()
            } else {
                "single_verifier".to_string()
            }
        }
        6..=8 => {
            if assessment.context_estimate > 2.0 {
                "vs_deep".to_string()
            } else {
                "vs_shallow".to_string()
            }
        }
        _ => "vs_deep".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn make_spec(overview: &str) -> SpecSheet {
        SpecSheet {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            overview: Some(overview.to_string()),
            requirements: Some(r#"["Req A","Req B"]"#.to_string()),
            acceptance_criteria: Some(r#"["AC 1"]"#.to_string()),
            constraints: Some(r#"["Constraint 1"]"#.to_string()),
            tech_notes: Some("Use existing patterns".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_parse_assessment_output_clean_json() {
        let output = r#"{ "complexity_score": 5, "files_estimated": 8, "subsystems": ["database", "API"], "context_estimate": 0.4, "decomposability": "high", "recommended": "single_verifier", "confidence": 0.85 }"#;
        let result = parse_assessment_output(output).unwrap();
        assert_eq!(result.complexity_score, 5);
        assert_eq!(result.files_estimated, 8);
        assert_eq!(result.subsystems, vec!["database", "API"]);
        assert!((result.context_estimate - 0.4).abs() < f64::EPSILON);
        assert_eq!(result.decomposability, "high");
        assert_eq!(result.recommended, "single_verifier");
        assert!((result.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_assessment_output_with_markdown_fences() {
        let output = "```json\n{ \"complexity_score\": 3, \"files_estimated\": 2, \"subsystems\": [\"frontend\"], \"context_estimate\": 0.1, \"decomposability\": \"low\", \"recommended\": \"single\", \"confidence\": 0.9 }\n```";
        let result = parse_assessment_output(output).unwrap();
        assert_eq!(result.complexity_score, 3);
        assert_eq!(result.recommended, "single");
    }

    #[test]
    fn test_parse_assessment_output_with_surrounding_text() {
        let output = "Here is the assessment:\n{ \"complexity_score\": 7, \"files_estimated\": 15, \"subsystems\": [\"db\", \"api\", \"frontend\"], \"context_estimate\": 1.2, \"decomposability\": \"high\", \"recommended\": \"vs_shallow\", \"confidence\": 0.75 }\nDone!";
        let result = parse_assessment_output(output).unwrap();
        assert_eq!(result.complexity_score, 7);
        assert_eq!(result.recommended, "vs_shallow");
    }

    #[test]
    fn test_parse_assessment_output_invalid_json() {
        let output = "not json at all";
        assert!(parse_assessment_output(output).is_err());
    }

    #[test]
    fn test_route_from_score_single() {
        let assessment = SpecAssessment {
            complexity_score: 2,
            files_estimated: 3,
            subsystems: vec!["frontend".to_string()],
            context_estimate: 0.1,
            decomposability: "low".to_string(),
            recommended: "single".to_string(),
            confidence: 0.9,
        };
        assert_eq!(route_from_score(&assessment), "single");
    }

    #[test]
    fn test_route_from_score_single_promoted_by_context() {
        let assessment = SpecAssessment {
            complexity_score: 3,
            files_estimated: 5,
            subsystems: vec!["frontend".to_string(), "api".to_string()],
            context_estimate: 0.5,
            decomposability: "medium".to_string(),
            recommended: "single".to_string(),
            confidence: 0.8,
        };
        // Score 3 but context >= 0.4 promotes to single_verifier
        assert_eq!(route_from_score(&assessment), "single_verifier");
    }

    #[test]
    fn test_route_from_score_single_verifier() {
        let assessment = SpecAssessment {
            complexity_score: 5,
            files_estimated: 8,
            subsystems: vec!["database".to_string(), "api".to_string()],
            context_estimate: 0.5,
            decomposability: "medium".to_string(),
            recommended: "single_verifier".to_string(),
            confidence: 0.85,
        };
        assert_eq!(route_from_score(&assessment), "single_verifier");
    }

    #[test]
    fn test_route_from_score_single_verifier_promoted_by_context() {
        let assessment = SpecAssessment {
            complexity_score: 4,
            files_estimated: 12,
            subsystems: vec![
                "database".to_string(),
                "api".to_string(),
                "frontend".to_string(),
            ],
            context_estimate: 0.8,
            decomposability: "high".to_string(),
            recommended: "single_verifier".to_string(),
            confidence: 0.7,
        };
        // Score 4 but context > 0.7 promotes to vs_shallow
        assert_eq!(route_from_score(&assessment), "vs_shallow");
    }

    #[test]
    fn test_route_from_score_vs_shallow() {
        let assessment = SpecAssessment {
            complexity_score: 7,
            files_estimated: 15,
            subsystems: vec!["db".to_string(), "api".to_string(), "frontend".to_string()],
            context_estimate: 1.2,
            decomposability: "high".to_string(),
            recommended: "vs_shallow".to_string(),
            confidence: 0.75,
        };
        assert_eq!(route_from_score(&assessment), "vs_shallow");
    }

    #[test]
    fn test_route_from_score_vs_shallow_promoted_by_context() {
        let assessment = SpecAssessment {
            complexity_score: 8,
            files_estimated: 25,
            subsystems: vec![
                "db".to_string(),
                "api".to_string(),
                "frontend".to_string(),
                "auth".to_string(),
            ],
            context_estimate: 2.5,
            decomposability: "high".to_string(),
            recommended: "vs_shallow".to_string(),
            confidence: 0.6,
        };
        // Score 8 but context > 2.0 promotes to vs_deep
        assert_eq!(route_from_score(&assessment), "vs_deep");
    }

    #[test]
    fn test_route_from_score_vs_deep() {
        let assessment = SpecAssessment {
            complexity_score: 10,
            files_estimated: 30,
            subsystems: vec![
                "db".to_string(),
                "api".to_string(),
                "frontend".to_string(),
                "auth".to_string(),
                "infra".to_string(),
            ],
            context_estimate: 3.0,
            decomposability: "high".to_string(),
            recommended: "vs_deep".to_string(),
            confidence: 0.7,
        };
        assert_eq!(route_from_score(&assessment), "vs_deep");
    }

    #[test]
    fn test_build_assessment_prompt_includes_spec_fields() {
        let spec = make_spec("Build a complex multi-service feature");
        let prompt = build_assessment_prompt(&spec, "Complex Feature");

        assert!(prompt.contains("# Complexity Assessment: Complex Feature"));
        assert!(prompt.contains("Build a complex multi-service feature"));
        assert!(prompt.contains("Req A"));
        assert!(prompt.contains("AC 1"));
        assert!(prompt.contains("Constraint 1"));
        assert!(prompt.contains("Use existing patterns"));
        assert!(prompt.contains("complexity_score"));
        assert!(prompt.contains("single_verifier"));
        assert!(prompt.contains("vs_shallow"));
        assert!(prompt.contains("vs_deep"));
    }

    #[test]
    fn test_build_assessment_prompt_empty_spec() {
        let spec = SpecSheet {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            overview: None,
            requirements: None,
            acceptance_criteria: None,
            constraints: None,
            tech_notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let prompt = build_assessment_prompt(&spec, "Minimal Task");
        assert!(prompt.contains("# Complexity Assessment: Minimal Task"));
        assert!(prompt.contains("complexity_score"));
    }
}
