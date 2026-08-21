-- Activity-level expected economics needs the same immutable context stamp as
-- direct cost and loot. This remains a rebuildable projection: raw kill/tool
-- facts are unchanged, and the next rollup version owns the finer cell grain.
ALTER TABLE session_offensive_evidence_rollups
    ADD COLUMN context_id INTEGER REFERENCES session_contexts(id);

CREATE INDEX idx_session_offensive_evidence_rollups_context
    ON session_offensive_evidence_rollups(session_id, context_id);

DELETE FROM session_offensive_evidence_rollups;

INSERT INTO session_offensive_evidence_rollups (
    session_id,
    mob_species,
    evidence_fingerprint,
    expected_economics_json,
    shots_fired,
    missing_candidate_raw_tt,
    missing_basis_phases,
    context_id
)
SELECT
    k.session_id,
    COALESCE(k.mob_species, ''),
    ts.evidence_fingerprint,
    ts.expected_economics_json,
    SUM(CASE WHEN ts.shots_fired > 0 THEN ts.shots_fired ELSE 0 END),
    SUM(
        CASE
            WHEN ts.expected_economics_json IS NULL
                 AND ts.shots_fired > 0
                 AND ts.cost_per_shot > 0
            THEN ts.shots_fired * ts.cost_per_shot
            ELSE 0
        END
    ),
    SUM(
        CASE
            WHEN ts.expected_economics_json IS NULL
                 AND ts.shots_fired > 0
                 AND ts.cost_per_shot > 0
            THEN 1
            ELSE 0
        END
    ),
    k.context_id
FROM session_rollup_meta m
CROSS JOIN kills k
CROSS JOIN kill_tool_stats ts
WHERE m.rollup_version >= 3
  AND k.session_id = m.session_id
  AND ts.kill_id = k.id
GROUP BY k.session_id, k.context_id, 2, ts.evidence_fingerprint, ts.expected_economics_json;

UPDATE session_rollup_meta
SET rollup_version = 4
WHERE rollup_version >= 3;
