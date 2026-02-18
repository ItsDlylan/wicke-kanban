-- Rework task status pipeline:
-- Old: backlog, todo, spec, plan, ralph, inreview, done, cancelled
-- New: backlog, plangenerating, ready, ralph, inprogress, qa, done, cancelled

-- 1. Create new table with updated CHECK constraint (includes all columns from previous migrations)
CREATE TABLE tasks_new (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'backlog'
                   CHECK (status IN ('backlog','plangenerating','ready','ralph','inprogress','qa','done','cancelled')),
    parent_workspace_id BLOB,
    parent_task_id BLOB REFERENCES tasks_new(id) ON DELETE SET NULL,
    sort_order  INTEGER DEFAULT 0,
    plan        TEXT,
    plan_status TEXT DEFAULT NULL
                   CHECK (plan_status IN ('pending', 'generating', 'completed', 'failed')),
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- 2. Copy existing data with status mapping
-- First pass: map statuses (plan_status='generating' tasks get 'plangenerating')
INSERT INTO tasks_new (id, project_id, title, description, status, parent_workspace_id, parent_task_id, sort_order, plan, plan_status, created_at, updated_at)
SELECT id, project_id, title, description,
       CASE
           WHEN plan_status = 'generating' THEN 'plangenerating'
           WHEN status IN ('todo', 'spec', 'plan') THEN 'ready'
           WHEN status = 'inreview' THEN 'qa'
           ELSE status
       END,
       parent_workspace_id, parent_task_id, sort_order, plan, plan_status, created_at, updated_at
FROM tasks;

-- 3. Drop old table and rename
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- 4. Recreate indexes
CREATE INDEX idx_tasks_parent_task_id ON tasks(parent_task_id);
