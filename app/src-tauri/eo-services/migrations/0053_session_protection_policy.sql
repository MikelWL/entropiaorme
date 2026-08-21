-- Segment-specific protection declaration is an authored session-definition
-- choice. The value is stamped onto a session so later definition edits do not
-- rewrite what its overlay offered or how its protection cost is attributed.

ALTER TABLE session_definitions
    ADD COLUMN track_protection_by_segment INTEGER NOT NULL DEFAULT 1
    CHECK (track_protection_by_segment IN (0, 1));

ALTER TABLE tracking_sessions
    ADD COLUMN track_protection_by_segment INTEGER NOT NULL DEFAULT 1
    CHECK (track_protection_by_segment IN (0, 1));
