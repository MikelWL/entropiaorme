ALTER TABLE navigation_runs ADD COLUMN last_position_at REAL;
ALTER TABLE navigation_runs ADD COLUMN hotkey TEXT NOT NULL DEFAULT 'f8';
