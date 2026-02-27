-- Swarm core enhancements: git branch isolation, hybrid model selection, sub-swarm tracking

-- Each agent generation gets its own git branch for dead-end filtering (§4.5, §7.4)
ALTER TABLE swarm_agents ADD COLUMN git_branch TEXT DEFAULT NULL;

-- Hybrid model selection: cheaper verifier model (§10.4)
ALTER TABLE swarms ADD COLUMN verifier_model TEXT DEFAULT NULL;

-- Track succession count per subtask lineage for redecomposition heuristic
ALTER TABLE swarm_agents ADD COLUMN succession_count INTEGER NOT NULL DEFAULT 0;
