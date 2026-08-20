-- Canonical quest-reward materialisation and provenance.
--
-- Universal Ammo is PED-equivalent operating value: it is recognised in the
-- ledger and never becomes stock. Every other confirmed reward item is a
-- stock acquisition whose quest/run/context provenance survives subsequent
-- movements. Historical playlist tables intentionally remain untouched as
-- dormant, non-lossy storage.

ALTER TABLE session_quest_completion_reward_items
    ADD COLUMN accounting_kind TEXT NOT NULL DEFAULT 'stock'
        CHECK (accounting_kind IN ('stock', 'liquid'));
ALTER TABLE session_quest_completion_reward_items
    ADD COLUMN ledger_entry_id TEXT REFERENCES ledger_entries(id);

UPDATE session_quest_completion_reward_items
SET accounting_kind = 'liquid'
WHERE lower(trim(item_name)) = 'universal ammo';

-- Exact historical Universal Ammo evidence can be materialised without
-- inference. The deterministic id makes the backfill idempotent by identity.
INSERT OR IGNORE INTO ledger_entries(id, date, type, description, amount, tag)
SELECT
    'quest-ammo-' || ri.id,
    strftime('%Y-%m-%dT%H:%M:%SZ', c.completed_at, 'unixepoch'),
    'markup',
    'Quest: ' || q.name,
    ri.value_ped,
    'quest_reward'
FROM session_quest_completion_reward_items ri
JOIN session_quest_completions c ON c.id = ri.completion_id
JOIN quests q ON q.id = c.quest_id
WHERE ri.accounting_kind = 'liquid'
  AND ri.value_ped > 0
  AND COALESCE((
      SELECT r.outcome
      FROM quest_reward_reviews r
      WHERE r.completion_id = c.id
      ORDER BY r.reviewed_at DESC, r.id DESC
      LIMIT 1
  ), c.reward_outcome) = 'confirmed';

UPDATE session_quest_completion_reward_items
SET ledger_entry_id = 'quest-ammo-' || id
WHERE accounting_kind = 'liquid'
  AND value_ped > 0
  AND EXISTS (
      SELECT 1 FROM ledger_entries le WHERE le.id = 'quest-ammo-' || session_quest_completion_reward_items.id
  );

-- Immutable ownership of a completion reward across the exact activity
-- contexts traversed by its durable run. A completion with no measurable
-- context owns no rows here and remains globally/quest attributable only.
CREATE TABLE quest_reward_attributions (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    completion_id         INTEGER NOT NULL
        REFERENCES session_quest_completions(id) ON DELETE CASCADE,
    activity_context_id   INTEGER NOT NULL REFERENCES session_contexts(id),
    session_definition_id INTEGER,
    weight                REAL NOT NULL CHECK (weight > 0 AND weight <= 1),
    basis                 TEXT NOT NULL CHECK (basis IN ('cycled', 'duration')),
    cycled_ped            REAL NOT NULL DEFAULT 0 CHECK (cycled_ped >= 0),
    duration_seconds      REAL NOT NULL DEFAULT 0 CHECK (duration_seconds >= 0),
    UNIQUE(completion_id, activity_context_id)
);

CREATE INDEX idx_quest_reward_attributions_context
    ON quest_reward_attributions(activity_context_id);
CREATE INDEX idx_quest_reward_attributions_definition
    ON quest_reward_attributions(session_definition_id);

-- Preserve the strongest provenance older completions actually recorded.
INSERT INTO quest_reward_attributions(
    completion_id, activity_context_id, session_definition_id,
    weight, basis, cycled_ped, duration_seconds
)
SELECT c.id, c.activity_context_id, s.definition_id, 1, 'duration', 0, 0
FROM session_quest_completions c
LEFT JOIN tracking_sessions s ON s.id = c.session_id
WHERE c.activity_context_id IS NOT NULL;

-- Corrections append a reversal rather than deleting the reward fact. A
-- liquid reversal owns the compensating ledger row; stock reversals are
-- projected by excluding the acquisition once dependency checks pass.
CREATE TABLE quest_reward_reversals (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    completion_id         INTEGER NOT NULL UNIQUE
        REFERENCES session_quest_completions(id),
    reversed_at           REAL NOT NULL,
    liquid_ledger_entry_id TEXT REFERENCES ledger_entries(id),
    pes_reversal_claim_id  INTEGER REFERENCES quest_claims(id)
);

-- Cooldown correction is separate from economic correction. The completion
-- and its run remain historical facts; availability reads ignore only the
-- completion explicitly disavowed here.
CREATE TABLE quest_cooldown_resets (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    quest_id      INTEGER NOT NULL REFERENCES quests(id),
    completion_id INTEGER NOT NULL UNIQUE REFERENCES session_quest_completions(id),
    reset_at      REAL NOT NULL
);

CREATE INDEX idx_quest_cooldown_resets_quest
    ON quest_cooldown_resets(quest_id, reset_at DESC);

-- Quest provenance extends the existing signed movement vocabulary. The
-- table rebuild is required only to widen its CHECK and add the immutable
-- reward/run/context keys; all existing rows are copied byte-for-byte.
PRAGMA foreign_keys = OFF;

CREATE TABLE stock_movements_new (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    item_name             TEXT NOT NULL,
    movement_kind         TEXT NOT NULL CHECK (movement_kind IN (
                                'listing', 'listing_return',
                                'conversion_out', 'conversion_in',
                                'trade', 'removal',
                                'opening_balance', 'legacy_adjustment')),
    ref_id                TEXT,
    source_kind           TEXT NOT NULL CHECK (source_kind IN (
                                'harvest', 'hunt', 'quest',
                                'conversion', 'unattributed')),
    source_event_id       TEXT,
    yield_tier            TEXT CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown')
                                      OR yield_tier IS NULL),
    mob_species           TEXT,
    quantity              REAL NOT NULL,
    tt_value              REAL NOT NULL,
    occurred_at           TEXT NOT NULL,
    created_at            REAL NOT NULL,
    tool_name             TEXT,
    session_definition_id INTEGER,
    quest_reward_item_id  INTEGER REFERENCES session_quest_completion_reward_items(id),
    quest_run_id          INTEGER REFERENCES quest_runs(id),
    quest_id              INTEGER REFERENCES quests(id),
    activity_context_id   INTEGER REFERENCES session_contexts(id)
);

INSERT INTO stock_movements_new (
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, mob_species, quantity, tt_value, occurred_at, created_at,
    tool_name, session_definition_id
)
SELECT
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, mob_species, quantity, tt_value, occurred_at, created_at,
    tool_name, session_definition_id
FROM stock_movements;

DROP TABLE stock_movements;
ALTER TABLE stock_movements_new RENAME TO stock_movements;

CREATE INDEX idx_stock_movements_item
    ON stock_movements(item_name, yield_tier, tool_name);
CREATE INDEX idx_stock_movements_species
    ON stock_movements(item_name, mob_species);
CREATE INDEX idx_stock_movements_ref ON stock_movements(ref_id);
CREATE INDEX idx_stock_movements_hunting_definition
    ON stock_movements(item_name, mob_species, session_definition_id);
CREATE INDEX idx_stock_movements_quest
    ON stock_movements(quest_id, quest_run_id, activity_context_id);
CREATE INDEX idx_stock_movements_reward_item
    ON stock_movements(quest_reward_item_id);

PRAGMA foreign_keys = ON;

-- Quest stock TT is a realised return family of its own. Universal Ammo is
-- absent because its full value already enters through the ordinary ledger.
ALTER TABLE daily_rollups ADD COLUMN quest_item_tt REAL;
