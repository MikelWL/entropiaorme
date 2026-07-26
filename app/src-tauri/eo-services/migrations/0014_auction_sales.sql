-- Structured market sales, stock conversions, and the signed stock-movement
-- ledger that replaces the interim removed-quantity overlay.
--
-- Recorded loot stays the acquisition base: harvest_loot_items joined to its
-- event's yield tier already says what each activity produced, so nothing here
-- duplicates it. This migration adds only what leaves, transforms, or returns.
-- Current position = active recorded loot + the signed movements below.
--
-- The auction lifecycle is the reason a listing is a first-class record rather
-- than a ledger row: listed stock has physically left the player's inventory at
-- an unknown final price, and may sell, or expire and come back. Only a
-- confirmed sale realises markup.

CREATE TABLE auction_listings (
    id                TEXT PRIMARY KEY,
    item_name         TEXT NOT NULL,
    -- Which activity family the sold stock came from. Harvesting today;
    -- hunting reuses the same table rather than growing a parallel one.
    profession        TEXT NOT NULL DEFAULT 'harvesting',
    quantity          INTEGER NOT NULL CHECK (quantity > 0),
    -- Quantity covered by tracked stock, and the excess beyond it. The excess
    -- is real money with no activity claim on it, so it is carried explicitly
    -- rather than being silently credited to the activities that are tracked.
    attributed_qty    REAL NOT NULL DEFAULT 0 CHECK (attributed_qty >= 0),
    unattributed_qty  REAL NOT NULL DEFAULT 0 CHECK (unattributed_qty >= 0),
    tt_value          REAL NOT NULL CHECK (tt_value >= 0),
    attributed_tt     REAL NOT NULL DEFAULT 0 CHECK (attributed_tt >= 0),
    starting_bid      REAL NOT NULL CHECK (starting_bid >= 0),
    buyout            REAL,
    -- Charged and deducted the moment the listing is created; spent whether or
    -- not the item ever sells.
    listing_fee       REAL NOT NULL DEFAULT 0 CHECK (listing_fee >= 0),
    listed_at         TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'pending'
                      CHECK (status IN ('pending', 'sold', 'expired')),
    -- Set on resolution only.
    final_price       REAL,
    sale_fee          REAL,
    resolved_at       TEXT,
    -- The ledger rows this listing owns. The ledger stays the system of record
    -- for money; the listing owns the lifecycle and points at its entries.
    fee_entry_id      TEXT,
    sale_entry_id     TEXT,
    sale_fee_entry_id TEXT,
    created_at        REAL NOT NULL,
    updated_at        REAL NOT NULL
);

CREATE INDEX idx_auction_listings_status ON auction_listings(status, listed_at DESC);
CREATE INDEX idx_auction_listings_item ON auction_listings(item_name);

-- Stock transformations that preserve TT exactly (recycling boards into
-- Nanocubes is 1:1: 100 PED of wood becomes 100 PED of Nanocubes, with no
-- refiner decay). Not a sale: no markup is realised, and the source's activity
-- composition rides forward into the produced item so a later Nanocube sale
-- still attributes back to the tiers that grew it.
CREATE TABLE stock_conversions (
    id            TEXT PRIMARY KEY,
    source_item   TEXT NOT NULL,
    target_item   TEXT NOT NULL,
    quantity      REAL NOT NULL CHECK (quantity > 0),
    tt_value      REAL NOT NULL CHECK (tt_value >= 0),
    converted_at  TEXT NOT NULL,
    created_at    REAL NOT NULL
);

CREATE INDEX idx_stock_conversions_source ON stock_conversions(source_item);

-- The signed movement ledger. Positive rows acquire, negative rows consume.
-- Rows are append-only: an expired listing writes returning rows rather than
-- deleting its original allocation, so what was attributed at listing time
-- stays auditable.
--
-- Quantities are REAL because a weighted split across yield tiers is
-- proportional and must stay exact; rounding belongs at the display edge.
CREATE TABLE stock_movements (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    item_name       TEXT NOT NULL,
    movement_kind   TEXT NOT NULL CHECK (movement_kind IN (
                        'listing', 'listing_return',
                        'conversion_out', 'conversion_in',
                        'legacy_adjustment')),
    -- The listing or conversion this movement belongs to.
    ref_id          TEXT,
    -- Where the consumed or produced units trace back to. 'unattributed' is an
    -- explicit unknown, never a guess.
    source_kind     TEXT NOT NULL CHECK (source_kind IN (
                        'harvest', 'conversion', 'unattributed')),
    source_event_id TEXT,
    yield_tier      TEXT CHECK (yield_tier IN ('short', 'long', 'huge', 'unknown')
                                OR yield_tier IS NULL),
    quantity        REAL NOT NULL,
    tt_value        REAL NOT NULL,
    occurred_at     TEXT NOT NULL,
    created_at      REAL NOT NULL
);

CREATE INDEX idx_stock_movements_item ON stock_movements(item_name, yield_tier);
CREATE INDEX idx_stock_movements_ref ON stock_movements(ref_id);

-- Carry the interim overlay across before retiring it. Its rows record only
-- that a quantity left holdings, never which activity produced it, so they
-- migrate as explicitly unattributed rather than being apportioned across
-- tiers: inventing that provenance is exactly what the accounting model
-- forbids. TT comes from the item's own recorded unit value.
INSERT INTO stock_movements (
    item_name, movement_kind, ref_id, source_kind, source_event_id,
    yield_tier, quantity, tt_value, occurred_at, created_at
)
SELECT
    r.item_name,
    'legacy_adjustment',
    NULL,
    'unattributed',
    NULL,
    NULL,
    -r.removed_qty,
    -(r.removed_qty * COALESCE((
        SELECT SUM(l.value_ped) / NULLIF(SUM(l.quantity), 0)
        FROM harvest_loot_items AS l
        WHERE l.item_name = r.item_name
    ), 0)),
    date(r.updated_at, 'unixepoch'),
    r.updated_at
FROM harvest_stock_removed AS r
WHERE r.removed_qty > 0;

-- Retired: current position now derives from recorded loot plus the movement
-- ledger above, and two sources of truth for the same quantity is precisely
-- the drift this replaces. The rows themselves were copied first.
DROP TABLE harvest_stock_removed;
