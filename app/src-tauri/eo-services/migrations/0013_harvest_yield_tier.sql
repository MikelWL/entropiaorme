-- Durable Tree Cutting source activity. The yield tier records what
-- board class the swing made available, independently of the tool that
-- paid for it. Unknown is explicit when the evidence cannot classify a
-- swing honestly.

ALTER TABLE harvest_events ADD COLUMN yield_tier TEXT NOT NULL DEFAULT 'unknown'
    CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown'));
ALTER TABLE harvest_events ADD COLUMN yield_tier_source TEXT
    CHECK (yield_tier_source IN ('board', 'inferred') OR yield_tier_source IS NULL);

CREATE INDEX idx_harvest_session_tool_time
    ON harvest_events(session_id, tool_name, timestamp);

-- Direct board evidence is authoritative even when the item was later
-- deactivated from accounting totals. Conflicting board classes on one
-- event remain unknown rather than choosing a class arbitrarily.
WITH board_evidence AS (
    SELECT
        harvest_id,
        CASE
            WHEN substr(item_name, 1, 6) = 'Short ' THEN 'short'
            WHEN substr(item_name, 1, 5) = 'Long ' THEN 'huge'
            ELSE 'long'
        END AS tier
    FROM harvest_loot_items
    WHERE substr(item_name, -6) = ' Board'
),
direct AS (
    SELECT harvest_id, MIN(tier) AS tier
    FROM board_evidence
    GROUP BY harvest_id
    HAVING MIN(tier) = MAX(tier)
)
UPDATE harvest_events
SET
    yield_tier = (SELECT direct.tier FROM direct WHERE direct.harvest_id = harvest_events.id),
    yield_tier_source = 'board'
WHERE id IN (SELECT harvest_id FROM direct);

-- Boardless rows inherit only from the same session and tool within 30
-- seconds. One-sided evidence is sufficient; two-sided evidence must
-- agree. Conflicts and isolated rows stay unknown.
WITH neighbours AS (
    SELECT
        h.id,
        (
            SELECT previous.yield_tier
            FROM harvest_events AS previous
            WHERE previous.session_id = h.session_id
              AND previous.tool_name IS h.tool_name
              AND previous.yield_tier_source = 'board'
              AND (
                    previous.timestamp < h.timestamp
                    OR (previous.timestamp = h.timestamp AND previous.id < h.id)
                  )
              AND h.timestamp - previous.timestamp <= 30.0
            ORDER BY previous.timestamp DESC, previous.id DESC
            LIMIT 1
        ) AS previous_tier,
        (
            SELECT following.yield_tier
            FROM harvest_events AS following
            WHERE following.session_id = h.session_id
              AND following.tool_name IS h.tool_name
              AND following.yield_tier_source = 'board'
              AND (
                    following.timestamp > h.timestamp
                    OR (following.timestamp = h.timestamp AND following.id > h.id)
                  )
              AND following.timestamp - h.timestamp <= 30.0
            ORDER BY following.timestamp ASC, following.id ASC
            LIMIT 1
        ) AS following_tier
    FROM harvest_events AS h
    WHERE h.yield_tier = 'unknown'
),
inferred AS (
    SELECT id, COALESCE(previous_tier, following_tier) AS tier
    FROM neighbours
    WHERE COALESCE(previous_tier, following_tier) IS NOT NULL
      AND (
            previous_tier IS NULL
            OR following_tier IS NULL
            OR previous_tier = following_tier
          )
)
UPDATE harvest_events
SET
    yield_tier = (SELECT inferred.tier FROM inferred WHERE inferred.id = harvest_events.id),
    yield_tier_source = 'inferred'
WHERE id IN (SELECT id FROM inferred);

CREATE INDEX idx_harvest_time_tier_tool
    ON harvest_events(timestamp, yield_tier, tool_name);
