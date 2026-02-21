-- Add planning workflow: task_type column and planning_sessions table
-- task_type: 'task' (default, existing behavior) or 'epic' (planning board)
-- New statuses: idea, planning, specreview (for epic planning flow)

-- 1. Create new table with task_type column and expanded status CHECK
CREATE TABLE tasks_new (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'backlog'
                   CHECK (status IN ('backlog','plangenerating','ready','ralph','inprogress','qa','done','cancelled','idea','planning','specreview')),
    task_type   TEXT NOT NULL DEFAULT 'task'
                   CHECK (task_type IN ('task', 'epic')),
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

-- 2. Copy existing data (all existing tasks get task_type='task')
INSERT INTO tasks_new (id, project_id, title, description, status, task_type, parent_workspace_id, parent_task_id, sort_order, plan, plan_status, created_at, updated_at)
SELECT id, project_id, title, description, status, 'task', parent_workspace_id, parent_task_id, sort_order, plan, plan_status, created_at, updated_at
FROM tasks;

-- 3. Drop old table and rename
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- 4. Recreate indexes
CREATE INDEX idx_tasks_parent_task_id ON tasks(parent_task_id);
CREATE INDEX idx_tasks_project_type ON tasks(project_id, task_type);

-- 5. Create planning_sessions table
CREATE TABLE planning_sessions (
    id          BLOB PRIMARY KEY,
    task_id     BLOB NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_ref TEXT NOT NULL,
    notes       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec'))
);
CREATE INDEX idx_planning_sessions_task_id ON planning_sessions(task_id);
