-- Healing cost is booked from a paid activation confirmed by a hotbar intent,
-- never from an isolated chat-log output. The evidence tables keep the raw
-- outputs and effect windows so attribution remains inspectable and can grow
-- to cover restoration-style healing without rewriting session totals.

CREATE TABLE healing_activations (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    equipment_id INTEGER REFERENCES equipment_library(id) ON DELETE SET NULL,
    tool_name TEXT NOT NULL,
    intent_at REAL NOT NULL,
    observed_at REAL NOT NULL,
    chat_timestamp TEXT NOT NULL,
    context_id INTEGER REFERENCES session_contexts(id) ON DELETE SET NULL,
    cost_ped REAL NOT NULL CHECK (cost_ped >= 0),
    profile_json TEXT NOT NULL,
    provenance TEXT NOT NULL CHECK (
        provenance IN ('direct', 'health_capped', 'retrospective')
    )
);

CREATE INDEX idx_healing_activations_session
    ON healing_activations(session_id, observed_at, id);

CREATE TABLE healing_effect_windows (
    id TEXT PRIMARY KEY,
    activation_id TEXT NOT NULL REFERENCES healing_activations(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    equipment_id INTEGER REFERENCES equipment_library(id) ON DELETE SET NULL,
    tool_name TEXT NOT NULL,
    started_at REAL NOT NULL,
    expires_at REAL NOT NULL,
    tick_min REAL,
    tick_max REAL,
    tick_seconds REAL,
    context_id INTEGER REFERENCES session_contexts(id) ON DELETE SET NULL,
    CHECK (expires_at >= started_at),
    CHECK (tick_min IS NULL OR tick_min >= 0),
    CHECK (tick_max IS NULL OR tick_max >= COALESCE(tick_min, 0)),
    CHECK (tick_seconds IS NULL OR tick_seconds > 0)
);

CREATE INDEX idx_healing_effect_windows_session
    ON healing_effect_windows(session_id, started_at, id);

CREATE TABLE healing_outputs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    activation_id TEXT REFERENCES healing_activations(id) ON DELETE SET NULL,
    effect_window_id TEXT REFERENCES healing_effect_windows(id) ON DELETE SET NULL,
    context_id INTEGER REFERENCES session_contexts(id) ON DELETE SET NULL,
    observed_at REAL NOT NULL,
    chat_timestamp TEXT NOT NULL,
    amount REAL NOT NULL CHECK (amount > 0),
    classification TEXT NOT NULL CHECK (
        classification IN ('direct', 'effect', 'passive', 'unattributed')
    ),
    reason TEXT NOT NULL
);

CREATE INDEX idx_healing_outputs_session
    ON healing_outputs(session_id, observed_at, id);
