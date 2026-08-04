-- A session always has a definition to be an instance of.
--
-- Selection used to be clearable, which left tracking with no session
-- name, no roster, and (once the overlay's activity control lands)
-- nothing to declare against. "Nothing in particular" is a legitimate
-- way to play, but it is a choice the user makes, not a hole: this
-- migration seeds one definition to carry it, so a fresh install can
-- start tracking immediately without being told to author something
-- first, and the clear affordance can go.
--
-- `is_protected` marks a definition that must not be deleted. It is
-- about existence, not identity: the seeded row is renameable and takes
-- a roster like any other, so a player whose permanent fallback is
-- really "General Hunting" can make it that.
--
-- Additive and forward-only: no existing row changes meaning. Sessions
-- recorded before definitions existed keep their NULL `definition_id`.
--
-- Settings live in a JSON file rather than this database, so the
-- selection itself is not backfilled here: `resolve_selection` in
-- `session_definitions.rs` reads "nothing chosen" as this definition,
-- which is what both the snapshot and session start go through.

ALTER TABLE session_definitions
    ADD COLUMN is_protected INTEGER NOT NULL DEFAULT 0;

-- Guarded so a database that already carries an active definition of
-- this name (authored by hand before this migration) is left alone
-- rather than colliding with the case-insensitive uniqueness rule.
INSERT INTO session_definitions (name, ad_hoc_segments, is_active, is_protected)
SELECT 'Default Tracking', 0, 1, 1
WHERE NOT EXISTS (
    SELECT 1 FROM session_definitions
    WHERE is_active = 1 AND lower(name) = 'default tracking'
);

-- Protect whichever row now carries the name, so a hand-authored one
-- becomes the protected definition instead of leaving none protected.
UPDATE session_definitions
SET is_protected = 1
WHERE is_active = 1 AND lower(name) = 'default tracking';
