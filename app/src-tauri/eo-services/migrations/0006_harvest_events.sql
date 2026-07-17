-- Harvesting (tree cutting) swing records: the second tracked
-- activity beside hunting. One row per swing, successful or failed;
-- a successful swing carries its wood loot as per-item rows. The tool
-- identity and per-swing cost are captured at swing time (immune to
-- later equipment edits); tool_name is NULL when no harvesting tool
-- was known and the swing recorded at zero cost.

CREATE TABLE harvest_events (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES tracking_sessions(id),
    timestamp       REAL NOT NULL,
    success         INTEGER NOT NULL DEFAULT 1,
    tool_name       TEXT,
    cost_ped        REAL DEFAULT 0,
    loot_total_ped  REAL DEFAULT 0
);

CREATE INDEX idx_harvest_session ON harvest_events(session_id);

-- deactivated_at mirrors kill_loot_items: room for the loot-edit
-- (deactivation) flow to extend to harvest loot without a schema move.
CREATE TABLE harvest_loot_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    harvest_id      TEXT NOT NULL REFERENCES harvest_events(id),
    item_name       TEXT NOT NULL,
    quantity        INTEGER DEFAULT 1,
    value_ped       REAL DEFAULT 0,
    deactivated_at  REAL
);

CREATE INDEX idx_harvest_loot_items_harvest ON harvest_loot_items(harvest_id);

-- Harvest columns on the materialised session summary (SUMMARY_VERSION 3):
-- harvest loot joins loot_tt and swing decay joins cycled_ped in the
-- computed values, with the explicit per-activity columns beside them.
ALTER TABLE session_summaries ADD COLUMN harvest_swings INTEGER DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN harvest_successes INTEGER DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN harvest_loot_tt REAL DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN harvest_cost REAL DEFAULT 0;

-- Harvest family columns on the daily rollup projection (ROLLUP_VERSION 2:
-- existing rows heal on the next read). NULL-preserving like the other
-- family columns.
ALTER TABLE daily_rollups ADD COLUMN harvest_loot_tt REAL;
ALTER TABLE daily_rollups ADD COLUMN harvest_cost REAL;
