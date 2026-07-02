-- Analytical read-path indexes.
--
-- Every named index in the baseline schema is write-side (per-session foreign
-- keys) or per-scan; the time-window predicates the analytics and session
-- reads filter on carried none, so those reads scanned the base tables in full.
-- These three cover the timestamp columns the read paths window on. Added
-- forward-only over the existing schema; a large existing database builds them
-- once inside its normal migration on the next launch.

CREATE INDEX idx_kills_timestamp ON kills(timestamp);
CREATE INDEX idx_codex_claims_claimed_at ON codex_claims(claimed_at);
CREATE INDEX idx_tracking_sessions_started_at ON tracking_sessions(started_at);

-- Covering index for the kill_tool_stats -> kills weapon-cost aggregate. The
-- UNIQUE(kill_id, tool_name, cost_per_shot) autoindex is one column short of
-- covering: reading shots_fired forces a table-row fetch per matched row, the
-- single largest per-statement cost on a large database. Carrying cost_per_shot,
-- shots_fired and tool_name after kill_id lets SUM(cost_per_shot * shots_fired)
-- resolve from the index alone.
CREATE INDEX idx_kill_tool_stats_covering
    ON kill_tool_stats(kill_id, cost_per_shot, shots_fired, tool_name);

-- Build planner statistics. The application had never run ANALYZE, so the
-- planner chose plans without knowing index selectivity; run it once here now
-- that the analytical indexes exist. PRAGMA optimize on shutdown keeps the
-- statistics current thereafter.
ANALYZE;
