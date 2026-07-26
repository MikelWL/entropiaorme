-- Record which tool produced the stock an outflow consumed, beside the yield
-- tier already recorded.
--
-- The tier is the source activity, but a tier is worked by several tools, and
-- which one produced a given unit is knowable only while that unit is still
-- held. Recording it here keeps a finer reading available; nothing reports on
-- it today.
--
-- Deriving the tool share from lifetime loot composition would model what this
-- records: allocations are captured at the moment stock leaves, so a change in
-- which tool is used later cannot rewrite what an earlier sale drew on.
--
-- NULL means the movement predates this column, or the swing recorded no tool.
-- Both are genuinely unknown and stay that way rather than being apportioned.
ALTER TABLE stock_movements ADD COLUMN tool_name TEXT;

DROP INDEX IF EXISTS idx_stock_movements_item;
CREATE INDEX idx_stock_movements_item
    ON stock_movements(item_name, yield_tier, tool_name);
