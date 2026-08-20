-- Protection costs settle recorded defensive evidence, not the session that
-- happened to own the recording prompt. This permits a user to defer a
-- burdensome terminal reading and allocate it across earlier sessions later.

ALTER TABLE protection_observations ADD COLUMN defence_event_cursor INTEGER;

CREATE TABLE protection_cost_windows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('limited_decay', 'repair')),
    set_id INTEGER REFERENCES protection_sets(id),
    armour_set_id INTEGER REFERENCES protection_sets(id),
    plate_set_id INTEGER REFERENCES protection_sets(id),
    opening_observation_id INTEGER REFERENCES protection_observations(id),
    closing_observation_id INTEGER REFERENCES protection_observations(id),
    consumed_tt_ped REAL,
    markup_percent REAL,
    cost_ped REAL NOT NULL CHECK (cost_ped >= 0),
    status TEXT NOT NULL CHECK (status IN ('booked', 'pending')),
    reason TEXT,
    client_token TEXT UNIQUE,
    created_at REAL NOT NULL,
    CHECK (
        (kind = 'limited_decay'
            AND set_id IS NOT NULL
            AND armour_set_id IS NULL
            AND plate_set_id IS NULL
            AND opening_observation_id IS NOT NULL
            AND closing_observation_id IS NOT NULL
            AND consumed_tt_ped IS NOT NULL
            AND markup_percent IS NOT NULL)
        OR
        (kind = 'repair'
            AND set_id IS NULL
            AND opening_observation_id IS NULL
            AND closing_observation_id IS NULL
            AND consumed_tt_ped IS NULL
            AND markup_percent IS NULL)
    )
);

CREATE INDEX idx_protection_cost_windows_created
    ON protection_cost_windows(created_at, id);
CREATE INDEX idx_protection_cost_windows_set
    ON protection_cost_windows(set_id, created_at, id);

CREATE TABLE protection_cost_allocations (
    window_id INTEGER NOT NULL REFERENCES protection_cost_windows(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    damage_weight REAL NOT NULL CHECK (damage_weight >= 0),
    deflection_count INTEGER NOT NULL CHECK (deflection_count >= 0),
    allocation_share REAL NOT NULL CHECK (allocation_share >= 0 AND allocation_share <= 1),
    cost_ped REAL NOT NULL CHECK (cost_ped >= 0),
    PRIMARY KEY (window_id, session_id)
);

CREATE TABLE protection_cost_context_allocations (
    window_id INTEGER NOT NULL REFERENCES protection_cost_windows(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    context_key INTEGER NOT NULL,
    context_id INTEGER REFERENCES session_contexts(id) ON DELETE SET NULL,
    damage_weight REAL NOT NULL CHECK (damage_weight >= 0),
    deflection_count INTEGER NOT NULL CHECK (deflection_count >= 0),
    allocation_share REAL NOT NULL CHECK (allocation_share >= 0 AND allocation_share <= 1),
    cost_ped REAL NOT NULL CHECK (cost_ped >= 0),
    PRIMARY KEY (window_id, session_id, context_key),
    CHECK ((context_key = -1 AND context_id IS NULL) OR context_key = context_id)
);

-- A defensive event can fund one cost for each physical layer. A NULL set is
-- the compatibility scope used by the legacy combined repair path.
CREATE TABLE protection_cost_evidence (
    window_id INTEGER NOT NULL REFERENCES protection_cost_windows(id) ON DELETE CASCADE,
    set_id INTEGER REFERENCES protection_sets(id),
    defence_event_id INTEGER NOT NULL REFERENCES protection_defence_events(id) ON DELETE CASCADE,
    PRIMARY KEY (window_id, set_id, defence_event_id)
);

CREATE UNIQUE INDEX idx_protection_cost_evidence_set
    ON protection_cost_evidence(set_id, defence_event_id) WHERE set_id IS NOT NULL;
CREATE UNIQUE INDEX idx_protection_cost_evidence_global
    ON protection_cost_evidence(defence_event_id) WHERE set_id IS NULL;

-- Databases that exercised the earlier single-session path retain that history
-- without re-booking its already-applied session cost. Existing observations
-- begin the new cursor model at the migration boundary, so old evidence cannot
-- be consumed again by a later measurement.
UPDATE protection_observations
SET defence_event_cursor = (SELECT COALESCE(MAX(id), 0) FROM protection_defence_events)
WHERE defence_event_cursor IS NULL;

INSERT INTO protection_cost_windows (
    kind, set_id, opening_observation_id, closing_observation_id,
    consumed_tt_ped, markup_percent, cost_ped, status, reason, created_at
)
SELECT
    'limited_decay', set_id, opening_observation_id, closing_observation_id,
    consumed_tt_ped, markup_percent, cost_ped, status, reason, created_at
FROM protection_reconciliations;

INSERT INTO protection_cost_allocations (
    window_id, session_id, damage_weight, deflection_count, allocation_share, cost_ped
)
SELECT window.id, legacy.session_id, 0, 0, 1, legacy.cost_ped
FROM protection_reconciliations legacy
JOIN protection_cost_windows window
  ON window.kind = 'limited_decay'
 AND window.closing_observation_id = legacy.closing_observation_id
WHERE legacy.status = 'booked';

INSERT INTO protection_cost_context_allocations (
    window_id, session_id, context_key, context_id,
    damage_weight, deflection_count, allocation_share, cost_ped
)
SELECT window.id, legacy.session_id, -1, NULL, 0, 0, 1, legacy.cost_ped
FROM protection_reconciliations legacy
JOIN protection_cost_windows window
  ON window.kind = 'limited_decay'
 AND window.closing_observation_id = legacy.closing_observation_id
WHERE legacy.status = 'booked';
