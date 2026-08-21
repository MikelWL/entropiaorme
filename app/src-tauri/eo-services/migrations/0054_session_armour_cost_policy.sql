-- Armour-cost capture is the parent session-definition policy. Historical
-- sessions remain cost-enabled. Segment attribution was not yet a user-facing
-- default, so definitions move to the quieter whole-session default.

ALTER TABLE session_definitions
    ADD COLUMN track_protection_costs INTEGER NOT NULL DEFAULT 1
    CHECK (track_protection_costs IN (0, 1));

UPDATE session_definitions
SET track_protection_by_segment = 0;

ALTER TABLE tracking_sessions
    ADD COLUMN track_protection_costs INTEGER NOT NULL DEFAULT 1
    CHECK (track_protection_costs IN (0, 1));
