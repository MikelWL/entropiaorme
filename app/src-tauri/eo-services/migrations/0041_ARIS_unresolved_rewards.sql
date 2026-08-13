-- Surface historical ARIS completions whose voucher could not be attributed
-- exactly during guarded doctoring. Preserve the absence of invented evidence.

UPDATE session_quest_completions
SET reward_outcome = 'unresolved',
    reward_policy_snapshot = 'named_items',
    reward_unresolved_reason = 'Historical completion has no exact voucher attribution'
WHERE reward_outcome IS NULL
  AND reward_kind IS NULL
  AND quest_id IN (SELECT id FROM quests WHERE name LIKE 'ARIS - %');
