-- Item-name lookups over active loot rows.
--
-- The stock surfaces key several reads on an item's name: the per-item
-- position arithmetic inside the sale/convert/undo transactions, the raw
-- safety check behind reversals, and the DISTINCT item universes behind the
-- market and stock panels. Both loot tables grow with every recorded event
-- and had no item-name index, so each of those reads paid a full scan of a
-- table whose size tracks total play history.
--
-- Partial on active rows because that is the only population those reads
-- address: a deactivated row is archived out of positions and universes
-- alike, and keeping it out of the index keeps the index dense.

CREATE INDEX idx_kill_loot_items_item_active
    ON kill_loot_items (item_name)
    WHERE deactivated_at IS NULL;

CREATE INDEX idx_harvest_loot_items_item_active
    ON harvest_loot_items (item_name)
    WHERE deactivated_at IS NULL;
