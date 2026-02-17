-- Add spec_sheets table for task specifications
CREATE TABLE spec_sheets (
    id          BLOB PRIMARY KEY,
    task_id     BLOB NOT NULL UNIQUE,
    overview    TEXT,
    requirements TEXT,          -- JSON array of requirement strings
    acceptance_criteria TEXT,   -- JSON array
    constraints TEXT,           -- JSON array
    tech_notes  TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now', 'subsec')),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);
