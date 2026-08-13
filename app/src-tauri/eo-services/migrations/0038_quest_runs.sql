-- Durable administrative quest runs and the declared effort intervals they own.

CREATE TABLE quest_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    quest_id INTEGER NOT NULL REFERENCES quests(id),
    status TEXT NOT NULL CHECK (status IN ('in_progress', 'completed', 'cancelled')),
    started_at REAL NOT NULL,
    completed_at REAL,
    completion_id INTEGER REFERENCES session_quest_completions(id),
    CHECK ((status = 'completed') = (completed_at IS NOT NULL))
);

CREATE UNIQUE INDEX idx_quest_runs_one_active
    ON quest_runs(quest_id) WHERE status = 'in_progress';
CREATE INDEX idx_quest_runs_quest_started
    ON quest_runs(quest_id, started_at);

CREATE TABLE quest_run_intervals (
    run_id INTEGER NOT NULL REFERENCES quest_runs(id) ON DELETE CASCADE,
    interval_id INTEGER NOT NULL REFERENCES session_intervals(id),
    PRIMARY KEY (run_id, interval_id),
    UNIQUE (interval_id)
);

ALTER TABLE session_quest_completions
    ADD COLUMN quest_run_id INTEGER REFERENCES quest_runs(id);

-- Preserve a durable run identity for already recorded completions without
-- inventing effort links. Existing interval provenance is linked only when it
-- names the same quest as the completion.
INSERT INTO quest_runs(quest_id, status, started_at, completed_at, completion_id)
SELECT quest_id, 'completed', completed_at, completed_at, id
FROM session_quest_completions;

UPDATE session_quest_completions
SET quest_run_id = (
    SELECT r.id FROM quest_runs r
    WHERE r.completion_id = session_quest_completions.id
);

INSERT OR IGNORE INTO quest_run_intervals(run_id, interval_id)
SELECT r.id, c.activity_interval_id
FROM session_quest_completions c
JOIN quest_runs r ON r.completion_id = c.id
JOIN session_intervals i ON i.id = c.activity_interval_id
WHERE c.activity_interval_id IS NOT NULL
  AND i.kind = 'quest'
  AND i.ref_id = c.quest_id;
