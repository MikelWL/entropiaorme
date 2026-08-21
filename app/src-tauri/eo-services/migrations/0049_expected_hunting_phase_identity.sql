-- Preserve distinct offensive evidence phases even when their display name and
-- raw per-shot cost happen to match. Historical expected-return calculations
-- must retain the exact loadout basis captured for each phase.

CREATE TABLE kill_tool_stats_v0049 (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    kill_id                 TEXT NOT NULL REFERENCES kills(id),
    tool_name               TEXT NOT NULL,
    shots_fired             INTEGER DEFAULT 0,
    damage_dealt            REAL DEFAULT 0,
    critical_hits           INTEGER DEFAULT 0,
    cost_per_shot           REAL DEFAULT 0,
    expected_economics_json TEXT,
    evidence_fingerprint    TEXT NOT NULL DEFAULT '',
    UNIQUE(kill_id, tool_name, cost_per_shot, evidence_fingerprint)
);

INSERT INTO kill_tool_stats_v0049 (
    id,
    kill_id,
    tool_name,
    shots_fired,
    damage_dealt,
    critical_hits,
    cost_per_shot,
    expected_economics_json,
    evidence_fingerprint
)
SELECT
    id,
    kill_id,
    tool_name,
    shots_fired,
    damage_dealt,
    critical_hits,
    cost_per_shot,
    expected_economics_json,
    COALESCE(expected_economics_json, '')
FROM kill_tool_stats;

DROP TABLE kill_tool_stats;
ALTER TABLE kill_tool_stats_v0049 RENAME TO kill_tool_stats;

CREATE INDEX idx_kill_tool_stats_covering
    ON kill_tool_stats(kill_id, cost_per_shot, shots_fired, tool_name);
