-- Item evidence is an independent component of a completion reward.
--
-- Migration 0035 classified any completion carrying item evidence as an item
-- reward. Restore the primary treatment for the uncommon case where that same
-- completion also has a confirmed liquid or progression reward. Analytics
-- derives the mixed presentation from this treatment plus the item evidence.

UPDATE session_quest_completions
SET reward_kind = CASE reward_source
    WHEN 'ledger' THEN 'fixed_liquid'
    WHEN 'skill' THEN 'skill'
END
WHERE reward_kind = 'item'
  AND reward_source IN ('ledger', 'skill')
  AND EXISTS (
      SELECT 1
      FROM session_quest_completion_reward_items ri
      WHERE ri.completion_id = session_quest_completions.id
  );
