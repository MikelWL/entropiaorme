-- Per-preset pin configurations. The pin palette becomes first-class,
-- per-(planet, map view) data, and every placed pin is an instance of a
-- configuration. Colour and special behaviour derive from the config, so
-- editing a config restyles its placed pins; deleting a config cascades to
-- them. Category is generic (no behaviour) or special; the only special kind
-- so far is 'tree', which carries a distinct on-cooldown colour.

CREATE TABLE pin_configs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    planet TEXT NOT NULL,
    map_view_id INTEGER,
    label TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('generic', 'special')),
    special_kind TEXT CHECK (special_kind IN ('tree')),
    icon TEXT NOT NULL,
    radius_m REAL,
    colour TEXT NOT NULL,
    cooldown_colour TEXT,
    ordinal INTEGER NOT NULL,
    created_at REAL NOT NULL,
    FOREIGN KEY (map_view_id) REFERENCES map_views(id) ON DELETE CASCADE
);

CREATE INDEX idx_pin_configs_scope ON pin_configs (planet, map_view_id, ordinal);

ALTER TABLE map_pins ADD COLUMN pin_config_id INTEGER
    REFERENCES pin_configs(id) ON DELETE CASCADE;

CREATE INDEX idx_map_pins_config ON map_pins (pin_config_id);

-- Backfill: one configuration per distinct existing pin style in each
-- planet/view. Tree-emoji or tree-kind pins become the special 'tree' kind
-- with a distinct cooldown colour; everything else is generic. The tree-ness
-- expression is part of the grouping key, so each config's category and
-- colours are constant within its group.
INSERT INTO pin_configs
    (planet, map_view_id, label, category, special_kind, icon, radius_m, colour, cooldown_colour, ordinal, created_at)
SELECT
    planet,
    map_view_id,
    name,
    CASE WHEN icon IN ('🌳', '🌲') OR kind = 'tree' THEN 'special' ELSE 'generic' END,
    CASE WHEN icon IN ('🌳', '🌲') OR kind = 'tree' THEN 'tree' END,
    icon,
    radius_m,
    CASE WHEN icon IN ('🌳', '🌲') OR kind = 'tree' THEN '#22c55e' ELSE '#38bdf8' END,
    CASE WHEN icon IN ('🌳', '🌲') OR kind = 'tree' THEN '#f59e0b' END,
    0,
    MIN(created_at)
FROM map_pins
GROUP BY
    planet,
    map_view_id,
    name,
    icon,
    radius_m,
    CASE WHEN icon IN ('🌳', '🌲') OR kind = 'tree' THEN 'tree' END;

-- Link each placed pin to its synthesised configuration.
UPDATE map_pins SET pin_config_id = (
    SELECT pc.id FROM pin_configs pc
    WHERE pc.planet = map_pins.planet
      AND pc.map_view_id IS map_pins.map_view_id
      AND pc.label = map_pins.name
      AND pc.icon = map_pins.icon
      AND pc.radius_m IS map_pins.radius_m
      AND pc.special_kind IS (CASE WHEN map_pins.icon IN ('🌳', '🌲') OR map_pins.kind = 'tree' THEN 'tree' END)
    ORDER BY pc.id
    LIMIT 1
);
