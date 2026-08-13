-- Typed quest completion and reward policies, plus immutable outcomes.

ALTER TABLE quests
    ADD COLUMN completion_trigger TEXT NOT NULL DEFAULT 'mission_log' CHECK (
        completion_trigger IN ('mission_log', 'signal_item')
    );
ALTER TABLE quests
    ADD COLUMN reward_policy TEXT NOT NULL DEFAULT 'none' CHECK (
        reward_policy IN ('none', 'fixed_ped', 'fixed_pes', 'named_items', 'completion_clump')
    );

UPDATE quests
SET completion_trigger = 'signal_item'
WHERE signal_loot_item IS NOT NULL AND trim(signal_loot_item) != '';

UPDATE quests
SET reward_policy = CASE
    WHEN reward_ped > 0 AND reward_is_skill = 1 THEN 'fixed_pes'
    WHEN reward_ped > 0 THEN 'fixed_ped'
    ELSE 'none'
END;

CREATE TABLE quest_reward_item_rules (
    quest_id INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    item_name TEXT NOT NULL CHECK (trim(item_name) != ''),
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (quest_id, item_name)
);

CREATE INDEX idx_quest_reward_item_rules_quest
    ON quest_reward_item_rules(quest_id, sort_order);

-- Every ARIS daily pays one voucher. The signal-completed bosses retain the
-- same item independently as their completion trigger.
UPDATE quests
SET reward_policy = 'named_items'
WHERE name LIKE 'ARIS - %';

INSERT OR IGNORE INTO quest_reward_item_rules(quest_id, item_name, sort_order)
SELECT id, 'Hyperion Daily Voucher', 0
FROM quests
WHERE name LIKE 'ARIS - %';

ALTER TABLE session_quest_completions
    ADD COLUMN reward_outcome TEXT CHECK (
        reward_outcome IS NULL OR reward_outcome IN ('confirmed', 'none', 'unresolved')
    );
ALTER TABLE session_quest_completions ADD COLUMN reward_policy_snapshot TEXT;
ALTER TABLE session_quest_completions ADD COLUMN reward_unresolved_reason TEXT;
ALTER TABLE session_quest_completions ADD COLUMN reward_evidence_json TEXT;

UPDATE session_quest_completions
SET reward_outcome = CASE
        WHEN reward_kind IS NULL THEN NULL
        WHEN reward_kind = 'none' THEN 'none'
        ELSE 'confirmed'
    END,
    reward_policy_snapshot = CASE reward_kind
        WHEN 'fixed_liquid' THEN 'fixed_ped'
        WHEN 'skill' THEN 'fixed_pes'
        WHEN 'item' THEN 'named_items'
        WHEN 'included_in_loot' THEN 'none'
        WHEN 'none' THEN 'none'
        ELSE NULL
    END;

