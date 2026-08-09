-- First-class stock outcomes beyond the auction lifecycle.
--
-- A private trade realises a known price without auction fees. A removal says
-- only that stock is no longer held and deliberately has no financial effect.
-- Both keep the original loot intact and consume it through provenance-aware
-- movements. Shrapnel conversion remains a conversion, but unlike recycling
-- it produces 101% of the consumed TT and owns the resulting ledger gain.

CREATE TABLE private_sales (
    id                TEXT PRIMARY KEY,
    item_name         TEXT NOT NULL,
    profession        TEXT NOT NULL,
    quantity          REAL NOT NULL CHECK (quantity > 0),
    attributed_qty    REAL NOT NULL CHECK (attributed_qty >= 0),
    unattributed_qty  REAL NOT NULL CHECK (unattributed_qty >= 0),
    tt_value          REAL NOT NULL CHECK (tt_value >= 0),
    attributed_tt     REAL NOT NULL CHECK (attributed_tt >= 0),
    final_price       REAL NOT NULL CHECK (final_price >= 0),
    sold_at           TEXT NOT NULL,
    sale_entry_id     TEXT,
    created_at        REAL NOT NULL,
    undone_at         TEXT
);

CREATE INDEX idx_private_sales_profession
    ON private_sales(profession, sold_at DESC);
CREATE INDEX idx_private_sales_item ON private_sales(item_name);

CREATE TABLE stock_removals (
    id            TEXT PRIMARY KEY,
    item_name     TEXT NOT NULL,
    profession    TEXT NOT NULL,
    quantity      REAL NOT NULL CHECK (quantity > 0),
    tt_value      REAL NOT NULL CHECK (tt_value >= 0),
    removed_at    TEXT NOT NULL,
    created_at    REAL NOT NULL,
    undone_at     TEXT
);

CREATE INDEX idx_stock_removals_profession
    ON stock_removals(profession, removed_at DESC);

ALTER TABLE stock_conversions ADD COLUMN output_tt_value REAL;
ALTER TABLE stock_conversions ADD COLUMN attributed_tt REAL;
ALTER TABLE stock_conversions ADD COLUMN gain_entry_id TEXT;

-- Widen the closed movement vocabulary. SQLite requires a table rebuild to
-- alter a CHECK constraint; every existing row and provenance dimension is
-- copied byte-for-byte.
PRAGMA foreign_keys = OFF;

CREATE TABLE stock_movements_new (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    item_name             TEXT NOT NULL,
    movement_kind         TEXT NOT NULL CHECK (movement_kind IN (
                                'listing', 'listing_return',
                                'conversion_out', 'conversion_in',
                                'trade', 'removal',
                                'opening_balance', 'legacy_adjustment')),
    ref_id                TEXT,
    source_kind           TEXT NOT NULL CHECK (source_kind IN (
                                'harvest', 'hunt', 'conversion', 'unattributed')),
    source_event_id       TEXT,
    yield_tier            TEXT CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown')
                                      OR yield_tier IS NULL),
    mob_species           TEXT,
    quantity              REAL NOT NULL,
    tt_value              REAL NOT NULL,
    occurred_at           TEXT NOT NULL,
    created_at            REAL NOT NULL,
    tool_name             TEXT,
    session_definition_id INTEGER
);

INSERT INTO stock_movements_new (
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, mob_species, quantity, tt_value, occurred_at, created_at,
    tool_name, session_definition_id
)
SELECT
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, mob_species, quantity, tt_value, occurred_at, created_at,
    tool_name, session_definition_id
FROM stock_movements;

DROP TABLE stock_movements;
ALTER TABLE stock_movements_new RENAME TO stock_movements;

CREATE INDEX idx_stock_movements_item
    ON stock_movements(item_name, yield_tier, tool_name);
CREATE INDEX idx_stock_movements_species
    ON stock_movements(item_name, mob_species);
CREATE INDEX idx_stock_movements_ref ON stock_movements(ref_id);
CREATE INDEX idx_stock_movements_hunting_definition
    ON stock_movements(item_name, mob_species, session_definition_id);

PRAGMA foreign_keys = ON;
