use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Represents an open question extracted from the planning/assessment phase
/// that should be answered before proceeding (section 6.3.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InterviewQuestion {
    pub question: String,
    pub context: String,
    pub priority: InterviewPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InterviewPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// A completed question-answer pair from the interview phase.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InterviewAnswer {
    pub question: String,
    pub answer: String,
}

/// Represents the full interview phase state for a task (section 6.3.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct InterviewPhase {
    pub open_questions: Vec<InterviewQuestion>,
    pub answers: Vec<InterviewAnswer>,
    pub status: InterviewStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InterviewStatus {
    Pending,
    AwaitingAnswers,
    Completed,
    Skipped,
}

impl InterviewPhase {
    pub fn new() -> Self {
        Self {
            open_questions: vec![],
            answers: vec![],
            status: InterviewStatus::Pending,
        }
    }

    pub fn with_questions(questions: Vec<InterviewQuestion>) -> Self {
        let status = if questions.is_empty() {
            InterviewStatus::Skipped
        } else {
            InterviewStatus::AwaitingAnswers
        };
        Self {
            open_questions: questions,
            answers: vec![],
            status,
        }
    }

    pub fn add_answer(&mut self, question: String, answer: String) {
        self.answers.push(InterviewAnswer { question, answer });
        // Check if all questions answered
        if self.answers.len() >= self.open_questions.len() {
            self.status = InterviewStatus::Completed;
        }
    }

    pub fn is_complete(&self) -> bool {
        self.status == InterviewStatus::Completed || self.status == InterviewStatus::Skipped
    }

    pub fn unanswered_questions(&self) -> Vec<&InterviewQuestion> {
        let answered: std::collections::HashSet<&str> =
            self.answers.iter().map(|a| a.question.as_str()).collect();
        self.open_questions
            .iter()
            .filter(|q| !answered.contains(q.question.as_str()))
            .collect()
    }
}

/// Extract open questions from plan text by looking for question patterns.
/// Returns questions found in the plan that indicate areas needing clarification.
pub fn extract_open_questions(plan_text: &str) -> Vec<InterviewQuestion> {
    let mut questions = Vec::new();

    for line in plan_text.lines() {
        let trimmed = line.trim();

        // Check for TODO/TBD/CLARIFY markers first (takes priority)
        let lower = trimmed.to_lowercase();
        let is_marker = (lower.starts_with("todo:")
            || lower.starts_with("tbd:")
            || lower.starts_with("clarify:"))
            && trimmed.len() > 5;

        if is_marker {
            questions.push(InterviewQuestion {
                question: trimmed.to_string(),
                context: extract_surrounding_context(plan_text, trimmed),
                priority: InterviewPriority::High,
            });
        } else if trimmed.ends_with('?')
            && !trimmed.starts_with("//")
            && !trimmed.starts_with('#')
            && trimmed.len() > 10
        {
            // Lines that end with ? (but weren't already matched as markers)
            let priority = if lower.contains("critical")
                || lower.contains("must")
                || lower.contains("block")
            {
                InterviewPriority::Critical
            } else if lower.contains("important") || lower.contains("should") {
                InterviewPriority::High
            } else {
                InterviewPriority::Medium
            };

            questions.push(InterviewQuestion {
                question: trimmed.to_string(),
                context: extract_surrounding_context(plan_text, trimmed),
                priority,
            });
        }
    }

    questions
}

/// Extract a few lines of context around a target line in the text.
fn extract_surrounding_context(text: &str, target: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == target {
            let start = i.saturating_sub(2);
            let end = (i + 3).min(lines.len());
            return lines[start..end].join("\n");
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interview_phase_new() {
        let phase = InterviewPhase::new();
        assert_eq!(phase.status, InterviewStatus::Pending);
        assert!(phase.open_questions.is_empty());
        assert!(phase.answers.is_empty());
    }

    #[test]
    fn test_interview_phase_with_questions() {
        let questions = vec![InterviewQuestion {
            question: "What auth provider?".to_string(),
            context: "Auth section".to_string(),
            priority: InterviewPriority::High,
        }];
        let phase = InterviewPhase::with_questions(questions);
        assert_eq!(phase.status, InterviewStatus::AwaitingAnswers);
        assert_eq!(phase.open_questions.len(), 1);
    }

    #[test]
    fn test_interview_phase_empty_questions_skipped() {
        let phase = InterviewPhase::with_questions(vec![]);
        assert_eq!(phase.status, InterviewStatus::Skipped);
        assert!(phase.is_complete());
    }

    #[test]
    fn test_interview_phase_add_answer_completes() {
        let questions = vec![InterviewQuestion {
            question: "What auth provider?".to_string(),
            context: "".to_string(),
            priority: InterviewPriority::High,
        }];
        let mut phase = InterviewPhase::with_questions(questions);
        assert!(!phase.is_complete());

        phase.add_answer("What auth provider?".to_string(), "OAuth2".to_string());
        assert!(phase.is_complete());
        assert_eq!(phase.status, InterviewStatus::Completed);
    }

    #[test]
    fn test_unanswered_questions() {
        let questions = vec![
            InterviewQuestion {
                question: "Q1?".to_string(),
                context: "".to_string(),
                priority: InterviewPriority::High,
            },
            InterviewQuestion {
                question: "Q2?".to_string(),
                context: "".to_string(),
                priority: InterviewPriority::Medium,
            },
        ];
        let mut phase = InterviewPhase::with_questions(questions);
        phase.add_answer("Q1?".to_string(), "A1".to_string());

        let unanswered = phase.unanswered_questions();
        assert_eq!(unanswered.len(), 1);
        assert_eq!(unanswered[0].question, "Q2?");
    }

    #[test]
    fn test_extract_open_questions_finds_questions() {
        let plan = "## Step 1\nSet up the database schema.\n\nShould we use PostgreSQL or SQLite for this feature?\n\n## Step 2\nImplement the API endpoints.";
        let questions = extract_open_questions(plan);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].question.contains("PostgreSQL or SQLite"));
        assert_eq!(questions[0].priority, InterviewPriority::High); // contains "should"
    }

    #[test]
    fn test_extract_open_questions_finds_todo_markers() {
        let plan = "## Step 1\nTODO: Determine the correct API endpoint structure\n\n## Step 2\nCLARIFY: What error codes should be returned?";
        let questions = extract_open_questions(plan);
        assert_eq!(questions.len(), 2);
    }

    #[test]
    fn test_extract_open_questions_ignores_comments_and_headers() {
        let plan =
            "# What is this?\n// What about this?\nThis is a real question that needs an answer?";
        let questions = extract_open_questions(plan);
        assert_eq!(questions.len(), 1);
        assert!(questions[0].question.contains("real question"));
    }

    #[test]
    fn test_extract_open_questions_ignores_short_questions() {
        let plan = "Really?\nIs this a longer question that actually matters?";
        let questions = extract_open_questions(plan);
        assert_eq!(questions.len(), 1);
    }

    #[test]
    fn test_extract_open_questions_critical_priority() {
        let plan = "Is this a critical blocking issue that must be resolved?";
        let questions = extract_open_questions(plan);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].priority, InterviewPriority::Critical);
    }
}
