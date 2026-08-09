-- Immutable quest-completion reward provenance.
--
-- Hunting costs and loot already stamp the exact session context in force
-- when they are recorded. Quest completions previously retained only the
-- session, quest, and completion instant, then analytics re-read the quest's
-- mutable reward configuration. These nullable additions preserve the exact
-- declared activity context and the reward fact recognised at completion.
-- NULL `reward_source` means the completion predates this capture model;
-- `none` is the explicit new-model fact that no separate reward was recorded.

ALTER TABLE session_quest_completions
    ADD COLUMN activity_context_id INTEGER REFERENCES session_contexts(id) ON DELETE SET NULL;
ALTER TABLE session_quest_completions
    ADD COLUMN activity_interval_id INTEGER REFERENCES session_intervals(id) ON DELETE SET NULL;
ALTER TABLE session_quest_completions
    ADD COLUMN reward_source TEXT CHECK (
        reward_source IS NULL OR reward_source IN ('none', 'tracked_loot', 'ledger', 'skill')
    );
ALTER TABLE session_quest_completions ADD COLUMN reward_ped REAL;
ALTER TABLE session_quest_completions ADD COLUMN expected_reward_markup_percent REAL;
ALTER TABLE session_quest_completions
    ADD COLUMN ledger_entry_id TEXT REFERENCES ledger_entries(id) ON DELETE SET NULL;
ALTER TABLE session_quest_completions
    ADD COLUMN quest_claim_id INTEGER REFERENCES quest_claims(id) ON DELETE SET NULL;

CREATE INDEX idx_sqc_activity_context
    ON session_quest_completions(activity_context_id)
    WHERE activity_context_id IS NOT NULL;

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
