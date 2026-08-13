-- Informational absolute PED-per-unit quotes for zero-TT and unit-priced items.

CREATE TABLE market_unit_price_observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    item_name TEXT NOT NULL CHECK (trim(item_name) != ''),
    ped_per_unit REAL NOT NULL CHECK (ped_per_unit >= 0),
    observed_at REAL NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'paste'))
);

CREATE INDEX idx_market_unit_prices_item_observed
    ON market_unit_price_observations(item_name, observed_at, id);
