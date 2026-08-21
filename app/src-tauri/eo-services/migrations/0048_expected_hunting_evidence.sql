-- Immutable, model-neutral evidence for the efficiency-bearing offensive
-- streams used by each recorded tool phase. Legacy rows remain NULL and are
-- never presented as if their historical loadout had been captured.
ALTER TABLE kill_tool_stats ADD COLUMN expected_economics_json TEXT;
