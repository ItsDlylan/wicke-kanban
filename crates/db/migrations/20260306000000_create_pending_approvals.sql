CREATE TABLE pending_approvals (
    id                   TEXT PRIMARY KEY,
    execution_process_id BLOB NOT NULL,
    task_id              BLOB NOT NULL,
    tool_name            TEXT NOT NULL,
    tool_input           TEXT NOT NULL,
    tool_call_id         TEXT,
    status               TEXT NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending','approved','denied','timed_out','cancelled')),
    response_input       TEXT,
    created_at           TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    timeout_at           TEXT NOT NULL,
    responded_at         TEXT,
    FOREIGN KEY (execution_process_id) REFERENCES execution_processes(id) ON DELETE CASCADE,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
CREATE INDEX idx_pending_approvals_task_id ON pending_approvals(task_id);
CREATE INDEX idx_pending_approvals_status ON pending_approvals(status);
