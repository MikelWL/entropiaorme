//! The market family: the manual markup-observation feed (paste
//! preview and commit) and its reads (overview, per-item history).
//!
//! Everything on this surface is the INFORMATIONAL market layer:
//! estimated markup is a distinct data class that never joins the
//! ledger, the analytics aggregates, or any realised P&L figure. The
//! DTOs stay in this module so the accounting surfaces cannot consume
//! them by accident.

use eo_services::market_paste::{self, MarketPasteRow};
use eo_services::market_service::{self, MarketError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Nullable;
use crate::{Api, ApiError};

// ── Wire vocabulary ─────────────────────────────────────────────────

/// One of the export's five aggregation horizons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarketHorizon {
    Day,
    Week,
    Month,
    Year,
    Decade,
}

impl From<MarketHorizon> for market_paste::MarketHorizon {
    fn from(value: MarketHorizon) -> Self {
        match value {
            MarketHorizon::Day => Self::Day,
            MarketHorizon::Week => Self::Week,
            MarketHorizon::Month => Self::Month,
            MarketHorizon::Year => Self::Year,
            MarketHorizon::Decade => Self::Decade,
        }
    }
}

// ── Response DTOs ───────────────────────────────────────────────────

/// One horizon's reading: the markup percentage (null where the game
/// reported N/A, meaning no sales in that horizon) and the sales
/// volume in PED.
#[derive(Debug, Clone, Copy, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketReading {
    pub markup_pct: Nullable<f64>,
    pub sales_ped: f64,
}

impl From<market_paste::MarketReading> for MarketReading {
    fn from(value: market_paste::MarketReading) -> Self {
        Self {
            markup_pct: value.markup_pct.into(),
            sales_ped: value.sales_ped,
        }
    }
}

/// One parsed item row: the five horizon readings by name.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketPastePreviewRow {
    pub item_name: String,
    pub tier: i64,
    pub day: MarketReading,
    pub week: MarketReading,
    pub month: MarketReading,
    pub year: MarketReading,
    pub decade: MarketReading,
}

impl MarketPastePreviewRow {
    fn from_row(row: &MarketPasteRow) -> Self {
        let (day, week, month, year, decade) = horizon_fields(row.readings);
        Self {
            item_name: row.item_name.clone(),
            tier: row.tier,
            day,
            week,
            month,
            year,
            decade,
        }
    }
}

/// The five converted readings in horizon order, ready to land on the
/// named DTO fields; the one place the array-to-fields mapping lives,
/// so the preview and overview rows stay in lock-step.
fn horizon_fields(
    readings: [market_paste::MarketReading; 5],
) -> (
    MarketReading,
    MarketReading,
    MarketReading,
    MarketReading,
    MarketReading,
) {
    let [day, week, month, year, decade] = readings;
    (
        day.into(),
        week.into(),
        month.into(),
        year.into(),
        decade.into(),
    )
}

/// One line the parser could not use, for the review flow.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketSkippedLine {
    /// 1-based line number within the paste.
    pub line_number: i64,
    pub content: String,
    pub reason: String,
}

/// The parse preview: what a commit of the same text would store, and
/// what it would ignore.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketPastePreview {
    pub rows: Vec<MarketPastePreviewRow>,
    pub skipped: Vec<MarketSkippedLine>,
}

/// A committed paste: the submission it created.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketCommitResult {
    pub submission_id: i64,
    pub item_count: i64,
    pub skipped_count: i64,
    /// Epoch seconds.
    pub observed_at: f64,
}

/// One overview row: an item's latest readings (from the most recent
/// submission that carried it) and when they were observed.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketOverviewRow {
    pub item_name: String,
    pub tier: i64,
    /// Epoch seconds of the submission the readings came from (the
    /// staleness signal).
    pub observed_at: f64,
    pub day: MarketReading,
    pub week: MarketReading,
    pub month: MarketReading,
    pub year: MarketReading,
    pub decade: MarketReading,
}

/// One point of an item's per-horizon history, oldest first.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketHistoryPoint {
    /// Epoch seconds.
    pub observed_at: f64,
    pub markup_pct: Nullable<f64>,
    pub sales_ped: f64,
}

// ── Operations ──────────────────────────────────────────────────────

impl Api {
    /// Parse a market-ledger paste without storing anything: the
    /// preview leg of the review-before-accept flow.
    pub async fn market_paste_preview(&self, text: String) -> Result<MarketPastePreview, ApiError> {
        let parse = self.market.preview(&text);
        Ok(MarketPastePreview {
            rows: parse
                .rows
                .iter()
                .map(MarketPastePreviewRow::from_row)
                .collect(),
            skipped: parse
                .skipped
                .into_iter()
                .map(|line| MarketSkippedLine {
                    line_number: line.line_number as i64,
                    content: line.content,
                    reason: line.reason,
                })
                .collect(),
        })
    }

    /// Parse and commit a market-ledger paste as one submission. The
    /// text is re-parsed server-side; the preview is never trusted as
    /// an echo.
    pub async fn market_paste_commit(&self, text: String) -> Result<MarketCommitResult, ApiError> {
        let outcome = self
            .market
            .commit_paste(&text)
            .await
            .map_err(|err| match err {
                MarketError::EmptyPaste => ApiError::bad_request(err.to_string()),
                MarketError::Db(source) => ApiError::internal("market paste commit")(source),
            })?;
        Ok(MarketCommitResult {
            submission_id: outcome.submission_id,
            item_count: outcome.item_count as i64,
            skipped_count: outcome.skipped_count as i64,
            observed_at: outcome.observed_at,
        })
    }

    /// Every observed item's latest readings, sorted by item name.
    pub async fn market_overview(&self) -> Result<Vec<MarketOverviewRow>, ApiError> {
        let rows = self
            .market
            .overview()
            .await
            .map_err(ApiError::internal("market overview"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let (day, week, month, year, decade) = horizon_fields(row.readings);
                MarketOverviewRow {
                    item_name: row.item_name,
                    tier: row.tier,
                    observed_at: row.observed_at,
                    day,
                    week,
                    month,
                    year,
                    decade,
                }
            })
            .collect())
    }

    /// One item's observations over time on one horizon, oldest first.
    pub async fn market_item_history(
        &self,
        item_name: String,
        horizon: MarketHorizon,
    ) -> Result<Vec<MarketHistoryPoint>, ApiError> {
        let points: Vec<market_service::HistoryPoint> = self
            .market
            .item_history(&item_name, horizon.into())
            .await
            .map_err(ApiError::internal("market item history"))?;
        Ok(points
            .into_iter()
            .map(|point| MarketHistoryPoint {
                observed_at: point.observed_at,
                markup_pct: point.markup_pct.into(),
                sales_ped: point.sales_ped,
            })
            .collect())
    }
}
