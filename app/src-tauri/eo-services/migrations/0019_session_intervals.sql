-- Session intervals: the segment layer.
--
-- A session is not uniform. A pill holds for part of it, a quest spans a
-- stretch of it, a lap is a slice the player draws around one run. Every
-- one of those is the same shape: something that held from one moment to
-- another, inside one session. Modelling them separately would fork the
-- same primitive four ways, so they share one spine and differ by kind.
--
-- Two tables carry it, and the split is deliberate:
--
--   * `session_intervals` is authoritative for DURATION and COST. It is
--     the only place that can record a stretch containing no events at
--     all, and the only place a consumable's identity (and therefore its
--     price) can hang.
--
--   * `session_contexts` is authoritative for ATTRIBUTION. A context is
--     the set of intervals in force at a moment; a fresh one is minted
--     whenever that set changes, and every event row stamps the context
--     current when it was written.
--
-- Attribution never compares timestamps. Interval bounds are wall-clock
-- (a declaration is a UI action), while kill, harvest and skill-gain
-- timestamps carry the chat log's centralised in-game server time, an
-- hour apart on a UK box. Bucketing events into intervals by time would
-- be silently wrong; stamping the context at insert cannot be.
--
-- One context per event row rather than one column per axis: an event
-- sits inside a lap AND a quest slice AND a modifier set at once, and a
-- column-per-axis schema grows a column for every future kind. A later
-- kind (consumable timers) needs no change to the event tables at all.

CREATE TABLE session_intervals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    -- Open vocabulary on purpose: the kinds are a product question, not a
    -- schema one, and a new kind must not need a migration.
    kind TEXT NOT NULL,
    -- The display name: a lap's name, a quest's title, a pill's item name.
    label TEXT,
    -- What the interval points at in its own domain, when it points at
    -- anything (a quest id, a playlist id, an equipment id). The table it
    -- refers to is implied by `kind`; no cross-domain foreign key is
    -- declared, so a referenced row's removal cannot cascade into the
    -- session record.
    ref_id INTEGER,
    -- The modifier magnitude, as the pill's labelled percentage. ZERO IS
    -- MEANINGFUL: it records "declared, and nothing was in force", which
    -- is what makes an unboosted baseline recordable. NULL is "this kind
    -- carries no magnitude", not "no boost".
    magnitude REAL,
    -- Wall-clock bounds. For duration and cost only; never for
    -- attributing an event to this interval (see the header).
    started_at REAL NOT NULL,
    ended_at REAL,
    origin_device TEXT
);

CREATE INDEX idx_session_intervals_session ON session_intervals(session_id, kind);
CREATE INDEX idx_session_intervals_open ON session_intervals(session_id) WHERE ended_at IS NULL;

-- The set of intervals in force at a moment. Minted on every change to
-- that set, including the empty one at session start: an event stamped
-- with a context that has no interval of a given kind was recorded under
-- the segment model with nothing of that kind declared, which is a
-- different fact from an event that predates the model entirely (those
-- carry no context at all).
CREATE TABLE session_contexts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES tracking_sessions(id) ON DELETE CASCADE,
    created_at REAL NOT NULL
);

CREATE INDEX idx_session_contexts_session ON session_contexts(session_id);

CREATE TABLE session_context_intervals (
    context_id INTEGER NOT NULL REFERENCES session_contexts(id) ON DELETE CASCADE,
    interval_id INTEGER NOT NULL REFERENCES session_intervals(id) ON DELETE CASCADE,
    PRIMARY KEY (context_id, interval_id)
);

CREATE INDEX idx_session_context_intervals_interval ON session_context_intervals(interval_id);

-- The stamp on every economically relevant event. NULL means the row
-- predates the segment model (or was written outside a session), and is
-- never to be read as "nothing was in force": unknown and declared-none
-- are different facts, and conflating them is what makes a baseline
-- impossible to establish.
ALTER TABLE kills ADD COLUMN context_id INTEGER REFERENCES session_contexts(id);
ALTER TABLE harvest_events ADD COLUMN context_id INTEGER REFERENCES session_contexts(id);
ALTER TABLE skill_gains ADD COLUMN context_id INTEGER REFERENCES session_contexts(id);

CREATE INDEX idx_kills_context ON kills(context_id);
CREATE INDEX idx_harvest_events_context ON harvest_events(context_id);
CREATE INDEX idx_skill_gains_context ON skill_gains(context_id);

-- No backfill. A historical session has no recorded segment structure,
-- and inventing one interval per session would manufacture a fact the
-- record never held: the boost it declared at its start is not evidence
-- that the same boost held throughout. Historical rows stay unstamped
-- and read as "not captured", which is the truth about them. A session's
-- own declared value remains on `tracking_sessions` for the sessions
-- that predate this model.
