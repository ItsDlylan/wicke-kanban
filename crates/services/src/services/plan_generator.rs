use db::models::spec_sheet::SpecSheet;

/// Generates a structured plan prompt from a spec sheet for use with Claude Code's plan mode.
pub fn generate_plan_prompt(spec: &SpecSheet, task_title: &str) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Plan Request: {}\n\n", task_title));
    prompt.push_str(
        "Please analyze the following specification and create a detailed implementation plan.\n\n",
    );

    if let Some(overview) = &spec.overview {
        prompt.push_str("## Overview\n");
        prompt.push_str(overview);
        prompt.push_str("\n\n");
    }

    if let Some(requirements) = &spec.requirements {
        if let Ok(items) = serde_json::from_str::<Vec<String>>(requirements) {
            if !items.is_empty() {
                prompt.push_str("## Requirements\n");
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
                prompt.push_str("## Acceptance Criteria\n");
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
                prompt.push_str("## Constraints\n");
                for item in &items {
                    prompt.push_str(&format!("- {}\n", item));
                }
                prompt.push('\n');
            }
        }
    }

    if let Some(tech_notes) = &spec.tech_notes {
        prompt.push_str("## Technical Notes\n");
        prompt.push_str(tech_notes);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Instructions\n");
    prompt.push_str("Create a step-by-step implementation plan that:\n");
    prompt.push_str("1. Identifies all files that need to be created or modified\n");
    prompt.push_str("2. Outlines the order of operations\n");
    prompt.push_str("3. Notes any potential risks or edge cases\n");
    prompt.push_str("4. Considers the acceptance criteria and constraints\n");

    prompt
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_generate_plan_prompt_full() {
        let spec = SpecSheet {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            overview: Some("Build a login page".to_string()),
            requirements: Some(r#"["Email input","Password input","Submit button"]"#.to_string()),
            acceptance_criteria: Some(r#"["User can log in"]"#.to_string()),
            constraints: Some(r#"["Must use existing auth service"]"#.to_string()),
            tech_notes: Some("Use React Hook Form".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let prompt = generate_plan_prompt(&spec, "Login Page");
        assert!(prompt.contains("# Plan Request: Login Page"));
        assert!(prompt.contains("Build a login page"));
        assert!(prompt.contains("- Email input"));
        assert!(prompt.contains("- User can log in"));
        assert!(prompt.contains("- Must use existing auth service"));
        assert!(prompt.contains("Use React Hook Form"));
    }

    #[test]
    fn test_generate_plan_prompt_minimal() {
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

        let prompt = generate_plan_prompt(&spec, "Empty Task");
        assert!(prompt.contains("# Plan Request: Empty Task"));
        assert!(prompt.contains("## Instructions"));
    }
}
