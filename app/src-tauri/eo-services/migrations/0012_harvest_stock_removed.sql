-- Per-item "already removed" overlay on recorded harvest loot: how much
-- of each harvested item has left the player's holdings (sold or spent),
-- relative to the lifetime recorded quantity. Current position = recorded
-- looted quantity minus this removed quantity.
--
-- This is an isolated market-position lever. It feeds the markup-confidence
-- estimate only; it never edits recorded activity stats (cycled, returns,
-- rate) or the ledger. Recorded harvest is the ground truth base; this is
-- the overlay on top of it. Maintained manually for now; the future ledger
-- sync of market sales will also write here.
CREATE TABLE harvest_stock_removed (
    item_name    TEXT PRIMARY KEY,
    removed_qty  INTEGER NOT NULL DEFAULT 0,
    updated_at   REAL NOT NULL
);
