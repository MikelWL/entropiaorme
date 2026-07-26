//! The analytics family: the Overview and Activity aggregates, the ledger
//! (keyset-paginated list + create/delete), the ledger presets, and the
//! inventory ledger (list / create / patch / delete / sell).
//!
//! The computation lives in [`eo_services::analytics::AnalyticsService`]
//! (shared with the guide-mode demo surface); this facade is the typed
//! boundary over it. The service returns typed aggregates and rows, and
//! the facade maps them field by field onto the declared DTOs, so the
//! wire shape is single-sourced here and the mapping is compiler-checked.
//!
//! One contract movement rides this migration, ratified under ADR-0019:
//! the Overview's numeric fields are typed `f64`, so the pydantic-era
//! `Any`-passthrough integers (the empty-window `cycledBreakdown` zeros
//! and any all-integer bucket) render as JSON floats (`0` -> `0.0`).
//! Numerically identical, and the values are floats over any non-empty
//! window (so the demo surface and every populated read are byte-stable);
//! only the all-empty case shifts.
//!
//! The ledger list folds the transport's `X-Next-Cursor` header into the
//! return DTO ([`LedgerPage`]): a typed command answers one structured
//! payload, so the cursor travels in the body, not a header. The delete
//! operations return no body (the transport's `{"status":"deleted"}`
//! acknowledgement retires with no consumer).

use std::collections::BTreeMap;

use eo_services::analytics::AnalyticsError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Nullable;
use crate::{Api, ApiError};

// ── Overview response DTOs ──────────────────────────────────────────

/// The liquid + progression returns breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReturnsBreakdown {
    pub loot_tt: f64,
    pub pes: f64,
    pub codex_pes: f64,
    pub quest_pes: f64,
    pub ledger: BTreeMap<String, f64>,
}

/// The per-family cycled-cost split.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CycledBreakdown {
    pub weapon: f64,
    pub healing: f64,
    pub enhancer: f64,
    pub armour: f64,
    pub dangling: f64,
}

/// The losses breakdown: tracking cost, its cycled split, and the ledger
/// expenses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LossesBreakdown {
    pub tracking_cost: f64,
    pub cycled_breakdown: CycledBreakdown,
    pub ledger: BTreeMap<String, f64>,
}

/// One day of the Overview timeline.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDay {
    pub date: String,
    pub loot_tt: f64,
    pub pes: f64,
    pub codex_pes: f64,
    pub quest_pes: f64,
    pub ledger_gains: BTreeMap<String, f64>,
    pub tracking_cost: f64,
    pub ledger_losses: BTreeMap<String, f64>,
}

/// One month of the Overview monthly breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyEntry {
    pub month: String,
    pub loot_tt: f64,
    pub pes: f64,
    pub codex_pes: f64,
    pub quest_pes: f64,
    pub ledger_gains: BTreeMap<String, f64>,
    pub tracking_cost: f64,
    pub ledger_losses: BTreeMap<String, f64>,
}

/// The Overview headline trend, as the service computes it. The
/// serialised forms are byte-identical to the strings they replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Improving,
    Declining,
    Stable,
}

/// A ledger entry's accounting class. Writes validate to exactly these
/// two values; the read side classifies anything else as an expense,
/// mirroring the binary styling check the frontend has always applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LedgerKind {
    Expense,
    Markup,
}

impl LedgerKind {
    fn classify(kind: &str) -> Self {
        if kind == "markup" {
            Self::Markup
        } else {
            Self::Expense
        }
    }
}

/// The Overview aggregate: the total return rate and trend, the returns /
/// losses breakdowns, the totals, and the day / month timelines.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverview {
    pub total_return_rate: f64,
    pub trend: Trend,
    pub returns_breakdown: ReturnsBreakdown,
    pub losses_breakdown: LossesBreakdown,
    pub total_gains: f64,
    pub total_losses: f64,
    pub timeline: Vec<TimelineDay>,
    pub monthly_breakdown: Vec<MonthlyEntry>,
}

// ── Activity response DTOs ──────────────────────────────────────────

/// One row of the per-mob activity comparison.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MobComparison {
    pub mob_name: String,
    pub sessions: i64,
    pub kills: i64,
    pub hours: f64,
    pub cycled: f64,
    pub pes_per100_ped: f64,
    pub loot_rate: f64,
}

/// One row of the per-tag activity comparison.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagComparison {
    pub tag_name: String,
    pub sessions: i64,
    pub kills: i64,
    pub hours: f64,
    pub cycled: f64,
    pub pes_per100_ped: f64,
    pub loot_rate: f64,
}

/// The Hunting aggregate: the per-mob and per-tag comparison tables.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsHunting {
    pub mob_comparisons: Vec<MobComparison>,
    pub tag_comparisons: Vec<TagComparison>,
}

/// One item in a tool's harvest loot composition: realised TT only.
/// The market markup column is merged in at the frontend from the
/// market layer, never joined into this accounting DTO.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestLootItem {
    pub item_name: String,
    pub quantity: i64,
    pub value_ped: f64,
}

/// Tree Cutting's durable source-activity vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarvestYieldTier {
    Short,
    Long,
    Huge,
    Unknown,
}

impl From<eo_services::harvest_yield::HarvestYieldTier> for HarvestYieldTier {
    fn from(value: eo_services::harvest_yield::HarvestYieldTier) -> Self {
        match value {
            eo_services::harvest_yield::HarvestYieldTier::Short => Self::Short,
            eo_services::harvest_yield::HarvestYieldTier::Long => Self::Long,
            eo_services::harvest_yield::HarvestYieldTier::Huge => Self::Huge,
            eo_services::harvest_yield::HarvestYieldTier::Unknown => Self::Unknown,
        }
    }
}

/// One tool strategy inside a yield tier.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestToolComparison {
    pub tool_name: Option<String>,
    pub swings: i64,
    pub cycled: f64,
    pub returns: f64,
    pub loot_rate: f64,
    pub loot_items: Vec<HarvestLootItem>,
}

/// One effective yield activity and its nested tool strategies.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestTierComparison {
    pub yield_tier: HarvestYieldTier,
    pub swings: i64,
    pub cycled: f64,
    pub returns: f64,
    pub loot_rate: f64,
    pub loot_items: Vec<HarvestLootItem>,
    pub tool_comparisons: Vec<HarvestToolComparison>,
}

/// The Tree Cutting aggregate: the tier-first comparison table.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsHarvest {
    pub tier_comparisons: Vec<HarvestTierComparison>,
}

// ── Ledger / preset / inventory DTOs ────────────────────────────────

/// One ledger entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LedgerItem {
    pub id: String,
    pub date: String,
    #[serde(rename = "type")]
    pub kind: LedgerKind,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// The whole-ledger summary for a period: the per-tag markup (gain) and
/// expense (loss) totals over every entry in the window, independent of
/// the paginated list. The Ledger tab's net-impact and source cards read
/// this instead of folding the loaded page window.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerSummary {
    pub gains: BTreeMap<String, f64>,
    pub losses: BTreeMap<String, f64>,
}

/// A page of ledger entries plus the opaque cursor for the next page
/// (`null` on the last page) and the whole-ledger row count, so a pager
/// can report true bounds while loading windows on demand: the keyset
/// `X-Next-Cursor` header folded into the typed return, since a command
/// answers one structured payload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPage {
    pub entries: Vec<LedgerItem>,
    pub next_cursor: Nullable<String>,
    pub total: i64,
}

/// One ledger preset (a reusable ledger-entry template).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LedgerPreset {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: LedgerKind,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// One inventory item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    pub id: String,
    pub name: String,
    pub tt_value: f64,
    pub markup_paid: f64,
    pub notes: Nullable<String>,
    pub acquired_at: String,
}

/// The result of selling an inventory item: the emitted ledger entry
/// (`null` for a zero-delta sale) and the sold item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventorySellResult {
    pub ledger_entry: Nullable<LedgerItem>,
    pub sold_item: InventoryItem,
}

/// One item's harvest-stock removed overlay: how much of the recorded
/// harvest loot has already left the player's holdings. Current position =
/// recorded looted quantity minus this. Position context only: it never
/// feeds market opportunity or its confidence levels, which stay
/// holding-independent, and never the recorded activity stats or the ledger.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestStockRemoval {
    pub item_name: String,
    pub removed_qty: i64,
}

// ── Request DTOs ────────────────────────────────────────────────────

/// A harvest-stock removed-overlay write payload.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestStockInput {
    pub item_name: String,
    pub removed_qty: i64,
}

/// A ledger-entry create payload.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LedgerEntryInput {
    pub date: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// A ledger-preset create payload.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LedgerPresetInput {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// An inventory-item create payload (snake_case, matching the frontend
/// request shape). `notes` / `acquired_at` are optional; an empty / absent
/// `acquired_at` defaults to today's UTC date.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InventoryItemInput {
    pub name: String,
    pub tt_value: f64,
    pub markup_paid: f64,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub acquired_at: Option<String>,
}

/// An inventory-item patch: only present (`Some`) fields update.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InventoryPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tt_value: Option<f64>,
    #[serde(default)]
    pub markup_paid: Option<f64>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// An inventory-sale payload.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct InventorySellInput {
    pub sale_price: f64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sold_at: Option<String>,
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// The Overview aggregate for a named period (`30d` / `90d` / `1y`, or
    /// all-time for any other value).
    pub async fn analytics_overview(&self, period: &str) -> Result<AnalyticsOverview, ApiError> {
        let value = self
            .analytics
            .overview(period)
            .await
            .map_err(analytics_error("analytics overview"))?;
        Ok(overview_dto(value))
    }

    /// The Hunting aggregate: the per-mob / per-tag tables.
    pub async fn analytics_hunting(&self) -> Result<AnalyticsHunting, ApiError> {
        let value = self
            .analytics
            .hunting()
            .await
            .map_err(analytics_error("analytics hunting"))?;
        Ok(hunting_dto(value))
    }

    /// The Tree Cutting aggregate for a named period: effective yield
    /// tiers, each with its tool strategies and matching loot composition.
    pub async fn analytics_harvest(&self, period: &str) -> Result<AnalyticsHarvest, ApiError> {
        let value = self
            .analytics
            .harvest(period)
            .await
            .map_err(analytics_error("analytics harvest"))?;
        Ok(harvest_dto(value))
    }

    /// The harvest-stock removed overlay (per-item quantity already sold or
    /// spent). Operational position context for sale and recycling actions;
    /// it does not influence holding-independent market opportunity.
    pub async fn harvest_stock(&self) -> Result<Vec<HarvestStockRemoval>, ApiError> {
        let rows = self
            .analytics
            .harvest_stock_removed()
            .await
            .map_err(analytics_error("harvest stock"))?;
        Ok(rows
            .into_iter()
            .map(|r| HarvestStockRemoval {
                item_name: r.item_name,
                removed_qty: r.removed_qty,
            })
            .collect())
    }

    /// Set an item's removed quantity (zero clears it). Writes the
    /// market-position lever alone: no activity stats, no ledger.
    pub async fn harvest_stock_set(&self, input: HarvestStockInput) -> Result<(), ApiError> {
        // Zero clears the overlay row; a negative quantity is meaningless and
        // would clear it just as silently, so it is a bad-request instead.
        if input.removed_qty < 0 {
            return Err(ApiError::bad_request(
                "removed quantity must not be negative",
            ));
        }
        self.analytics
            .set_harvest_stock_removed(&input.item_name, input.removed_qty)
            .await
            .map_err(analytics_error("harvest stock set"))?;
        Ok(())
    }

    /// One keyset page of ledger entries (newest first) plus the cursor for
    /// the next page. A malformed cursor is a bad-request.
    pub async fn ledger_list(
        &self,
        cursor: Option<String>,
        limit: Option<i64>,
    ) -> Result<LedgerPage, ApiError> {
        let page = self
            .analytics
            .list_ledger(cursor.as_deref(), limit)
            .await
            .map_err(analytics_error("ledger list"))?;
        Ok(LedgerPage {
            entries: page.entries.into_iter().map(ledger_item_dto).collect(),
            next_cursor: page.next_cursor.into(),
            total: page.total,
        })
    }

    /// The whole-ledger per-tag summary for a named period (`30d` / `90d` /
    /// `1y`, or all-time for any other value).
    pub async fn ledger_summary(&self, period: &str) -> Result<LedgerSummary, ApiError> {
        let summary = self
            .analytics
            .ledger_summary(period)
            .await
            .map_err(analytics_error("ledger summary"))?;
        Ok(LedgerSummary {
            gains: summary.gains,
            losses: summary.losses,
        })
    }

    /// Create a ledger entry (relanding its day's rollup).
    pub async fn ledger_create(&self, entry: LedgerEntryInput) -> Result<LedgerItem, ApiError> {
        let value = self
            .analytics
            .create_ledger_entry(
                &entry.date,
                &entry.kind,
                &entry.description,
                entry.amount,
                &entry.tag,
            )
            .await
            .map_err(analytics_error("ledger create"))?;
        Ok(ledger_item_dto(value))
    }

    /// Delete a ledger entry; a missing entry is a not-found.
    pub async fn ledger_delete(&self, entry_id: String) -> Result<(), ApiError> {
        match self
            .analytics
            .delete_ledger_entry(&entry_id)
            .await
            .map_err(analytics_error("ledger delete"))?
        {
            true => Ok(()),
            false => Err(ApiError::not_found("Entry not found")),
        }
    }

    /// The ledger presets.
    pub async fn ledger_presets_list(&self) -> Result<Vec<LedgerPreset>, ApiError> {
        let rows = self
            .analytics
            .list_ledger_presets()
            .await
            .map_err(analytics_error("ledger presets list"))?;
        Ok(rows.into_iter().map(ledger_preset_dto).collect())
    }

    /// Create a ledger preset; an invalid type is a bad-request.
    pub async fn ledger_preset_create(
        &self,
        preset: LedgerPresetInput,
    ) -> Result<LedgerPreset, ApiError> {
        let value = self
            .analytics
            .create_ledger_preset(
                &preset.name,
                &preset.kind,
                &preset.description,
                preset.amount,
                &preset.tag,
            )
            .await
            .map_err(analytics_error("ledger preset create"))?;
        Ok(ledger_preset_dto(value))
    }

    /// Delete a ledger preset; a missing preset is a not-found.
    pub async fn ledger_preset_delete(&self, preset_id: String) -> Result<(), ApiError> {
        match self
            .analytics
            .delete_ledger_preset(&preset_id)
            .await
            .map_err(analytics_error("ledger preset delete"))?
        {
            true => Ok(()),
            false => Err(ApiError::not_found("Preset not found")),
        }
    }

    /// The inventory items, newest acquisition first.
    pub async fn inventory_list(&self) -> Result<Vec<InventoryItem>, ApiError> {
        let rows = self
            .analytics
            .list_inventory()
            .await
            .map_err(analytics_error("inventory list"))?;
        Ok(rows.into_iter().map(inventory_item_dto).collect())
    }

    /// Create an inventory item.
    pub async fn inventory_create(
        &self,
        item: InventoryItemInput,
    ) -> Result<InventoryItem, ApiError> {
        let value = self
            .analytics
            .create_inventory_item(
                &item.name,
                item.tt_value,
                item.markup_paid,
                item.notes.as_deref(),
                item.acquired_at.as_deref(),
            )
            .await
            .map_err(analytics_error("inventory create"))?;
        Ok(inventory_item_dto(value))
    }

    /// Update an inventory item; a missing item is a not-found.
    pub async fn inventory_update(
        &self,
        item_id: String,
        patch: InventoryPatch,
    ) -> Result<InventoryItem, ApiError> {
        match self
            .analytics
            .update_inventory_item(
                &item_id,
                patch.name.as_deref(),
                patch.tt_value,
                patch.markup_paid,
                patch.notes.as_deref(),
            )
            .await
            .map_err(analytics_error("inventory update"))?
        {
            Some(value) => Ok(inventory_item_dto(value)),
            None => Err(ApiError::not_found("Inventory item not found")),
        }
    }

    /// Delete an inventory item; a missing item is a not-found.
    pub async fn inventory_delete(&self, item_id: String) -> Result<(), ApiError> {
        match self
            .analytics
            .delete_inventory_item(&item_id)
            .await
            .map_err(analytics_error("inventory delete"))?
        {
            true => Ok(()),
            false => Err(ApiError::not_found("Inventory item not found")),
        }
    }

    /// Sell an inventory item (emit the realised delta to the ledger and
    /// remove the row); a missing item is a not-found.
    pub async fn inventory_sell(
        &self,
        item_id: String,
        sale: InventorySellInput,
    ) -> Result<InventorySellResult, ApiError> {
        match self
            .analytics
            .sell_inventory_item(
                &item_id,
                sale.sale_price,
                sale.description.as_deref(),
                sale.sold_at.as_deref(),
            )
            .await
            .map_err(analytics_error("inventory sell"))?
        {
            Some(sale) => Ok(InventorySellResult {
                ledger_entry: sale.ledger_entry.map(ledger_item_dto).into(),
                sold_item: inventory_item_dto(sale.sold_item),
            }),
            None => Err(ApiError::not_found("Inventory item not found")),
        }
    }
}

// ── Service-row to DTO mapping ──────────────────────────────────────

pub(crate) fn overview_dto(data: eo_services::analytics::OverviewData) -> AnalyticsOverview {
    AnalyticsOverview {
        total_return_rate: data.total_return_rate,
        // The service emits exactly these three; its own fallback is stable.
        trend: match data.trend {
            "improving" => Trend::Improving,
            "declining" => Trend::Declining,
            _ => Trend::Stable,
        },
        returns_breakdown: ReturnsBreakdown {
            loot_tt: data.returns_breakdown.loot_tt,
            pes: data.returns_breakdown.pes,
            codex_pes: data.returns_breakdown.codex_pes,
            quest_pes: data.returns_breakdown.quest_pes,
            ledger: data.returns_breakdown.ledger,
        },
        losses_breakdown: LossesBreakdown {
            tracking_cost: data.losses_breakdown.tracking_cost,
            cycled_breakdown: CycledBreakdown {
                weapon: data.losses_breakdown.cycled_breakdown.weapon.as_f64(),
                healing: data.losses_breakdown.cycled_breakdown.healing.as_f64(),
                enhancer: data.losses_breakdown.cycled_breakdown.enhancer.as_f64(),
                armour: data.losses_breakdown.cycled_breakdown.armour.as_f64(),
                dangling: data.losses_breakdown.cycled_breakdown.dangling.as_f64(),
            },
            ledger: data.losses_breakdown.ledger,
        },
        total_gains: data.total_gains,
        total_losses: data.total_losses,
        timeline: data
            .timeline
            .into_iter()
            .map(|point| TimelineDay {
                date: point.bucket,
                loot_tt: point.loot_tt,
                pes: point.pes,
                codex_pes: point.codex_pes,
                quest_pes: point.quest_pes,
                ledger_gains: point.ledger_gains,
                tracking_cost: point.tracking_cost,
                ledger_losses: point.ledger_losses,
            })
            .collect(),
        monthly_breakdown: data
            .monthly_breakdown
            .into_iter()
            .map(|point| MonthlyEntry {
                month: point.bucket,
                loot_tt: point.loot_tt,
                pes: point.pes,
                codex_pes: point.codex_pes,
                quest_pes: point.quest_pes,
                ledger_gains: point.ledger_gains,
                tracking_cost: point.tracking_cost,
                ledger_losses: point.ledger_losses,
            })
            .collect(),
    }
}

pub(crate) fn hunting_dto(data: eo_services::analytics::HuntingData) -> AnalyticsHunting {
    AnalyticsHunting {
        mob_comparisons: data
            .mob_comparisons
            .into_iter()
            .map(|row| MobComparison {
                mob_name: row.name,
                sessions: row.sessions,
                kills: row.kills,
                hours: row.hours,
                cycled: row.cycled,
                pes_per100_ped: row.pes_per100_ped,
                loot_rate: row.loot_rate,
            })
            .collect(),
        tag_comparisons: data
            .tag_comparisons
            .into_iter()
            .map(|row| TagComparison {
                tag_name: row.name,
                sessions: row.sessions,
                kills: row.kills,
                hours: row.hours,
                cycled: row.cycled,
                pes_per100_ped: row.pes_per100_ped,
                loot_rate: row.loot_rate,
            })
            .collect(),
    }
}

pub(crate) fn harvest_dto(data: eo_services::analytics::HarvestData) -> AnalyticsHarvest {
    AnalyticsHarvest {
        tier_comparisons: data
            .tier_comparisons
            .into_iter()
            .map(|row| HarvestTierComparison {
                yield_tier: row.yield_tier.into(),
                swings: row.swings,
                cycled: row.cycled,
                returns: row.returns,
                loot_rate: row.loot_rate,
                loot_items: row
                    .loot_items
                    .into_iter()
                    .map(|item| HarvestLootItem {
                        item_name: item.item_name,
                        quantity: item.quantity,
                        value_ped: item.value_ped,
                    })
                    .collect(),
                tool_comparisons: row
                    .tool_comparisons
                    .into_iter()
                    .map(|tool| HarvestToolComparison {
                        tool_name: tool.name,
                        swings: tool.swings,
                        cycled: tool.cycled,
                        returns: tool.returns,
                        loot_rate: tool.loot_rate,
                        loot_items: tool
                            .loot_items
                            .into_iter()
                            .map(|item| HarvestLootItem {
                                item_name: item.item_name,
                                quantity: item.quantity,
                                value_ped: item.value_ped,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub(crate) fn ledger_item_dto(row: eo_services::analytics::LedgerRow) -> LedgerItem {
    LedgerItem {
        id: row.id,
        date: row.date,
        kind: LedgerKind::classify(&row.kind),
        description: row.description,
        amount: row.amount,
        tag: row.tag,
    }
}

pub(crate) fn ledger_preset_dto(row: eo_services::analytics::PresetRow) -> LedgerPreset {
    LedgerPreset {
        id: row.id,
        name: row.name,
        kind: LedgerKind::classify(&row.kind),
        description: row.description,
        amount: row.amount,
        tag: row.tag,
    }
}

pub(crate) fn inventory_item_dto(row: eo_services::analytics::InventoryRow) -> InventoryItem {
    InventoryItem {
        id: row.id,
        name: row.name,
        tt_value: row.tt_value,
        markup_paid: row.markup_paid,
        notes: row.notes.into(),
        acquired_at: row.acquired_at,
    }
}

/// Map the analytics service's error surface onto the IPC error contract:
/// the two validation variants become bad-requests carrying their verbatim
/// message; a driver / rollup failure collapses to the internal error,
/// logged server-side under `context`.
pub(crate) fn analytics_error(context: &'static str) -> impl FnOnce(AnalyticsError) -> ApiError {
    move |err| match err {
        AnalyticsError::InvalidCursor => ApiError::bad_request("Invalid cursor"),
        AnalyticsError::InvalidPresetType => {
            ApiError::bad_request("type must be 'expense' or 'markup'")
        }
        source => ApiError::internal(context)(source),
    }
}
