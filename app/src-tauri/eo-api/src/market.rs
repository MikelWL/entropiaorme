//! The market family: the manual markup-observation feed (paste
//! preview and commit) and its reads (overview, per-item history).
//!
//! Everything on this surface is the INFORMATIONAL market layer:
//! estimated markup is a distinct data class that never joins the
//! ledger, the analytics aggregates, or any realised P&L figure. The
//! DTOs stay in this module so the accounting surfaces cannot consume
//! them by accident.

use eo_services::market_paste::{self, MarketPasteRow};
use eo_services::market_service::{
    self, break_even_markup_pct, modelled_tt_return_pct, MarketError,
};
use serde_json::Value;

use crate::equipment::EquipmentKind;
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

/// One item of a contributable batch: the pasted readings verbatim,
/// by horizon name.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketContributionItem {
    pub item_name: String,
    pub tier: i64,
    pub day: MarketReading,
    pub week: MarketReading,
    pub month: MarketReading,
    pub year: MarketReading,
    pub decade: MarketReading,
}

/// The most recent accepted paste as a contributable batch: exactly
/// what an opted-in contributor shares, nothing more. The frontend
/// owns the send, strictly behind the contribution opt-in.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketContributionBatch {
    /// Epoch seconds of the submission.
    pub observed_at: f64,
    pub items: Vec<MarketContributionItem>,
}

/// One species' estimated-markup row: recorded loot composition
/// TT-weighted by the latest markup observations on one horizon.
/// Coverage keeps a thin sample honest: the estimate weights only the
/// covered TT, and the row says how much that is.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketMobRankingRow {
    pub mob_species: String,
    pub loot_tt: f64,
    pub covered_tt: f64,
    pub item_count: i64,
    pub covered_item_count: i64,
    pub est_markup_pct: Nullable<f64>,
}

/// One item's resolved markup in a tool's breakdown: the markup and the
/// horizon it came from (week preferred, then month, then year), or null
/// when no observation covers the item.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketToolItemMarkup {
    pub item_name: String,
    pub markup_pct: Nullable<f64>,
    pub horizon: Nullable<String>,
}

/// One harvesting tool's estimated-markup row: its recorded loot
/// composition resolved against markup observations, with the per-item
/// markup breakdown. `mu_projected_returns` projects the whole pool
/// (covered items at their markup, uncovered floored at TT); the MU rate
/// is that over the realised cycled cost, derived at the frontend.
/// Estimated markup, informational only, never a realised figure.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketToolRankingRow {
    pub tool_name: String,
    pub loot_tt: f64,
    pub covered_tt: f64,
    pub mu_projected_returns: f64,
    pub items: Vec<MarketToolItemMarkup>,
}

/// One looter profession and its believed-current level.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketLooterLevel {
    pub name: String,
    pub level: f64,
}

/// One (weapon, looter) break-even cell: the modelled TT-return rate
/// and the overall loot markup that loadout needs to break even. Both
/// figures are MODELLED ESTIMATES (community returns model, roughly a
/// one-percentage-point error bar), never measured rates.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketBreakEvenCell {
    pub looter_name: String,
    pub tt_return_pct: f64,
    pub break_even_markup_pct: f64,
}

/// One library weapon's break-even row: its catalogue efficiency (null
/// when the bundled catalogue does not carry the weapon) and the cells
/// across the player's looter professions.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketWeaponBreakEven {
    pub name: String,
    pub efficiency_pct: Nullable<f64>,
    pub cells: Vec<MarketBreakEvenCell>,
}

/// The break-even readout: the player's looter professions and every
/// library weapon's modelled break-even markup against each of them.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketBreakEven {
    pub looters: Vec<MarketLooterLevel>,
    pub weapons: Vec<MarketWeaponBreakEven>,
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

    /// The most recent accepted paste as a contributable batch, or null
    /// before the first commit.
    pub async fn market_contribution_batch(
        &self,
    ) -> Result<Nullable<MarketContributionBatch>, ApiError> {
        let batch = self
            .market
            .latest_submission()
            .await
            .map_err(ApiError::internal("market contribution batch"))?;
        Ok(batch
            .map(|batch| MarketContributionBatch {
                observed_at: batch.observed_at,
                items: batch
                    .items
                    .into_iter()
                    .map(|item| {
                        let (day, week, month, year, decade) = horizon_fields(item.readings);
                        MarketContributionItem {
                            item_name: item.item_name,
                            tier: item.tier,
                            day,
                            week,
                            month,
                            year,
                            decade,
                        }
                    })
                    .collect(),
            })
            .into())
    }

    /// The break-even markup readout: for every weapon in the equipment
    /// library, the modelled TT-return rate and required break-even
    /// markup against each of the player's looter professions.
    /// Efficiency comes from the bundled game-data catalogue by exact
    /// name (null, with no cells, when the catalogue lacks the weapon);
    /// looter levels ride the same believed-current derivation as the
    /// character professions read.
    pub async fn market_break_even(&self) -> Result<MarketBreakEven, ApiError> {
        let looters: Vec<MarketLooterLevel> = self
            .character_professions()
            .await?
            .into_iter()
            .filter(|profession| profession.name.contains("Looter"))
            .map(|profession| MarketLooterLevel {
                name: profession.name,
                level: profession.level,
            })
            .collect();

        let catalogue = self.game_data.get_entities("weapons");
        let efficiency_of = |name: &str| -> Option<f64> {
            catalogue
                .iter()
                .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name))
                .and_then(|entry| entry.get("economy"))
                .and_then(|economy| economy.get("efficiency"))
                .and_then(Value::as_f64)
        };

        let weapons = self
            .equipment_library()
            .await?
            .into_iter()
            .filter(|item| matches!(item.kind, EquipmentKind::Weapon))
            .map(|item| {
                let efficiency = efficiency_of(&item.name);
                let cells = efficiency
                    .map(|efficiency_pct| {
                        looters
                            .iter()
                            .map(|looter| {
                                let tt_return_pct =
                                    modelled_tt_return_pct(efficiency_pct, looter.level);
                                MarketBreakEvenCell {
                                    looter_name: looter.name.clone(),
                                    tt_return_pct,
                                    break_even_markup_pct: break_even_markup_pct(tt_return_pct),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                MarketWeaponBreakEven {
                    name: item.name,
                    efficiency_pct: efficiency.into(),
                    cells,
                }
            })
            .collect();

        Ok(MarketBreakEven { looters, weapons })
    }

    /// Every hunted species' estimated loot markup on one horizon,
    /// best estimate first (species with no observed items last).
    pub async fn market_mob_ranking(
        &self,
        horizon: MarketHorizon,
    ) -> Result<Vec<MarketMobRankingRow>, ApiError> {
        let rows = self
            .market
            .mob_ranking(horizon.into())
            .await
            .map_err(ApiError::internal("market mob ranking"))?;
        Ok(rows
            .into_iter()
            .map(|row| MarketMobRankingRow {
                mob_species: row.mob_species,
                loot_tt: row.loot_tt,
                covered_tt: row.covered_tt,
                item_count: row.item_count,
                covered_item_count: row.covered_item_count,
                est_markup_pct: row.est_markup_pct.into(),
            })
            .collect())
    }

    /// Every harvesting tool's estimated loot markup and per-item markup
    /// breakdown, best projected return first. Each item's markup
    /// resolves via the week -> month -> year horizon fallback.
    pub async fn market_tool_ranking(&self) -> Result<Vec<MarketToolRankingRow>, ApiError> {
        let rows = self
            .market
            .tool_ranking()
            .await
            .map_err(ApiError::internal("market tool ranking"))?;
        Ok(rows
            .into_iter()
            .map(|row| MarketToolRankingRow {
                tool_name: row.tool_name,
                loot_tt: row.loot_tt,
                covered_tt: row.covered_tt,
                mu_projected_returns: row.mu_projected_returns,
                items: row
                    .items
                    .into_iter()
                    .map(|item| MarketToolItemMarkup {
                        item_name: item.item_name,
                        markup_pct: item.markup_pct.into(),
                        horizon: item.horizon.into(),
                    })
                    .collect(),
            })
            .collect())
    }
}
