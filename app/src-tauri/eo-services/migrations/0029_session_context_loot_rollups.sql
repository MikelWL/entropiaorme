-- Activity-specific loot composition is a hot analytics read. Preserve the
-- context grain in the settled-session projection rather than re-scanning the
-- lifetime loot table whenever an activity is opened.
CREATE TABLE session_context_loot_rollups (
    session_id TEXT NOT NULL,
    context_id INTEGER,
    item_name TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    value_ped REAL NOT NULL
);

CREATE INDEX idx_session_context_loot_rollups_session
    ON session_context_loot_rollups(session_id);
