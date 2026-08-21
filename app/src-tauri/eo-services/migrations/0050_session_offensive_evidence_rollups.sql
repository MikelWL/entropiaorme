-- Keep Community Model inputs on the same settled-plus-live read boundary as
-- the rest of Hunting analytics. The projection stores model-neutral evidence,
-- not a computed return, so a later model version can replay the original
-- Efficiency, looter, raw-TT, premium, and shot evidence without touching the
-- lifetime kill tables.
CREATE TABLE session_offensive_evidence_rollups (
    session_id               TEXT NOT NULL,
    mob_species              TEXT NOT NULL,
    evidence_fingerprint     TEXT NOT NULL,
    expected_economics_json  TEXT,
    shots_fired              INTEGER NOT NULL,
    missing_candidate_raw_tt REAL NOT NULL,
    missing_basis_phases     INTEGER NOT NULL
);

CREATE INDEX idx_session_offensive_evidence_rollups_session
    ON session_offensive_evidence_rollups(session_id);

-- Existing version-2 session projections are already correct. Backfill only
-- the new cell family for those settled sessions, then advance their marker so
-- the first Hunting read does not rebuild every older projection from raw
-- facts. Unsettled sessions retain their old marker state and follow the normal
-- scoped raw-read and healing path.
INSERT INTO session_offensive_evidence_rollups (
    session_id,
    mob_species,
    evidence_fingerprint,
    expected_economics_json,
    shots_fired,
    missing_candidate_raw_tt,
    missing_basis_phases
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
    )
FROM session_rollup_meta m
CROSS JOIN kills k
CROSS JOIN kill_tool_stats ts
WHERE m.rollup_version >= 2
  AND k.session_id = m.session_id
  AND ts.kill_id = k.id
GROUP BY k.session_id, 2, ts.evidence_fingerprint, ts.expected_economics_json;

UPDATE session_rollup_meta
SET rollup_version = 3
WHERE rollup_version >= 2;
