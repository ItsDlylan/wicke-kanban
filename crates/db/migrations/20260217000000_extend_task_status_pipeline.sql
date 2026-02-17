-- Extend task status pipeline: backlog, todo, spec, plan, ralph, inreview, done, cancelled
-- SQLite doesn't support ALTER CHECK constraints, so we recreate the table

-- 1. Create new table with updated CHECK constraint
CREATE TABLE tasks_new (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'backlog'
                   CHECK (status IN ('backlog','todo','spec','plan','ralph','inreview','done','cancelled')),
    parent_workspace_id BLOB,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- 2. Copy existing data, mapping 'inprogress' to 'todo' (no longer a valid status)
INSERT INTO tasks_new (id, project_id, title, description, status, parent_workspace_id, created_at, updated_at)
SELECT id, project_id, title, description,
       CASE status
           WHEN 'inprogress' THEN 'todo'
           ELSE status
       END,
       parent_workspace_id, created_at, updated_at
FROM tasks;

-- 3. Drop old table and rename
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;
