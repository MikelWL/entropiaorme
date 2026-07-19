CREATE INDEX IF NOT EXISTS idx_map_pins_spatial
    ON map_pins (planet, map_view_id, lon, lat);

CREATE TABLE navigation_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    planet TEXT NOT NULL,
    map_view_id INTEGER,
    status TEXT NOT NULL CHECK (status IN ('active', 'paused', 'completed', 'ended')),
    start_lon REAL NOT NULL,
    start_lat REAL NOT NULL,
    current_lon REAL NOT NULL,
    current_lat REAL NOT NULL,
    hop_count INTEGER NOT NULL,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    FOREIGN KEY (map_view_id) REFERENCES map_views(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_navigation_one_live_run
    ON navigation_runs ((1)) WHERE status IN ('active', 'paused');

CREATE TABLE navigation_stops (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL,
    pin_id INTEGER NOT NULL,
    ordinal INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'active', 'visited', 'skipped')),
    completed_at REAL,
    completion_source TEXT,
    observed_lon REAL,
    observed_lat REAL,
    observed_distance REAL,
    FOREIGN KEY (run_id) REFERENCES navigation_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (pin_id) REFERENCES map_pins(id) ON DELETE CASCADE,
    UNIQUE (run_id, pin_id),
    UNIQUE (run_id, ordinal)
);

CREATE INDEX idx_navigation_stops_run_status
    ON navigation_stops (run_id, status, ordinal);

CREATE TABLE map_pin_visits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pin_id INTEGER NOT NULL,
    run_id INTEGER,
    visited_at REAL NOT NULL,
    source TEXT NOT NULL,
    outcome TEXT NOT NULL,
    observed_lon REAL NOT NULL,
    observed_lat REAL NOT NULL,
    observed_distance REAL NOT NULL,
    FOREIGN KEY (pin_id) REFERENCES map_pins(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES navigation_runs(id) ON DELETE SET NULL
);

CREATE INDEX idx_map_pin_visits_pin_time
    ON map_pin_visits (pin_id, visited_at DESC);

CREATE TABLE radar_calibration (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    centre_x INTEGER NOT NULL,
    centre_y INTEGER NOT NULL,
    north_x INTEGER NOT NULL,
    north_y INTEGER NOT NULL,
    radius_px REAL NOT NULL CHECK (radius_px >= 8),
    display_scale REAL NOT NULL DEFAULT 1.0 CHECK (display_scale > 0),
    updated_at REAL NOT NULL
);
