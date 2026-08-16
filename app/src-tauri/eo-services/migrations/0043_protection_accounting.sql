-- Protection catalogue, live identity, limited-item observations, and
-- defensive evidence. Armour and plates are separate economic layers;
-- loadouts compose them for one-action selection during play.

CREATE TABLE protection_sets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('armour', 'plates')),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    economy_kind TEXT NOT NULL CHECK (economy_kind IN ('limited', 'unlimited')),
    markup_percent REAL CHECK (
        (economy_kind = 'limited' AND markup_percent >= 100)
        OR (economy_kind = 'unlimited' AND markup_percent IS NULL)
    ),
    created_at REAL NOT NULL,
    archived_at REAL
);

CREATE UNIQUE INDEX idx_protection_sets_active_name
    ON protection_sets(kind, lower(name)) WHERE archived_at IS NULL;

CREATE TABLE protection_loadouts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    armour_set_id INTEGER REFERENCES protection_sets(id),
    plate_set_id INTEGER REFERENCES protection_sets(id),
    created_at REAL NOT NULL,
    archived_at REAL,
    CHECK (armour_set_id IS NOT NULL OR plate_set_id IS NOT NULL OR lower(name) = 'no protection')
);

CREATE UNIQUE INDEX idx_protection_loadouts_active_name
    ON protection_loadouts(lower(name)) WHERE archived_at IS NULL;

CREATE TABLE protection_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    active_loadout_id INTEGER REFERENCES protection_loadouts(id),
    updated_at REAL NOT NULL
);

CREATE TABLE protection_observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    set_id INTEGER NOT NULL REFERENCES protection_sets(id),
    client_token TEXT NOT NULL UNIQUE,
    tt_value_ped REAL NOT NULL CHECK (tt_value_ped >= 0),
    source TEXT NOT NULL CHECK (source IN ('ocr', 'manual')),
    raw_text TEXT,
    observed_at REAL NOT NULL,
    reset_reason TEXT
);

CREATE INDEX idx_protection_observations_set
    ON protection_observations(set_id, observed_at, id);

CREATE TABLE protection_reconciliations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    set_id INTEGER NOT NULL REFERENCES protection_sets(id),
    opening_observation_id INTEGER NOT NULL REFERENCES protection_observations(id),
    closing_observation_id INTEGER NOT NULL UNIQUE REFERENCES protection_observations(id),
    consumed_tt_ped REAL NOT NULL CHECK (consumed_tt_ped >= 0),
    markup_percent REAL NOT NULL CHECK (markup_percent >= 100),
    cost_ped REAL NOT NULL CHECK (cost_ped >= 0),
    status TEXT NOT NULL CHECK (status IN ('booked', 'pending')),
    session_id TEXT REFERENCES tracking_sessions(id),
    reason TEXT,
    created_at REAL NOT NULL,
    CHECK (
        (status = 'booked' AND session_id IS NOT NULL)
        OR (status = 'pending' AND session_id IS NULL)
    )
);

CREATE INDEX idx_protection_reconciliations_set
    ON protection_reconciliations(set_id, created_at, id);

-- Every protection interval retains the resolved component identities and
-- economic basis it began with. Later catalogue edits cannot rewrite play.
CREATE TABLE session_protection_intervals (
    interval_id INTEGER PRIMARY KEY REFERENCES session_intervals(id),
    loadout_id INTEGER NOT NULL REFERENCES protection_loadouts(id),
    loadout_name TEXT NOT NULL,
    armour_set_id INTEGER REFERENCES protection_sets(id),
    armour_set_name TEXT,
    armour_economy_kind TEXT,
    armour_markup_percent REAL,
    plate_set_id INTEGER REFERENCES protection_sets(id),
    plate_set_name TEXT,
    plate_economy_kind TEXT,
    plate_markup_percent REAL
);

CREATE INDEX idx_session_protection_armour
    ON session_protection_intervals(armour_set_id, interval_id);
CREATE INDEX idx_session_protection_plates
    ON session_protection_intervals(plate_set_id, interval_id);

-- One row per observed defensive chat event. Its context and protection
-- interval are the attribution facts; no wall-clock comparison is required.
CREATE TABLE protection_defence_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id),
    context_id INTEGER REFERENCES session_contexts(id),
    protection_interval_id INTEGER REFERENCES session_intervals(id),
    damage REAL CHECK (damage IS NULL OR damage >= 0),
    deflected INTEGER NOT NULL CHECK (deflected IN (0, 1)),
    CHECK ((damage IS NOT NULL AND deflected = 0) OR (damage IS NULL AND deflected = 1))
);

CREATE INDEX idx_protection_defence_context
    ON protection_defence_events(session_id, context_id, protection_interval_id);
