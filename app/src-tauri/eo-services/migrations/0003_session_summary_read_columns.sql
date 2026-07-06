-- Extend the per-session summary with the counters the Activity and
-- session-list reads need, so those surfaces read a materialised row instead
-- of re-aggregating the raw tables on every request.
--
-- The summary already carries the costs, loot, cycled PED, and the dominant
-- mob/tag/weapon names; these add the pieces those two reads additionally
-- shape: the dominant mob/tag kill counts (Activity sums them), the raw
-- session skill-TT (Activity's SUM(ped_value), distinct from the per-skill
-- regular_skill_tt), the top-three primary mob/weapon lists (ungated, unlike
-- the single dominant_* fields), and the session's global/HOF counts.
--
-- Existing rows take the defaults until the version bump (SUMMARY_VERSION) heals
-- them: the read paths rebuild any below-version row on first read.

ALTER TABLE session_summaries ADD COLUMN dominant_mob_kills   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN dominant_tag_kills   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN activity_skill_tt    REAL    NOT NULL DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN primary_mobs_json    TEXT    NOT NULL DEFAULT '[]';
ALTER TABLE session_summaries ADD COLUMN primary_weapons_json TEXT    NOT NULL DEFAULT '[]';
ALTER TABLE session_summaries ADD COLUMN globals              INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session_summaries ADD COLUMN hofs                 INTEGER NOT NULL DEFAULT 0;
