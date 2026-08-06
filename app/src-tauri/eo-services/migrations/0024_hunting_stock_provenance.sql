-- Hunting joins the stock ledger: provenance for hunted output.
--
-- The auction lifecycle was built to be shared (`auction_listings.profession`
-- has said so since 0014); what was still harvest-only is the provenance a
-- movement carries. A harvest movement traces to a yield tier; a hunting
-- movement traces to the mob species whose kills produced the stock, which is
-- the observed axis Hunting reports realised markup on. The two dimensions are
-- deliberately separate nullable columns rather than one overloaded key: a row
-- carries at most one of them, and a CHECK cannot tell a species from a tier
-- if both live in one column.
--
-- SQLite cannot alter a CHECK constraint in place, so the table is rebuilt
-- (the 0016 precedent). Every row carries across unchanged; the accepted
-- source vocabulary widens by 'hunt' and the species column starts NULL
-- everywhere, because no existing movement came from hunting.

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
                        'harvest', 'hunt', 'conversion', 'unattributed')),
    source_event_id TEXT,
    yield_tier      TEXT CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown')
                                OR yield_tier IS NULL),
    -- The species whose kills produced the stock this movement drew on.
    -- NULL for harvest and unattributed rows; never a guess.
    mob_species     TEXT,
    quantity        REAL NOT NULL,
    tt_value        REAL NOT NULL,
    occurred_at     TEXT NOT NULL,
    created_at      REAL NOT NULL,
    tool_name       TEXT
);

INSERT INTO stock_movements_new (
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, mob_species, quantity, tt_value, occurred_at, created_at,
    tool_name
)
SELECT
    id, item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, NULL, quantity, tt_value, occurred_at, created_at, tool_name
FROM stock_movements;

DROP TABLE stock_movements;
ALTER TABLE stock_movements_new RENAME TO stock_movements;

DROP INDEX IF EXISTS idx_stock_movements_item;
DROP INDEX IF EXISTS idx_stock_movements_ref;
CREATE INDEX idx_stock_movements_item
    ON stock_movements(item_name, yield_tier, tool_name);
CREATE INDEX idx_stock_movements_species
    ON stock_movements(item_name, mob_species);
CREATE INDEX idx_stock_movements_ref ON stock_movements(ref_id);

PRAGMA foreign_keys = ON;

-- Conversions gain the same activity-family stamp listings have carried since
-- 0014, so each activity's History can list its own conversions rather than
-- every activity seeing everyone's. Every existing row was written by Tree
-- Cutting, which the default records truthfully.
ALTER TABLE stock_conversions ADD COLUMN profession TEXT NOT NULL DEFAULT 'harvesting';
