-- Task dependencies for story decomposition ordering
CREATE TABLE task_dependencies (
    task_id     BLOB NOT NULL,
    depends_on  BLOB NOT NULL,
    PRIMARY KEY (task_id, depends_on),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (depends_on) REFERENCES tasks(id) ON DELETE CASCADE
);
CREATE INDEX idx_task_deps_task ON task_dependencies(task_id);
CREATE INDEX idx_task_deps_dep ON task_dependencies(depends_on);

-- Sort order for child task execution ordering
ALTER TABLE tasks ADD COLUMN sort_order INTEGER DEFAULT 0;
