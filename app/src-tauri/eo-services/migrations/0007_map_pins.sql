-- Cartography pins: durable named locations on the bundled planet
-- maps. Free-standing user data with an optional backlink to the
-- tracked session a pin was dropped during (room for pins-from-events
-- analytics without a schema move). Coordinates are game units on the
-- global tile grid; radius_m NULL marks an exact point, a value an
-- area pin of that radius in metres; altitude rides along when the
-- source read carried one (display and waypoint copy tolerate NULL).
-- kind and icon are user-shaped presentation vocabulary (the pin
-- palette is user-configured), deliberately open TEXT, not a closed
-- set.

CREATE TABLE map_pins (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    planet      TEXT NOT NULL,
    lon         REAL NOT NULL,
    lat         REAL NOT NULL,
    altitude    REAL,
    name        TEXT NOT NULL,
    icon        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    radius_m    REAL,
    notes       TEXT,
    session_id  TEXT REFERENCES tracking_sessions(id),
    created_at  REAL NOT NULL
);

CREATE INDEX idx_map_pins_planet ON map_pins(planet);
