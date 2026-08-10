-- Preserve the user-designated Hunting axis through stock movements.
--
-- Species remains the observed source activity. A session definition is an
-- orthogonal context carried by the same hunted loot, so one movement can
-- legitimately name both. Confirmed markup can then be projected by species
-- or by repeatable session without duplicating the sale.
--
-- Existing movements stay NULL. They predate this context dimension and no
-- safe historical assignment can be inferred after fungible stock has moved.

ALTER TABLE stock_movements ADD COLUMN session_definition_id INTEGER;

CREATE INDEX idx_stock_movements_hunting_definition
    ON stock_movements(item_name, mob_species, session_definition_id);
