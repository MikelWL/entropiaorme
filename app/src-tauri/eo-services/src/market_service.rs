//! The market domain service: the manual markup-observation feed and
//! its reads.
//!
//! The user pastes the game's market-ledger export ([`crate::market_paste`]);
//! an accepted paste commits as one submission whose observations carry
//! the five aggregation horizons per item. Reads serve the overview
//! (each item's latest readings, with when they were observed) and a
//! per-item, per-horizon history.
//!
//! Everything here is the INFORMATIONAL market layer: estimated markup
//! never joins the ledger, the analytics aggregates, or any realised
//! P&L figure. This service touches only the `market_*` tables and
//! deliberately has no view into the accounting ones.

use std::sync::Arc;

use crate::clock::Clock;
use crate::db::{Db, DbError};
use crate::market_paste::{parse_market_paste, MarketHorizon, MarketPasteParse, MarketReading};
use crate::time::naive_to_epoch;

/// The market domain service over the shared database and injected
/// clock.
pub struct MarketService {
    db: Db,
    clock: Arc<dyn Clock>,
}

#[derive(Debug, thiserror::Error)]
pub enum MarketError {
    /// The paste yielded no usable rows, so there is nothing to commit.
    #[error("the paste contained no readable market rows")]
    EmptyPaste,
    #[error(transparent)]
    Db(#[from] DbError),
}

/// A committed paste: the submission it created and the timestamp its
/// observations carry.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitOutcome {
    pub submission_id: i64,
    pub item_count: usize,
    pub skipped_count: usize,
    /// Epoch seconds.
    pub observed_at: f64,
}

/// One overview row: an item's latest readings (all five horizons from
/// the most recent submission that carried the item) and when they were
/// observed.
#[derive(Debug, Clone, PartialEq)]
pub struct OverviewRow {
    pub item_name: String,
    pub tier: i64,
    /// Epoch seconds of the submission the readings came from.
    pub observed_at: f64,
    /// Indexed in [`MarketHorizon::ALL`] order.
    pub readings: [MarketReading; 5],
}

/// One history point of an item's horizon: the observation and when it
/// was submitted.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryPoint {
    /// Epoch seconds.
    pub observed_at: f64,
    pub markup_pct: Option<f64>,
    pub sales_ped: f64,
}

/// The modelled TT-return rate (percent) of a hunting loadout: the
/// community returns model, roughly linear in weapon efficiency and
/// looter profession level (86% baseline, ~7pp each across 0-100).
/// A MODELLED ESTIMATE with an error bar of about one percentage point
/// against observed loot-only returns; never present it as a measured
/// figure, and never let it near a realised rate.
pub fn modelled_tt_return_pct(efficiency_pct: f64, looter_level: f64) -> f64 {
    86.0 + 7.0 * (efficiency_pct / 100.0) + 7.0 * (looter_level / 100.0)
}

/// The overall loot markup (percent premium over TT) an activity needs
/// to break even at the given modelled TT-return rate: mu* = 1/R - 1.
pub fn break_even_markup_pct(tt_return_pct: f64) -> f64 {
    (100.0 / tt_return_pct - 1.0) * 100.0
}

impl MarketService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }

    /// Parse a paste without touching the database (the preview leg of
    /// the review-before-accept flow).
    pub fn preview(&self, text: &str) -> MarketPasteParse {
        parse_market_paste(text)
    }

    /// Parse and commit a paste as one submission. The text is
    /// re-parsed here rather than trusting a client echo of the
    /// preview, so the stored rows always come from this parser.
    pub async fn commit_paste(&self, text: &str) -> Result<CommitOutcome, MarketError> {
        let parse = parse_market_paste(text);
        if parse.rows.is_empty() {
            return Err(MarketError::EmptyPaste);
        }
        let observed_at = naive_to_epoch(self.clock.now());
        let item_count = parse.rows.len();
        let skipped_count = parse.skipped.len();
        let rows = parse.rows;
        let submission_id = self
            .db
            .with_writer(move |connection| {
                let tx = connection.transaction()?;
                tx.execute(
                    "INSERT INTO market_submissions (submitted_at, source, item_count) \
                     VALUES (?1, 'paste', ?2)",
                    rusqlite::params![observed_at, item_count as i64],
                )?;
                let submission_id = tx.last_insert_rowid();
                {
                    let mut insert = tx.prepare(
                        "INSERT INTO market_observations \
                         (submission_id, item_name, tier, horizon, markup_pct, sales_ped) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )?;
                    for row in &rows {
                        for (horizon, reading) in MarketHorizon::ALL.iter().zip(&row.readings) {
                            insert.execute(rusqlite::params![
                                submission_id,
                                row.item_name,
                                row.tier,
                                horizon.as_str(),
                                reading.markup_pct,
                                reading.sales_ped,
                            ])?;
                        }
                    }
                }
                tx.commit()?;
                Ok(submission_id)
            })
            .await?;
        Ok(CommitOutcome {
            submission_id,
            item_count,
            skipped_count,
            observed_at,
        })
    }

    /// Every observed item's latest readings: the five horizons from
    /// the most recent submission that carried the item, sorted by item
    /// name.
    pub async fn overview(&self) -> Result<Vec<OverviewRow>, DbError> {
        self.db
            .with_reader(|connection| {
                let mut stmt = connection.prepare(
                    "SELECT o.item_name, o.tier, s.submitted_at, o.horizon, \
                            o.markup_pct, o.sales_ped \
                     FROM market_observations o \
                     JOIN market_submissions s ON s.id = o.submission_id \
                     WHERE o.submission_id = (SELECT MAX(o2.submission_id) \
                                              FROM market_observations o2 \
                                              WHERE o2.item_name = o.item_name) \
                     ORDER BY o.item_name, o.id",
                )?;
                let mut rows = stmt.query([])?;
                let mut overview: Vec<OverviewRow> = Vec::new();
                while let Some(row) = rows.next()? {
                    let item_name: String = row.get(0)?;
                    let horizon: String = row.get(3)?;
                    let Some(horizon) = MarketHorizon::from_stored(&horizon) else {
                        // A vocabulary value outside the enum never gets
                        // written; tolerate rather than fail the read.
                        continue;
                    };
                    if overview.last().map(|entry| entry.item_name.as_str())
                        != Some(item_name.as_str())
                    {
                        overview.push(OverviewRow {
                            item_name,
                            tier: row.get(1)?,
                            observed_at: row.get(2)?,
                            readings: [MarketReading {
                                markup_pct: None,
                                sales_ped: 0.0,
                            }; 5],
                        });
                    }
                    let entry = overview.last_mut().expect("pushed above");
                    entry.readings[horizon as usize] = MarketReading {
                        markup_pct: row.get(4)?,
                        sales_ped: row.get(5)?,
                    };
                }
                Ok(overview)
            })
            .await
    }

    /// One item's observations over time on one horizon, oldest first.
    pub async fn item_history(
        &self,
        item_name: &str,
        horizon: MarketHorizon,
    ) -> Result<Vec<HistoryPoint>, DbError> {
        let item_name = item_name.to_string();
        self.db
            .with_reader(move |connection| {
                let mut stmt = connection.prepare(
                    "SELECT s.submitted_at, o.markup_pct, o.sales_ped \
                     FROM market_observations o \
                     JOIN market_submissions s ON s.id = o.submission_id \
                     WHERE o.item_name = ?1 AND o.horizon = ?2 \
                     ORDER BY o.submission_id",
                )?;
                let points = stmt
                    .query_map(rusqlite::params![item_name, horizon.as_str()], |row| {
                        Ok(HistoryPoint {
                            observed_at: row.get(0)?,
                            markup_pct: row.get(1)?,
                            sales_ped: row.get(2)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(points)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;

    #[test]
    fn the_returns_model_matches_its_calibration_points() {
        // The calibration sanity check: ~55%-efficiency weapons at looter
        // ~37 predicted ~92.4%, observed loot-only returns within ~1pp.
        let r = modelled_tt_return_pct(55.0, 37.0);
        assert!((r - 92.44).abs() < 0.01, "got {r}");
        // The model endpoints: 86 baseline, 100 at both maxima.
        assert_eq!(modelled_tt_return_pct(0.0, 0.0), 86.0);
        assert_eq!(modelled_tt_return_pct(100.0, 100.0), 100.0);
    }

    #[test]
    fn break_even_markup_inverts_the_return_rate() {
        // mu* = 1/R - 1: a 92.44% modelled return needs ~8.18% markup.
        let mu = break_even_markup_pct(92.44);
        assert!((mu - 8.178).abs() < 0.01, "got {mu}");
        // A 100% return breaks even with no markup at all.
        assert_eq!(break_even_markup_pct(100.0), 0.0);
    }

    const SAMPLE: &str = "Carabok Hide\t0\t106.880%\t451.900 PED\t107.160%\t531.900 PED\t\
106.020%\t979.040 PED\t108.280%\t13.500K PED\t158.920%\t35.300K PED\n\
Carabok Leg Fur\t0\tN/A\t0.000 PEC\tN/A\t0.000 PEC\tN/A\t0.000 PEC\t109.380%\t6.400 PED\t\
339.100%\t375.400 PED";

    fn rig(dir: &std::path::Path) -> (tokio::runtime::Runtime, MarketService, Arc<MockClock>) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let db = runtime
            .block_on(Db::open(&dir.join("entropia_orme.db")))
            .unwrap();
        let clock = Arc::new(MockClock::new(
            Some(
                chrono::NaiveDateTime::parse_from_str("2026-07-11 12:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
            ),
            0.0,
        ));
        let service = MarketService::new(db, clock.clone());
        (runtime, service, clock)
    }

    #[test]
    fn commit_then_overview_round_trips_the_readings() {
        let dir = tempfile::tempdir().unwrap();
        let (runtime, service, _clock) = rig(dir.path());

        let outcome = runtime.block_on(service.commit_paste(SAMPLE)).unwrap();
        assert_eq!(outcome.item_count, 2);
        assert_eq!(outcome.skipped_count, 0);

        let overview = runtime.block_on(service.overview()).unwrap();
        assert_eq!(overview.len(), 2);
        // Sorted by item name; readings land on their horizons.
        assert_eq!(overview[0].item_name, "Carabok Hide");
        assert_eq!(overview[0].readings[0].markup_pct, Some(106.880));
        assert_eq!(overview[0].readings[4].sales_ped, 35_300.0);
        assert_eq!(overview[1].item_name, "Carabok Leg Fur");
        assert_eq!(overview[1].readings[0].markup_pct, None);
        assert_eq!(overview[1].readings[3].markup_pct, Some(109.380));
        assert_eq!(overview[0].observed_at, outcome.observed_at);
    }

    #[test]
    fn overview_serves_each_items_latest_submission() {
        let dir = tempfile::tempdir().unwrap();
        let (runtime, service, clock) = rig(dir.path());

        runtime.block_on(service.commit_paste(SAMPLE)).unwrap();
        clock.advance(7.0 * 24.0 * 3600.0).unwrap();
        // A week later only Carabok Hide is re-observed, cheaper.
        let later = "Carabok Hide\t0\t101.000%\t10.000 PED\t101.000%\t10.000 PED\t\
101.000%\t10.000 PED\t101.000%\t10.000 PED\t101.000%\t10.000 PED";
        let second = runtime.block_on(service.commit_paste(later)).unwrap();

        let overview = runtime.block_on(service.overview()).unwrap();
        assert_eq!(overview.len(), 2);
        // The re-observed item serves the new submission...
        assert_eq!(overview[0].item_name, "Carabok Hide");
        assert_eq!(overview[0].readings[0].markup_pct, Some(101.000));
        assert_eq!(overview[0].observed_at, second.observed_at);
        // ...while the unrefreshed one keeps its older readings and
        // (staleness-bearing) older timestamp.
        assert_eq!(overview[1].item_name, "Carabok Leg Fur");
        assert_eq!(overview[1].readings[4].markup_pct, Some(339.100));
        assert!(overview[1].observed_at < second.observed_at);
    }

    #[test]
    fn history_orders_one_horizon_over_time() {
        let dir = tempfile::tempdir().unwrap();
        let (runtime, service, clock) = rig(dir.path());

        runtime.block_on(service.commit_paste(SAMPLE)).unwrap();
        clock.advance(24.0 * 3600.0).unwrap();
        let later = "Carabok Hide\t0\t105.500%\t200.000 PED\t107.000%\t500.000 PED\t\
106.000%\t900.000 PED\t108.000%\t13.000K PED\t158.000%\t35.000K PED";
        runtime.block_on(service.commit_paste(later)).unwrap();

        let history = runtime
            .block_on(service.item_history("Carabok Hide", MarketHorizon::Day))
            .unwrap();
        assert_eq!(history.len(), 2);
        assert!(history[0].observed_at < history[1].observed_at);
        assert_eq!(history[0].markup_pct, Some(106.880));
        assert_eq!(history[1].markup_pct, Some(105.500));
        assert_eq!(history[1].sales_ped, 200.0);

        // A horizon the item never traded on still has its rows (N/A
        // markup as NULL), and an unknown item has none.
        let fur = runtime
            .block_on(service.item_history("Carabok Leg Fur", MarketHorizon::Day))
            .unwrap();
        assert_eq!(fur.len(), 1);
        assert_eq!(fur[0].markup_pct, None);
        let none = runtime
            .block_on(service.item_history("Unseen Item", MarketHorizon::Day))
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn an_unusable_paste_commits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (runtime, service, _clock) = rig(dir.path());

        let err = runtime
            .block_on(service.commit_paste("not market data\nat all"))
            .unwrap_err();
        assert!(matches!(err, MarketError::EmptyPaste));
        assert!(runtime.block_on(service.overview()).unwrap().is_empty());
    }
}
