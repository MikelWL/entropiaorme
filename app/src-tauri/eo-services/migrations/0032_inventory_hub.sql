-- Central inventory ownership for loot and capital equipment.
--
-- Existing listings are pooled-loot listings. New equipment listings reuse
-- the same auction lifecycle while pointing at the stable inventory row whose
-- acquisition cost basis they realise. The item row is retained after sale so
-- history and undo never depend on a deleted source record.

ALTER TABLE inventory_items
    ADD COLUMN state TEXT NOT NULL DEFAULT 'held'
    CHECK (state IN ('held', 'listed', 'sold'));

ALTER TABLE inventory_items ADD COLUMN disposed_at TEXT;

ALTER TABLE auction_listings
    ADD COLUMN subject_kind TEXT NOT NULL DEFAULT 'loot'
    CHECK (subject_kind IN ('loot', 'equipment'));

ALTER TABLE auction_listings ADD COLUMN inventory_item_id TEXT;
ALTER TABLE auction_listings ADD COLUMN cost_basis REAL;

ALTER TABLE auction_listings
    ADD COLUMN channel TEXT NOT NULL DEFAULT 'auction'
    CHECK (channel IN ('auction', 'trade'));

CREATE INDEX idx_auction_listings_subject
    ON auction_listings(subject_kind, inventory_item_id, status);

CREATE INDEX idx_inventory_items_state
    ON inventory_items(state, acquired_at DESC);
