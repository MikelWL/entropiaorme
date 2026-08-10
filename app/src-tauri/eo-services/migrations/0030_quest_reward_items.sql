-- Immutable item evidence observed alongside a quest completion.
--
-- The quest's authored expected-markup field is intentionally not copied
-- here. Market value is a current projection over the actual item and its
-- completion-time TT, while missing market data leaves that TT unmarked-up.

CREATE TABLE session_quest_completion_reward_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    completion_id INTEGER NOT NULL
        REFERENCES session_quest_completions(id) ON DELETE CASCADE,
    item_name TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    value_ped REAL NOT NULL CHECK (value_ped >= 0)
);

CREATE INDEX idx_sqc_reward_items_completion
    ON session_quest_completion_reward_items(completion_id);
