-- Enforce the one-to-one link between a durable quest run and its completion.

CREATE UNIQUE INDEX idx_quest_runs_completion
    ON quest_runs(completion_id) WHERE completion_id IS NOT NULL;

CREATE UNIQUE INDEX idx_session_quest_completions_run
    ON session_quest_completions(quest_run_id) WHERE quest_run_id IS NOT NULL;

CREATE TRIGGER trg_quest_runs_completion_pair
BEFORE UPDATE OF completion_id ON quest_runs
WHEN NEW.completion_id IS NOT NULL
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM session_quest_completions c
        WHERE c.id = NEW.completion_id AND c.quest_run_id = NEW.id
    ) THEN RAISE(ABORT, 'quest run completion ownership must be reciprocal') END;
END;

CREATE TRIGGER trg_session_quest_completions_run_pair
BEFORE UPDATE OF quest_run_id ON session_quest_completions
WHEN NEW.quest_run_id IS NOT NULL
BEGIN
    SELECT CASE WHEN EXISTS (
        SELECT 1 FROM quest_runs r
        WHERE r.completion_id = NEW.id AND r.id != NEW.quest_run_id
    ) OR EXISTS (
        SELECT 1 FROM quest_runs r
        WHERE r.id = NEW.quest_run_id
          AND r.completion_id IS NOT NULL
          AND r.completion_id != NEW.id
    ) THEN RAISE(ABORT, 'quest completion run ownership must be reciprocal') END;
END;
