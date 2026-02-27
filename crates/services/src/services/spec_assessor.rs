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

/// Extended ranking report from a single ranking agent (section 6.3.4).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RankingReport {
    pub complexity_score: u8,
    pub complexity_rationale: String,
    pub files_estimated: u32,
    pub subsystems_touched: Vec<String>,
    pub estimated_context_requirement: f64,
    pub gaps_found: Vec<GapFinding>,
    pub risks_found: Vec<RiskFinding>,
    pub decomposability: String,
    pub recommended_architecture: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct GapFinding {
    pub description: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RiskFinding {
    pub description: String,
    pub mitigation: String,
}

/// Result of the synthesis agent (section 6.3.5).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SynthesisResult {
    pub routing_decision: String,
    pub confidence: f64,
    pub divergence_detected: bool,
    pub confirmed_gaps: Vec<GapFinding>,
    pub prioritized_risks: Vec<RiskFinding>,
    pub implementation_brief: String,
    pub median_complexity_score: u8,
    pub score_range: u8,
}

/// Outcome of the synthesis step — either a routing decision or a signal
/// that the spec needs more interview/clarification before assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SynthesisOutcome {
    Routed(SynthesisResult),
    NeedsInterview {
        ambiguous_sections: Vec<String>,
        recommended_questions: Vec<String>,
    },
}

/// The 3 ranking agent perspectives (section 6.3.4).
#[derive(Debug, Clone, Copy)]
pub enum RankingPerspective {
    Complexity,
    GapAnalysis,
    RiskIdentification,
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

/// Build a ranking agent prompt with a specific perspective focus (section 6.3.4).
pub fn build_ranking_prompt(
    spec: &SpecSheet,
    task_title: &str,
    perspective: RankingPerspective,
) -> String {
    let mut prompt = String::new();

    prompt.push_str(&format!("# Spec Ranking Assessment: {}\n\n", task_title));

    let perspective_instruction = match perspective {
        RankingPerspective::Complexity => {
            "You are a senior software architect focusing on **complexity assessment**. \
             Your primary goal is to accurately estimate the number of files, systems, \
             and dependencies involved. Pay close attention to cross-cutting concerns, \
             hidden dependencies between subsystems, and the true scope of changes required."
        }
        RankingPerspective::GapAnalysis => {
            "You are a senior software architect focusing on **gap analysis**. \
             Your primary goal is to identify missing requirements, ambiguities, \
             under-specified behaviors, and areas where the specification is incomplete. \
             Look for edge cases that aren't covered, error handling that isn't specified, \
             and integration points that lack detail."
        }
        RankingPerspective::RiskIdentification => {
            "You are a senior software architect focusing on **risk identification**. \
             Your primary goal is to identify untested areas, external dependencies, \
             performance bottlenecks, security concerns, and areas where implementation \
             could go wrong. For each risk, suggest a concrete mitigation strategy."
        }
    };

    prompt.push_str(perspective_instruction);
    prompt.push_str("\n\n");
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

    prompt.push_str("## Assessment Output\n\n");
    prompt.push_str("Evaluate the specification and produce a structured ranking report.\n\n");
    prompt.push_str("Respond with ONLY a JSON object in this exact format. No markdown fences, no explanatory text.\n\n");
    prompt.push_str(
        r#"{
  "complexity_score": 5,
  "complexity_rationale": "Brief explanation of the complexity rating",
  "files_estimated": 8,
  "subsystems_touched": ["database", "API"],
  "estimated_context_requirement": 0.4,
  "gaps_found": [{"description": "...", "severity": "high|medium|low"}],
  "risks_found": [{"description": "...", "mitigation": "..."}],
  "decomposability": "high",
  "recommended_architecture": "single|single_verifier|vs_shallow|vs_deep",
  "confidence": 0.85
}"#,
    );
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

/// Run 3 ranking agents concurrently, each with a different perspective (section 6.3.4).
/// Returns results from all 3 agents. If an agent fails, its error is included in the vec.
pub async fn run_concurrent_ranking(
    spec: &SpecSheet,
    task_title: &str,
    working_dir: &Path,
) -> Vec<Result<RankingReport, SpecAssessorError>> {
    let perspectives = [
        RankingPerspective::Complexity,
        RankingPerspective::GapAnalysis,
        RankingPerspective::RiskIdentification,
    ];

    let mut handles = Vec::with_capacity(3);
    for perspective in perspectives {
        let prompt = build_ranking_prompt(spec, task_title, perspective);
        let dir = working_dir.to_path_buf();
        handles.push(tokio::task::spawn_blocking(move || {
            run_assessment(&prompt, &dir).and_then(|output| parse_ranking_output(&output))
        }));
    }

    let mut results = Vec::with_capacity(3);
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => results.push(Err(SpecAssessorError::ExecutionFailed(format!(
                "Ranking agent panicked: {e}"
            )))),
        }
    }

    results
}

/// Parse the raw output from Claude into a RankingReport.
pub fn parse_ranking_output(output: &str) -> Result<RankingReport, SpecAssessorError> {
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
        utils::text::find_json_object_start(json_str).unwrap_or(json_str)
    };

    let result: RankingReport = serde_json::from_str(json_str)
        .map_err(|e| SpecAssessorError::ParseFailed(format!("{e}: input was: {json_str}")))?;

    Ok(result)
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
        utils::text::find_json_object_start(json_str).unwrap_or(json_str)
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

/// Route from a synthesis result, using the synthesized routing decision.
pub fn route_from_synthesis(synthesis: &SynthesisResult) -> String {
    synthesis.routing_decision.clone()
}

/// Maximum score divergence before flagging ambiguity (section 6.6).
const DIVERGENCE_THRESHOLD: u8 = 4;
/// Score agreement window for high confidence.
const CONSENSUS_THRESHOLD: u8 = 2;

/// Synthesize 3 ranking reports into a single routing decision (section 6.3.5).
/// Returns either a routed synthesis result or a signal that interview is needed.
pub fn run_synthesis(reports: &[RankingReport]) -> SynthesisOutcome {
    if reports.is_empty() {
        return SynthesisOutcome::Routed(SynthesisResult {
            routing_decision: "single".to_string(),
            confidence: 0.0,
            divergence_detected: false,
            confirmed_gaps: vec![],
            prioritized_risks: vec![],
            implementation_brief: "No ranking reports available.".to_string(),
            median_complexity_score: 1,
            score_range: 0,
        });
    }

    let scores: Vec<u8> = reports.iter().map(|r| r.complexity_score).collect();
    let min_score = *scores.iter().min().unwrap_or(&1);
    let max_score = *scores.iter().max().unwrap_or(&1);
    let score_range = max_score - min_score;

    // Compute median score
    let mut sorted_scores = scores.clone();
    sorted_scores.sort();
    let median_score = sorted_scores[sorted_scores.len() / 2];

    // Check for divergence (section 6.6 - Loop 2)
    if score_range > DIVERGENCE_THRESHOLD {
        // Identify which sections caused the disagreement
        let ambiguous_sections: Vec<String> = reports
            .iter()
            .flat_map(|r| {
                r.gaps_found
                    .iter()
                    .filter(|g| g.severity == "high")
                    .map(|g| g.description.clone())
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let recommended_questions: Vec<String> = ambiguous_sections
            .iter()
            .map(|s| format!("Can you clarify: {s}"))
            .collect();

        // If there are high-severity ambiguities, recommend interview
        if !ambiguous_sections.is_empty() {
            return SynthesisOutcome::NeedsInterview {
                ambiguous_sections,
                recommended_questions,
            };
        }
    }

    // Consensus check: scores within +/- CONSENSUS_THRESHOLD = high confidence
    let consensus = score_range <= CONSENSUS_THRESHOLD;

    // Confirm gaps: gaps found by 2+ agents
    let mut gap_counts: std::collections::HashMap<String, (usize, String)> =
        std::collections::HashMap::new();
    for report in reports {
        for gap in &report.gaps_found {
            let entry = gap_counts
                .entry(gap.description.clone())
                .or_insert((0, gap.severity.clone()));
            entry.0 += 1;
        }
    }
    let confirmed_gaps: Vec<GapFinding> = gap_counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= 2)
        .map(|(desc, (_, severity))| GapFinding {
            description: desc,
            severity,
        })
        .collect();

    // Aggregate risks: risks flagged by multiple agents get higher priority
    let mut risk_counts: std::collections::HashMap<String, (usize, String)> =
        std::collections::HashMap::new();
    for report in reports {
        for risk in &report.risks_found {
            let entry = risk_counts
                .entry(risk.description.clone())
                .or_insert((0, risk.mitigation.clone()));
            entry.0 += 1;
        }
    }
    let mut prioritized_risks: Vec<(usize, RiskFinding)> = risk_counts
        .into_iter()
        .map(|(desc, (count, mitigation))| {
            (
                count,
                RiskFinding {
                    description: desc,
                    mitigation,
                },
            )
        })
        .collect();
    prioritized_risks.sort_by(|a, b| b.0.cmp(&a.0));
    let prioritized_risks: Vec<RiskFinding> =
        prioritized_risks.into_iter().map(|(_, r)| r).collect();

    // Build routing decision from median score
    let routing_assessment = SpecAssessment {
        complexity_score: median_score,
        files_estimated: reports.iter().map(|r| r.files_estimated).sum::<u32>()
            / reports.len() as u32,
        subsystems: reports
            .iter()
            .flat_map(|r| r.subsystems_touched.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect(),
        context_estimate: reports
            .iter()
            .map(|r| r.estimated_context_requirement)
            .sum::<f64>()
            / reports.len() as f64,
        decomposability: reports
            .first()
            .map_or("medium".to_string(), |r| r.decomposability.clone()),
        recommended: String::new(),
        confidence: if consensus {
            reports.iter().map(|r| r.confidence).sum::<f64>() / reports.len() as f64
        } else {
            (reports.iter().map(|r| r.confidence).sum::<f64>() / reports.len() as f64) * 0.7
        },
    };

    let routing_decision = route_from_score(&routing_assessment);

    // Build implementation brief
    let mut brief =
        format!("Median complexity: {median_score}/10 (range: {min_score}-{max_score}). ");
    if consensus {
        brief.push_str("High consensus among ranking agents. ");
    } else {
        brief.push_str(&format!("Moderate divergence (range {}). ", score_range));
    }
    brief.push_str(&format!(
        "Routing: {}. {} confirmed gaps, {} risks identified.",
        routing_decision,
        confirmed_gaps.len(),
        prioritized_risks.len()
    ));

    SynthesisOutcome::Routed(SynthesisResult {
        routing_decision,
        confidence: routing_assessment.confidence,
        divergence_detected: score_range > DIVERGENCE_THRESHOLD,
        confirmed_gaps,
        prioritized_risks,
        implementation_brief: brief,
        median_complexity_score: median_score,
        score_range,
    })
}

/// Result of the full assessment pipeline, including interview recommendations
/// when ranking agents diverge significantly (section 6.6, Loop 2).
#[derive(Debug)]
pub struct FullAssessmentResult {
    pub assessment: SpecAssessment,
    pub synthesis: Option<SynthesisResult>,
    /// When ranking agents diverge beyond threshold, contains the ambiguous
    /// sections and recommended questions for the interview loop.
    pub needs_interview: Option<InterviewRecommendation>,
}

/// Recommendation to loop back to interview phase due to ranking divergence.
#[derive(Debug, Clone)]
pub struct InterviewRecommendation {
    pub ambiguous_sections: Vec<String>,
    pub recommended_questions: Vec<String>,
}

/// Run the full concurrent assessment pipeline: 3 ranking agents + synthesis.
/// Falls back to single assessment if all ranking agents fail.
pub async fn run_full_assessment(
    spec: &SpecSheet,
    task_title: &str,
    working_dir: &Path,
) -> Result<FullAssessmentResult, SpecAssessorError> {
    let ranking_results = run_concurrent_ranking(spec, task_title, working_dir).await;

    // Collect successful reports
    let reports: Vec<RankingReport> = ranking_results.into_iter().filter_map(|r| r.ok()).collect();

    if reports.is_empty() {
        // Fallback: single assessment call (original behavior)
        let prompt = build_assessment_prompt(spec, task_title);
        let dir = working_dir.to_path_buf();
        let raw_output = tokio::task::spawn_blocking(move || run_assessment(&prompt, &dir))
            .await
            .map_err(|e| {
                SpecAssessorError::ExecutionFailed(format!("Fallback assessment panicked: {e}"))
            })??;
        let assessment = parse_assessment_output(&raw_output)?;
        return Ok(FullAssessmentResult {
            assessment,
            synthesis: None,
            needs_interview: None,
        });
    }

    // Synthesize
    match run_synthesis(&reports) {
        SynthesisOutcome::Routed(synthesis) => {
            // Convert synthesis to a SpecAssessment for backward compatibility
            let assessment = SpecAssessment {
                complexity_score: synthesis.median_complexity_score,
                files_estimated: reports.iter().map(|r| r.files_estimated).sum::<u32>()
                    / reports.len() as u32,
                subsystems: reports
                    .iter()
                    .flat_map(|r| r.subsystems_touched.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect(),
                context_estimate: reports
                    .iter()
                    .map(|r| r.estimated_context_requirement)
                    .sum::<f64>()
                    / reports.len() as f64,
                decomposability: reports
                    .first()
                    .map_or("medium".to_string(), |r| r.decomposability.clone()),
                recommended: synthesis.routing_decision.clone(),
                confidence: synthesis.confidence,
            };
            Ok(FullAssessmentResult {
                assessment,
                synthesis: Some(synthesis),
                needs_interview: None,
            })
        }
        SynthesisOutcome::NeedsInterview {
            ambiguous_sections,
            recommended_questions,
        } => {
            // Even when interview is recommended, still produce a best-effort assessment
            // so the pipeline can continue. The caller uses `needs_interview` to decide
            // whether to loop back to the interview phase (section 6.6, Loop 2).
            let median_score = {
                let mut scores: Vec<u8> = reports.iter().map(|r| r.complexity_score).collect();
                scores.sort();
                scores[scores.len() / 2]
            };
            let assessment = SpecAssessment {
                complexity_score: median_score,
                files_estimated: reports.iter().map(|r| r.files_estimated).sum::<u32>()
                    / reports.len() as u32,
                subsystems: reports
                    .iter()
                    .flat_map(|r| r.subsystems_touched.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect(),
                context_estimate: reports
                    .iter()
                    .map(|r| r.estimated_context_requirement)
                    .sum::<f64>()
                    / reports.len() as f64,
                decomposability: "medium".to_string(),
                recommended: route_from_score(&SpecAssessment {
                    complexity_score: median_score,
                    files_estimated: 0,
                    subsystems: vec![],
                    context_estimate: reports
                        .iter()
                        .map(|r| r.estimated_context_requirement)
                        .sum::<f64>()
                        / reports.len() as f64,
                    decomposability: "medium".to_string(),
                    recommended: String::new(),
                    confidence: 0.0,
                }),
                confidence: 0.4, // low confidence due to divergence
            };

            tracing::warn!(
                "Ranking agents diverged significantly. Ambiguous sections: {:?}",
                ambiguous_sections
            );

            Ok(FullAssessmentResult {
                assessment,
                synthesis: None,
                needs_interview: Some(InterviewRecommendation {
                    ambiguous_sections,
                    recommended_questions,
                }),
            })
        }
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

    fn make_ranking_report(
        score: u8,
        gaps: Vec<GapFinding>,
        risks: Vec<RiskFinding>,
    ) -> RankingReport {
        RankingReport {
            complexity_score: score,
            complexity_rationale: format!("Score {} rationale", score),
            files_estimated: 10,
            subsystems_touched: vec!["api".to_string(), "db".to_string()],
            estimated_context_requirement: 0.5,
            gaps_found: gaps,
            risks_found: risks,
            decomposability: "high".to_string(),
            recommended_architecture: "single_verifier".to_string(),
            confidence: 0.8,
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
    fn test_parse_ranking_output_clean_json() {
        let output = r#"{ "complexity_score": 6, "complexity_rationale": "Multiple subsystems", "files_estimated": 12, "subsystems_touched": ["db", "api"], "estimated_context_requirement": 0.6, "gaps_found": [{"description": "Missing error handling", "severity": "high"}], "risks_found": [{"description": "External API dependency", "mitigation": "Add retry logic"}], "decomposability": "high", "recommended_architecture": "vs_shallow", "confidence": 0.8 }"#;
        let result = parse_ranking_output(output).unwrap();
        assert_eq!(result.complexity_score, 6);
        assert_eq!(result.gaps_found.len(), 1);
        assert_eq!(result.risks_found.len(), 1);
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

    #[test]
    fn test_build_ranking_prompt_complexity_perspective() {
        let spec = make_spec("Feature X");
        let prompt = build_ranking_prompt(&spec, "Feature X", RankingPerspective::Complexity);
        assert!(prompt.contains("complexity assessment"));
        assert!(prompt.contains("Feature X"));
        assert!(prompt.contains("complexity_rationale"));
    }

    #[test]
    fn test_build_ranking_prompt_gap_perspective() {
        let spec = make_spec("Feature X");
        let prompt = build_ranking_prompt(&spec, "Feature X", RankingPerspective::GapAnalysis);
        assert!(prompt.contains("gap analysis"));
    }

    #[test]
    fn test_build_ranking_prompt_risk_perspective() {
        let spec = make_spec("Feature X");
        let prompt =
            build_ranking_prompt(&spec, "Feature X", RankingPerspective::RiskIdentification);
        assert!(prompt.contains("risk identification"));
    }

    #[test]
    fn test_synthesis_consensus() {
        let reports = vec![
            make_ranking_report(5, vec![], vec![]),
            make_ranking_report(6, vec![], vec![]),
            make_ranking_report(5, vec![], vec![]),
        ];
        match run_synthesis(&reports) {
            SynthesisOutcome::Routed(result) => {
                assert!(!result.divergence_detected);
                assert!(result.confidence > 0.7);
                assert_eq!(result.median_complexity_score, 5);
                assert!(result.score_range <= CONSENSUS_THRESHOLD);
            }
            SynthesisOutcome::NeedsInterview { .. } => {
                panic!("Expected Routed, got NeedsInterview");
            }
        }
    }

    #[test]
    fn test_synthesis_divergence_with_ambiguity() {
        let gap = GapFinding {
            description: "Missing auth spec".to_string(),
            severity: "high".to_string(),
        };
        let reports = vec![
            make_ranking_report(3, vec![gap.clone()], vec![]),
            make_ranking_report(9, vec![gap.clone()], vec![]),
            make_ranking_report(4, vec![], vec![]),
        ];
        match run_synthesis(&reports) {
            SynthesisOutcome::NeedsInterview {
                ambiguous_sections, ..
            } => {
                assert!(!ambiguous_sections.is_empty());
            }
            SynthesisOutcome::Routed(result) => {
                // Also acceptable if no high-severity gaps shared
                assert!(result.divergence_detected);
            }
        }
    }

    #[test]
    fn test_synthesis_divergence_without_ambiguity() {
        // Big divergence but no high-severity gaps -> still routes
        let reports = vec![
            make_ranking_report(2, vec![], vec![]),
            make_ranking_report(9, vec![], vec![]),
            make_ranking_report(3, vec![], vec![]),
        ];
        match run_synthesis(&reports) {
            SynthesisOutcome::Routed(result) => {
                assert!(result.divergence_detected);
                // Confidence should be reduced
                assert!(result.confidence < 0.8);
            }
            SynthesisOutcome::NeedsInterview { .. } => {
                panic!("Expected Routed, got NeedsInterview (no ambiguous sections)");
            }
        }
    }

    #[test]
    fn test_synthesis_confirmed_gaps() {
        let gap = GapFinding {
            description: "Missing validation".to_string(),
            severity: "medium".to_string(),
        };
        let reports = vec![
            make_ranking_report(5, vec![gap.clone()], vec![]),
            make_ranking_report(6, vec![gap.clone()], vec![]),
            make_ranking_report(5, vec![], vec![]),
        ];
        match run_synthesis(&reports) {
            SynthesisOutcome::Routed(result) => {
                assert_eq!(result.confirmed_gaps.len(), 1);
                assert_eq!(result.confirmed_gaps[0].description, "Missing validation");
            }
            _ => panic!("Expected Routed"),
        }
    }

    #[test]
    fn test_synthesis_prioritized_risks() {
        let risk = RiskFinding {
            description: "External API failure".to_string(),
            mitigation: "Add circuit breaker".to_string(),
        };
        let reports = vec![
            make_ranking_report(5, vec![], vec![risk.clone()]),
            make_ranking_report(6, vec![], vec![risk.clone()]),
            make_ranking_report(5, vec![], vec![]),
        ];
        match run_synthesis(&reports) {
            SynthesisOutcome::Routed(result) => {
                assert!(!result.prioritized_risks.is_empty());
                assert_eq!(
                    result.prioritized_risks[0].description,
                    "External API failure"
                );
            }
            _ => panic!("Expected Routed"),
        }
    }

    #[test]
    fn test_synthesis_empty_reports() {
        match run_synthesis(&[]) {
            SynthesisOutcome::Routed(result) => {
                assert_eq!(result.confidence, 0.0);
                assert_eq!(result.routing_decision, "single");
            }
            _ => panic!("Expected Routed for empty input"),
        }
    }

    #[test]
    fn test_route_from_synthesis() {
        let synthesis = SynthesisResult {
            routing_decision: "vs_shallow".to_string(),
            confidence: 0.8,
            divergence_detected: false,
            confirmed_gaps: vec![],
            prioritized_risks: vec![],
            implementation_brief: "test".to_string(),
            median_complexity_score: 7,
            score_range: 1,
        };
        assert_eq!(route_from_synthesis(&synthesis), "vs_shallow");
    }
}
