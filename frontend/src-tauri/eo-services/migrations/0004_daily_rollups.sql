-- Daily analytics rollups: the materialised read model behind the Overview.
--
-- The Overview and its breakdowns aggregate the raw tracking tables from
-- scratch on every GET, which is O(total kills) and dominates the read cost on
-- a large database. These tables materialise the per-UTC-day sums of every
-- aggregate family those reads need, following the session_summaries
-- discipline (eager write at the mutation points, versioned lazy heal on
-- read). The read path then aggregates O(days) rollup rows plus a bounded raw
-- pass over the current (incomplete) day.
--
-- Rollups are a rebuildable projection over the raw tables as the source of
-- truth: dropping every row here and healing regenerates identical content.
-- They deliberately sit OUTSIDE the DB-state snapshot catalogue (like
-- session_summaries): parity surfaces through the analytics HTTP responses.
--
-- Column nullability is load-bearing. Each family column stores the raw
-- per-day SUM verbatim: NULL when the day had no contributing rows (or the
-- rows summed to NULL), so a window aggregate over rollups reproduces the
-- exact NULL-vs-zero engine typing the raw queries produce on the wire.
-- `has_rows` carries the one distinction NULL sums erase: whether any source
-- rows existed that day at all, which decides the day's membership in the
-- timeline/monthly point sets.

CREATE TABLE daily_rollups (
    day            TEXT PRIMARY KEY,          -- ISO 'YYYY-MM-DD', UTC
    rollup_version INTEGER NOT NULL,
    dirty          INTEGER NOT NULL DEFAULT 0,
    has_rows       INTEGER NOT NULL DEFAULT 0,
    loot_tt        REAL,                      -- SUM(kills.loot_total_ped)
    weapon_cost    REAL,                      -- SUM(cost_per_shot * shots_fired)
    enhancer_cost  REAL,                      -- SUM(kills.enhancer_cost)
    armour_cost    REAL,                      -- SUM(tracking_sessions.armour_cost)
    heal_cost      REAL,                      -- SUM(tracking_sessions.heal_cost)
    dangling_cost  REAL,                      -- SUM(tracking_sessions.dangling_cost)
    skill_tt       REAL,                      -- SUM(skill_gains.ped_value)
    codex_pes      REAL,                      -- SUM(codex_claims.ped_value)
    quest_pes      REAL,                      -- SUM(quest_claims.ped_value)
    computed_at    REAL NOT NULL DEFAULT (unixepoch('now'))
);

-- Per-day ledger sums by entry type and tag, normalised so the window totals
-- (GROUP BY tag), the per-day maps (GROUP BY day, tag) and the monthly merges
-- (GROUP BY month, tag) each stay one SQL pass. Amounts are stored unrounded;
-- rounding stays at response-build time, exactly as the raw reads round.
CREATE TABLE daily_ledger_rollups (
    day        TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    tag        TEXT NOT NULL,
    amount     REAL NOT NULL,
    PRIMARY KEY (day, entry_type, tag)
);

-- The ledger reads window and bucket on the ISO-date TEXT column; the heal
-- also sweeps it for entry dates outside the calendar walk. Both want the
-- index (the table was left unindexed while every read scanned it in full).
CREATE INDEX idx_ledger_entries_date ON ledger_entries(date);

-- The heal watermark: every day from the earliest data day up to and
-- including rolled_through has a daily_rollups row (empty days included,
-- keeping the range contiguous). Readers heal by advancing it to yesterday,
-- so the current day is never served from rollups and every day is
-- recomputed once after it completes.
CREATE TABLE daily_rollup_meta (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    rolled_through TEXT NOT NULL
);
