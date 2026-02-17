-- Add parent_task_id to track decomposition parentage (separate from workspace execution)
ALTER TABLE tasks ADD COLUMN parent_task_id BLOB REFERENCES tasks(id) ON DELETE SET NULL;
CREATE INDEX idx_tasks_parent_task_id ON tasks(parent_task_id);
