-- Co-recordable session facets. A session carries independent context
-- facets instead of one mutually exclusive tag-or-mob capture mode: the
-- user-designated session name and the skill-boost configuration the
-- session ran under. Both are NULL on legacy rows and on sessions that
-- declared nothing; NULL means "not captured", never a guessed default.

ALTER TABLE tracking_sessions ADD COLUMN session_name TEXT;
ALTER TABLE tracking_sessions ADD COLUMN skill_boost_percent INTEGER
    CHECK (skill_boost_percent > 0 OR skill_boost_percent IS NULL);

-- Per-kill mob stamp provenance. 'declared' marks a stamp fed by the
-- player's declaration; 'detected' is reserved for automatic detection.
-- NULL on legacy rows (whose stamps predate the discriminant) and on
-- kills recorded with no declaration in force.
ALTER TABLE kills ADD COLUMN mob_stamp_source TEXT
    CHECK (mob_stamp_source IN ('declared', 'detected') OR mob_stamp_source IS NULL);

-- Backfill: a legacy tag-mode session's stamped tag was its de facto
-- session name (kills stamped with an empty species carry the tag in
-- mob_name). Lift the session's dominant tag so the designated axis
-- reads continuously across the model change; 'Unknown' rows are the
-- unset stamp, not a tag. Mob-mode sessions keep a NULL name.
UPDATE tracking_sessions
SET session_name = (
    SELECT k.mob_name
    FROM kills AS k
    WHERE k.session_id = tracking_sessions.id
      AND COALESCE(k.mob_species, '') = ''
      AND COALESCE(k.mob_name, '') NOT IN ('', 'Unknown')
    GROUP BY k.mob_name
    ORDER BY COUNT(*) DESC, k.mob_name ASC
    LIMIT 1
)
WHERE mob_tracking_mode = 'tag';
