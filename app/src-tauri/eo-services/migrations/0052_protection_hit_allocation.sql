-- Protection decay is caused by protection absorbed, which the chat log does
-- not reveal. Every observed incoming hit is therefore the honest equal-weight
-- proxy, including a deflection. Raw damage and deflection values remain as
-- evidence but no longer determine the allocation share.

ALTER TABLE protection_cost_allocations
    ADD COLUMN hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0);
ALTER TABLE protection_cost_context_allocations
    ADD COLUMN hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0);

CREATE TEMP TABLE old_protection_session_cost AS
SELECT a.session_id, SUM(a.cost_ped) AS cost_ped
FROM protection_cost_allocations a
WHERE EXISTS (
    SELECT 1 FROM protection_cost_evidence e WHERE e.window_id = a.window_id
)
GROUP BY a.session_id;

CREATE TEMP TABLE reweighted_protection_context AS
WITH hits AS (
    SELECT
        e.window_id,
        d.session_id,
        COALESCE(d.context_id, -1) AS context_key,
        d.context_id,
        COUNT(DISTINCT e.defence_event_id) AS hit_count
    FROM protection_cost_evidence e
    JOIN protection_defence_events d ON d.id = e.defence_event_id
    GROUP BY e.window_id, d.session_id, COALESCE(d.context_id, -1), d.context_id
), totals AS (
    SELECT window_id, SUM(hit_count) AS total_hits
    FROM hits
    GROUP BY window_id
)
SELECT
    h.window_id,
    h.session_id,
    h.context_key,
    h.context_id,
    h.hit_count,
    t.total_hits,
    ROW_NUMBER() OVER (
        PARTITION BY h.window_id ORDER BY h.session_id, h.context_key
    ) AS allocation_index,
    COUNT(*) OVER (PARTITION BY h.window_id) AS allocation_count
FROM hits h
JOIN totals t ON t.window_id = h.window_id
WHERE t.total_hits > 0;

UPDATE protection_cost_context_allocations AS allocation
SET hit_count = COALESCE((
        SELECT weight.hit_count
        FROM reweighted_protection_context weight
        WHERE weight.window_id = allocation.window_id
          AND weight.session_id = allocation.session_id
          AND weight.context_key = allocation.context_key
    ), 0)
WHERE EXISTS (
    SELECT 1 FROM reweighted_protection_context weight
    WHERE weight.window_id = allocation.window_id
);

UPDATE protection_cost_context_allocations AS allocation
SET
    allocation_share = (
        SELECT CAST(weight.hit_count AS REAL) / weight.total_hits
        FROM reweighted_protection_context weight
        WHERE weight.window_id = allocation.window_id
          AND weight.session_id = allocation.session_id
          AND weight.context_key = allocation.context_key
    ),
    cost_ped = CASE
        WHEN (
            SELECT weight.allocation_index = weight.allocation_count
            FROM reweighted_protection_context weight
            WHERE weight.window_id = allocation.window_id
              AND weight.session_id = allocation.session_id
              AND weight.context_key = allocation.context_key
        ) THEN (
            SELECT window.cost_ped - COALESCE(SUM(
                window.cost_ped * CAST(prior.hit_count AS REAL) / prior.total_hits
            ), 0)
            FROM protection_cost_windows window
            JOIN reweighted_protection_context current
              ON current.window_id = window.id
             AND current.session_id = allocation.session_id
             AND current.context_key = allocation.context_key
            LEFT JOIN reweighted_protection_context prior
              ON prior.window_id = current.window_id
             AND prior.allocation_index < current.allocation_index
            WHERE window.id = allocation.window_id
        )
        ELSE (
            SELECT window.cost_ped * CAST(weight.hit_count AS REAL) / weight.total_hits
            FROM protection_cost_windows window
            JOIN reweighted_protection_context weight ON weight.window_id = window.id
            WHERE window.id = allocation.window_id
              AND weight.session_id = allocation.session_id
              AND weight.context_key = allocation.context_key
        )
    END
WHERE EXISTS (
    SELECT 1 FROM reweighted_protection_context weight
    WHERE weight.window_id = allocation.window_id
      AND weight.session_id = allocation.session_id
      AND weight.context_key = allocation.context_key
);

DELETE FROM protection_cost_allocations
WHERE window_id IN (SELECT DISTINCT window_id FROM reweighted_protection_context);

INSERT INTO protection_cost_allocations (
    window_id, session_id, damage_weight, deflection_count,
    allocation_share, cost_ped, hit_count
)
SELECT
    window_id,
    session_id,
    SUM(damage_weight),
    SUM(deflection_count),
    SUM(allocation_share),
    SUM(cost_ped),
    SUM(hit_count)
FROM protection_cost_context_allocations
WHERE window_id IN (SELECT DISTINCT window_id FROM reweighted_protection_context)
GROUP BY window_id, session_id;

UPDATE tracking_sessions AS session
SET armour_cost = MAX(
    0,
    COALESCE(session.armour_cost, 0)
      - COALESCE((
          SELECT old.cost_ped FROM old_protection_session_cost old
          WHERE old.session_id = session.id
        ), 0)
      + COALESCE((
          SELECT SUM(allocation.cost_ped)
          FROM protection_cost_allocations allocation
          WHERE allocation.session_id = session.id
            AND allocation.window_id IN (
                SELECT DISTINCT window_id FROM reweighted_protection_context
            )
        ), 0)
)
WHERE session.id IN (
    SELECT session_id FROM old_protection_session_cost
    UNION
    SELECT session_id FROM protection_cost_allocations
    WHERE window_id IN (SELECT DISTINCT window_id FROM reweighted_protection_context)
);

DROP TABLE reweighted_protection_context;
DROP TABLE old_protection_session_cost;
