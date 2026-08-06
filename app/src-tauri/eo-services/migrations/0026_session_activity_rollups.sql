-- Materialised per-session activity rollups: the read model behind the
-- Hunting analytics and stock surfaces.
--
-- The activity reads aggregate at grains no finer than these tables hold
-- (kill cells by context, species, and maturity; loot cells by species and
-- item; skill-gain cells by context), yet the raw tables they fold grow
-- with every recorded event, so reading raw put the whole play history
-- behind every tab open. A session is the natural settlement unit: once it
-- ends its events are immutable outside a handful of known edit paths, so
-- each ended session's cells are computed once and re-read forever.
--
-- `session_rollup_meta` is the settlement marker. A session present at the
-- current rollup version is served from its rollup rows; anything else
-- (the live session, a freshly edited session, a version left behind by a
-- meaning change) is served from the raw tables, so a reader is correct
-- regardless of heal timing. Edit transactions recompute the session in
-- the same commit; healing on read is the backfill and crash repair.

CREATE TABLE session_kill_rollups (
    session_id   TEXT NOT NULL,
    context_id   INTEGER,
    mob_species  TEXT NOT NULL,
    mob_maturity TEXT NOT NULL,
    kills        INTEGER NOT NULL,
    cycled_ped   REAL NOT NULL,
    loot_tt      REAL NOT NULL
);

CREATE INDEX idx_session_kill_rollups_session
    ON session_kill_rollups (session_id);

CREATE TABLE session_loot_rollups (
    session_id           TEXT NOT NULL,
    mob_species          TEXT NOT NULL,
    is_enhancer_shrapnel INTEGER NOT NULL,
    item_name            TEXT NOT NULL,
    quantity             INTEGER NOT NULL,
    value_ped            REAL NOT NULL
);

CREATE INDEX idx_session_loot_rollups_session
    ON session_loot_rollups (session_id);

CREATE INDEX idx_session_loot_rollups_item
    ON session_loot_rollups (item_name);

CREATE TABLE session_pes_rollups (
    session_id TEXT NOT NULL,
    context_id INTEGER,
    pes        REAL NOT NULL
);

CREATE INDEX idx_session_pes_rollups_session
    ON session_pes_rollups (session_id);

CREATE TABLE session_rollup_meta (
    session_id     TEXT PRIMARY KEY,
    rollup_version INTEGER NOT NULL
);
