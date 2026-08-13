-- Separate economic reward treatment from evidence provenance.
--
-- `reward_source` records where completion evidence came from. In particular,
-- `tracked_loot` does not necessarily mean the reward remains in ordinary
-- loot: a captured item can be separated into immutable reward-item evidence.
-- `reward_kind` is the canonical accounting treatment used by analytics.

ALTER TABLE session_quest_completions
    ADD COLUMN reward_kind TEXT CHECK (
        reward_kind IS NULL OR reward_kind IN (
            'none', 'included_in_loot', 'fixed_liquid', 'item', 'skill'
        )
    );

UPDATE session_quest_completions
SET reward_kind = CASE
    WHEN EXISTS (
        SELECT 1
        FROM session_quest_completion_reward_items ri
        WHERE ri.completion_id = session_quest_completions.id
    ) THEN 'item'
    WHEN reward_source IS NULL THEN NULL
    WHEN reward_source = 'tracked_loot' THEN 'included_in_loot'
    WHEN reward_source = 'ledger' THEN 'fixed_liquid'
    WHEN reward_source = 'skill' THEN 'skill'
    ELSE 'none'
END;
