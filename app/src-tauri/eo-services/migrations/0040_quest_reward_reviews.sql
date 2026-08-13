-- Append-only adjudication of ambiguous quest reward evidence.

CREATE TABLE quest_reward_reviews (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    completion_id INTEGER NOT NULL REFERENCES session_quest_completions(id),
    outcome TEXT NOT NULL CHECK (outcome IN ('confirmed', 'none')),
    policy TEXT NOT NULL CHECK (policy IN ('none', 'named_items', 'completion_clump')),
    note TEXT,
    reviewed_at REAL NOT NULL
);

CREATE INDEX idx_quest_reward_reviews_completion
    ON quest_reward_reviews(completion_id, reviewed_at, id);

CREATE TABLE quest_reward_review_items (
    review_id INTEGER NOT NULL REFERENCES quest_reward_reviews(id) ON DELETE CASCADE,
    source_loot_item_id INTEGER NOT NULL REFERENCES kill_loot_items(id),
    item_name TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    value_ped REAL NOT NULL CHECK (value_ped >= 0),
    PRIMARY KEY (review_id, source_loot_item_id),
    UNIQUE (source_loot_item_id)
);
