-- Named views provide independent pin sets over the same bundled
-- planet raster. The permanent Default view is represented by NULL,
-- preserving every existing pin without manufacturing one row per
-- planet. The service deletes a named view and its pins atomically;
-- foreign-key enforcement is intentionally disabled for this database.

CREATE TABLE map_views (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    planet      TEXT NOT NULL,
    name        TEXT NOT NULL COLLATE NOCASE,
    created_at  REAL NOT NULL,
    UNIQUE (planet, name)
);

ALTER TABLE map_pins
    ADD COLUMN map_view_id INTEGER REFERENCES map_views(id) ON DELETE CASCADE;

CREATE INDEX idx_map_views_planet ON map_views(planet, created_at, id);
CREATE INDEX idx_map_pins_planet_view ON map_pins(planet, map_view_id, created_at);
