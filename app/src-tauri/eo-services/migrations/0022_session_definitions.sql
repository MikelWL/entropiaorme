-- Session definitions: the deliberate activity families sessions are
-- instances of.
--
-- Once per-session variation lives in the facets (mob, boost, quest,
-- segment), session names converge to a handful of stable activity
-- families ("ARIS Dailies", "General Hunting") instead of proliferating
-- ad-hoc strings. A definition captures one such family as authored
-- data: a name plus a roster of the activities rehearsed in it, and an
-- opt-in flag for free-text segment naming on dynamic session shapes.
--
-- Tracked sessions become instances of a definition through a nullable
-- reference stamped at session start. The session's own `session_name`
-- column keeps being stamped alongside it (with the definition's name
-- as of the moment it was selected): the stamp is the durable
-- per-session fact, so a later definition rename or delete never
-- rewrites recorded history, while the reference carries
-- instance-to-family identity for aggregation.
--
-- Additive and forward-only: no existing row changes meaning. A NULL
-- `definition_id` is a session recorded before definitions existed, or
-- deliberately started outside one.

CREATE TABLE session_definitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    -- Opt-in free-text segment naming for dynamic session shapes;
    -- standardised definitions keep it off and rely on their roster.
    ad_hoc_segments INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at REAL NOT NULL DEFAULT (unixepoch('now')),
    updated_at REAL
);

-- The authored roster: the ordered activity entries a definition
-- rehearses. `kind` names what an entry references: 'quest_family'
-- (ref_id -> quest_families.id), 'quest' (ref_id -> quests.id), or
-- 'segment' (label only, no reference). The roster is replaced
-- wholesale on definition update (the playlist-items precedent), so
-- entries carry no update lifecycle of their own.
CREATE TABLE session_definition_roster (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    definition_id INTEGER NOT NULL REFERENCES session_definitions(id),
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    ref_id INTEGER,
    label TEXT
);

CREATE INDEX idx_session_definition_roster_definition
    ON session_definition_roster(definition_id);

ALTER TABLE tracking_sessions
    ADD COLUMN definition_id INTEGER REFERENCES session_definitions(id);

CREATE INDEX idx_tracking_sessions_definition
    ON tracking_sessions(definition_id);
