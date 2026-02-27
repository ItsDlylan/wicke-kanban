-- Add interview phase data column to tasks (section 6.3.2)
-- Stores serialized InterviewPhase JSON: questions, answers, status
ALTER TABLE tasks ADD COLUMN interview_data TEXT DEFAULT NULL;
