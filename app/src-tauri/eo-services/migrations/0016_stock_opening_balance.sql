-- Admit an opening-balance movement kind, so disposing of more stock than was
-- ever recorded cannot drive a holding negative.
--
-- Position derives from recorded loot plus the movement ledger. A sale or
-- conversion may legitimately exceed recorded loot: the player held stock from
-- before tracking began, or from a source the app never saw. Booking only the
-- outflow made the app's arithmetic say the player holds less than nothing,
-- which is not a thing that can be true of an inventory.
--
-- The outflow is not the wrong fact; it is an incomplete one. Disposing of
-- units the app never recorded is itself the evidence that they were held, so
-- the acquisition is recorded at the same moment, with no tier and no tool
-- because none is known. It funds no activity (source_kind 'unattributed'
-- forecloses that) and nets the position to zero rather than below it. If the
-- listing later expires, the returning units come back as stock the app now
-- knows about, which is exactly what the player has in hand.
--
-- SQLite cannot alter a CHECK constraint in place, so the table is rebuilt.
-- Every row carries across unchanged; only the accepted vocabulary widens.

PRAGMA foreign_keys = OFF;

CREATE TABLE stock_movements_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    item_name       TEXT NOT NULL,
    movement_kind   TEXT NOT NULL CHECK (movement_kind IN (
                        'listing', 'listing_return',
                        'conversion_out', 'conversion_in',
                        'opening_balance', 'legacy_adjustment')),
    ref_id          TEXT,
    source_kind     TEXT NOT NULL CHECK (source_kind IN (
                        'harvest', 'conversion', 'unattributed')),
    source_event_id TEXT,
    yield_tier      TEXT CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown')
                                OR yield_tier IS NULL),
    quantity        REAL NOT NULL,
    tt_value        REAL NOT NULL,
    occurred_at     TEXT NOT NULL,
    created_at      REAL NOT NULL,
    tool_name       TEXT
);

INSERT INTO stock_movements_new (
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, quantity, tt_value, occurred_at, created_at, tool_name
)
SELECT
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, quantity, tt_value, occurred_at, created_at, tool_name
FROM stock_movements;

DROP TABLE stock_movements;
ALTER TABLE stock_movements_new RENAME TO stock_movements;

DROP INDEX IF EXISTS idx_stock_movements_item;
DROP INDEX IF EXISTS idx_stock_movements_ref;
CREATE INDEX idx_stock_movements_item
    ON stock_movements(item_name, yield_tier, tool_name);
CREATE INDEX idx_stock_movements_ref ON stock_movements(ref_id);

PRAGMA foreign_keys = ON;
