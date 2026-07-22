-- Market-markup observations: the manual bulk-paste feed behind the
-- market surface.
--
-- The game's auction house exports a per-item markup/sales table to the
-- clipboard, five aggregation horizons per item (day, week, month, year,
-- decade). The app ingests that export as a user-initiated paste; no
-- market data is fetched at runtime. One submission row records one
-- accepted paste; its observations carry the pasted readings verbatim,
-- five rows per item (markup NULL where the game reported N/A, meaning
-- no sales in that horizon; Sales is TT turnover normalised to PED).
--
-- These tables are an INFORMATIONAL layer: estimated markup never joins
-- the ledger or any realised P&L figure, so nothing here references the
-- accounting tables and nothing in them references these.

CREATE TABLE market_submissions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    submitted_at REAL NOT NULL,           -- epoch seconds
    source       TEXT NOT NULL,           -- 'paste' (room for future feeds)
    item_count   INTEGER NOT NULL
);

CREATE TABLE market_observations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id INTEGER NOT NULL REFERENCES market_submissions(id),
    item_name     TEXT NOT NULL,
    tier          INTEGER NOT NULL,
    horizon       TEXT NOT NULL,          -- 'day'|'week'|'month'|'year'|'decade'
    markup_pct    REAL,                   -- NULL = the game reported N/A
    sales_ped     REAL NOT NULL
);

-- The overview and history reads filter by item (and horizon) and order
-- by submission; the submission index serves the commit-time fanout and
-- any future submission-scoped delete.
CREATE INDEX idx_market_observations_item
    ON market_observations(item_name, horizon, submission_id);
CREATE INDEX idx_market_observations_submission
    ON market_observations(submission_id);
