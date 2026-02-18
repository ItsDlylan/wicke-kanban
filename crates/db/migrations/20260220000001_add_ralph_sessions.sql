-- Ralph sessions for multi-ticket automated execution
CREATE TABLE ralph_sessions (
    id            BLOB PRIMARY KEY,
    project_id    BLOB NOT NULL,
    workspace_id  BLOB,
    status        TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending','running','completed','failed')),
    created_at    TEXT NOT NULL DEFAULT (datetime('now','subsec')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now','subsec')),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL
);

CREATE TABLE ralph_session_tasks (
    ralph_session_id  BLOB NOT NULL,
    task_id           BLOB NOT NULL,
    execution_order   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (ralph_session_id, task_id),
    FOREIGN KEY (ralph_session_id) REFERENCES ralph_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX idx_ralph_session_tasks_session ON ralph_session_tasks(ralph_session_id);
CREATE INDEX idx_ralph_session_tasks_task ON ralph_session_tasks(task_id);
