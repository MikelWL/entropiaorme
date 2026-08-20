-- Manual-hand-in quests: a canonical three-way completion mode, stable raw
-- loot-clump identity, and durable waiting state for the overlay review flow.

-- `completion_trigger` was introduced with a two-value CHECK constraint.
-- Keep that applied column immutable and move new reads and writes to this
-- additive canonical column instead of rebuilding a referenced table.
ALTER TABLE quests ADD COLUMN completion_mode TEXT NOT NULL DEFAULT 'mission_log'
    CHECK (completion_mode IN ('mission_log', 'signal_item', 'manual_hand_in'));

UPDATE quests
SET completion_mode = completion_trigger;

ALTER TABLE kills ADD COLUMN loot_source_id TEXT;
ALTER TABLE harvest_events ADD COLUMN loot_source_id TEXT;

CREATE UNIQUE INDEX idx_kills_loot_source
    ON kills(loot_source_id) WHERE loot_source_id IS NOT NULL;
CREATE UNIQUE INDEX idx_harvest_loot_source
    ON harvest_events(loot_source_id) WHERE loot_source_id IS NOT NULL;

CREATE TABLE quest_reward_clumps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL UNIQUE,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('kill', 'harvest')),
    source_record_id TEXT NOT NULL,
    context_id INTEGER NOT NULL REFERENCES session_contexts(id),
    observed_at REAL NOT NULL,
    claimed_completion_id INTEGER UNIQUE REFERENCES session_quest_completions(id)
);

CREATE INDEX idx_quest_reward_clumps_session
    ON quest_reward_clumps(session_id, id);
CREATE INDEX idx_quest_reward_clumps_context
    ON quest_reward_clumps(context_id, id);

CREATE TABLE quest_reward_clump_items (
    clump_id INTEGER NOT NULL REFERENCES quest_reward_clumps(id) ON DELETE CASCADE,
    line_index INTEGER NOT NULL,
    item_name TEXT NOT NULL CHECK (trim(item_name) != ''),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    value_ped REAL NOT NULL CHECK (value_ped >= 0),
    PRIMARY KEY (clump_id, line_index)
);

ALTER TABLE quest_runs
    ADD COLUMN hand_in_after_clump_id INTEGER REFERENCES quest_reward_clumps(id);
ALTER TABLE quest_runs
    ADD COLUMN hand_in_waiting INTEGER NOT NULL DEFAULT 0 CHECK (hand_in_waiting IN (0, 1));

-- One overlay can wait for one manual hand-in at a time. Switching the
-- waiting quest is an explicit transaction in the service.
CREATE UNIQUE INDEX idx_quest_runs_one_hand_in_wait
    ON quest_runs(hand_in_waiting) WHERE hand_in_waiting = 1;
