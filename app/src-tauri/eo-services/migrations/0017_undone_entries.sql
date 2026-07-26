-- Keep an undone listing or conversion on file, marked rather than removed.
--
-- Undoing reverses every effect an entry had: its stock movements go, and so
-- does the money it wrote. What it does not do is pretend the player never
-- recorded it. The entry stays as a read-only record of something that was
-- taken back, which is the difference between correcting a mistake and losing
-- track of having made one.
--
-- NULL means the entry stands. Every read that computes a figure filters these
-- out; only the history list looks at them.
ALTER TABLE auction_listings ADD COLUMN undone_at TEXT;
ALTER TABLE stock_conversions ADD COLUMN undone_at TEXT;

CREATE INDEX idx_auction_listings_live ON auction_listings(undone_at, status);
