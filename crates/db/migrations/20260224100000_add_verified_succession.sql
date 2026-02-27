-- Verified Succession Swarm tables

CREATE TABLE swarms (
    id              BLOB PRIMARY KEY NOT NULL,
    task_id         BLOB NOT NULL REFERENCES tasks(id),
    workspace_id    BLOB NOT NULL REFERENCES workspaces(id),
    parent_agent_id BLOB REFERENCES swarm_agents(id),
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK(status IN ('pending','running','completed','failed','cancelled')),
    depth           INTEGER NOT NULL DEFAULT 0,
    max_depth       INTEGER NOT NULL DEFAULT 3,
    routing_decision TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now','subsec')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now','subsec'))
);

CREATE TABLE swarm_agents (
    id                      BLOB PRIMARY KEY NOT NULL,
    swarm_id                BLOB NOT NULL REFERENCES swarms(id) ON DELETE CASCADE,
    execution_process_id    BLOB REFERENCES execution_processes(id),
    subtask_description     TEXT NOT NULL,
    generation              INTEGER NOT NULL DEFAULT 1,
    predecessor_id          BLOB REFERENCES swarm_agents(id),
    status                  TEXT NOT NULL DEFAULT 'pending'
                            CHECK(status IN ('pending','running','completed','failed','threshold')),
    context_tokens_used     INTEGER DEFAULT 0,
    context_window_size     INTEGER DEFAULT 200000,
    context_threshold       REAL NOT NULL DEFAULT 0.6,
    sort_order              INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL DEFAULT (datetime('now','subsec')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now','subsec'))
);

CREATE TABLE swarm_successions (
    id                          BLOB PRIMARY KEY NOT NULL,
    swarm_id                    BLOB NOT NULL REFERENCES swarms(id) ON DELETE CASCADE,
    predecessor_id              BLOB NOT NULL REFERENCES swarm_agents(id),
    verifier_execution_id       BLOB REFERENCES execution_processes(id),
    successor_id                BLOB REFERENCES swarm_agents(id),
    predecessor_self_assessment TEXT,
    verification_report         TEXT,
    verifier_confidence         REAL,
    recovery_strategy           TEXT DEFAULT 'corrective'
                                CHECK(recovery_strategy IN ('corrective','clean_restart','redecomposition','escalation')),
    status                      TEXT NOT NULL DEFAULT 'pending'
                                CHECK(status IN ('pending','verifying','verified','successor_running','failed')),
    created_at                  TEXT NOT NULL DEFAULT (datetime('now','subsec')),
    updated_at                  TEXT NOT NULL DEFAULT (datetime('now','subsec'))
);

CREATE TABLE swarm_agent_dependencies (
    agent_id            BLOB NOT NULL REFERENCES swarm_agents(id) ON DELETE CASCADE,
    depends_on_agent_id BLOB NOT NULL REFERENCES swarm_agents(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, depends_on_agent_id)
);

CREATE INDEX idx_swarms_task ON swarms(task_id);
CREATE INDEX idx_swarm_agents_swarm ON swarm_agents(swarm_id);
CREATE INDEX idx_swarm_agents_exec ON swarm_agents(execution_process_id);
CREATE INDEX idx_swarm_successions_swarm ON swarm_successions(swarm_id);

-- Additional columns for routing pipeline (Phase 5)
ALTER TABLE tasks ADD COLUMN routing_decision TEXT DEFAULT NULL;
ALTER TABLE spec_sheets ADD COLUMN complexity_score INTEGER DEFAULT NULL;
