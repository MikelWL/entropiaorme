-- How long a listing was posted for.
--
-- The game charges its fee against a chosen duration, so the duration is part
-- of what the player decided at listing time and belongs with the listing.
--
-- The expiry date itself is NOT stored: it is listed_at plus this duration,
-- and a stored copy could drift from the two values it is derived from, the
-- same reason realised markup is derived rather than persisted.
--
-- Nullable on purpose. Listings created before this column existed were made
-- for a duration nobody recorded, and inventing one for them would fabricate
-- a deadline that never applied. A null duration simply never nudges.
ALTER TABLE auction_listings ADD COLUMN auction_days INTEGER
    CHECK (auction_days IS NULL OR auction_days > 0);
