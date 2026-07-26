//! The analytics domain service: the Overview and Activity aggregates, the
//! ledger (keyset-paginated list + create/delete), the ledger presets, and
//! the inventory ledger (list / create / patch / delete / sell).
//!
//! This is domain logic, extracted from the transport layer that used to
//! host it inline: the same SQL aggregation and camelCase-ready value
//! shaping, now behind [`AnalyticsService`] over the shared database and
//! injected clock, so both the typed IPC facade and the guide-mode demo
//! surface read through one implementation. Reads scale O(days), not
//! O(rows): the Overview brings the daily rollups current and aggregates
//! rollup rows plus bounded raw edges (see [`hybrid_window`]).
//!
//! The aggregates carry the engine's numeric typing internally as
//! [`SqlNumber`]: an empty `COALESCE(SUM(...), 0)` reads as the exact
//! integer `0`, integer sums stay exact, and rounding applies only to
//! floats. The response boundary declares `f64` fields, so every number
//! coerces to its float form exactly where the facade DTOs pin it.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::clock::Clock;
use crate::daily_rollup;
use crate::db::{Db, DbError};
use crate::harvest_yield::HarvestYieldTier;
use crate::stock_allocation;
use crate::time::naive_to_epoch;

/// The analytics domain service over the shared database and injected
/// clock: the Overview / Activity aggregates and the ledger / preset /
/// inventory CRUD.
pub struct AnalyticsService {
    db: Db,
    clock: Arc<dyn Clock>,
}

/// A page of ledger entries (newest first) plus the opaque cursor for the
/// following page (`None` on the last page) and the whole-table row count
/// (so a pager can report true bounds while loading windows on demand);
/// the cursor is the base64url keyset token.
pub struct LedgerPage {
    pub entries: Vec<LedgerRow>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

/// One ledger entry, in wire shape (`type` is the stored kind).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerRow {
    pub id: String,
    pub date: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// One ledger preset, in wire shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetRow {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub amount: f64,
    pub tag: String,
}

/// One inventory item, in wire shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryRow {
    pub id: String,
    pub name: String,
    pub tt_value: f64,
    pub markup_paid: f64,
    pub notes: Option<String>,
    pub acquired_at: String,
}

/// One canonical item's current position: recorded loot still held, after
/// everything that has left through a listing or a conversion and back
/// through an expiry. Position context only: it never feeds market
/// opportunity or its confidence levels, which stay holding-independent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StockPositionRow {
    pub item_name: String,
    pub quantity: f64,
    pub tt_value: f64,
    /// Quantity currently sitting in an unresolved auction listing. Already
    /// out of `quantity`: it has left the player's inventory and may still
    /// come back, so it is reported rather than silently absent.
    pub listed_quantity: f64,
}

/// One auction listing across its whole lifecycle.
///
/// Realised markup is derived on read from the listing's own numbers rather
/// than stored, so it cannot drift from the fees and price it was resolved
/// with. `None` until the listing is confirmed sold.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuctionListingRow {
    pub id: String,
    pub item_name: String,
    pub quantity: f64,
    pub attributed_qty: f64,
    pub unattributed_qty: f64,
    pub tt_value: f64,
    pub attributed_tt: f64,
    pub starting_bid: f64,
    pub buyout: Option<f64>,
    pub listing_fee: f64,
    pub listed_at: String,
    pub status: String,
    pub final_price: Option<f64>,
    pub sale_fee: Option<f64>,
    pub resolved_at: Option<String>,
    /// Net markup the activity may claim, after both auction fees and after
    /// removing the share covered by untracked stock.
    pub activity_net_markup: Option<f64>,
    /// Gross sale proceeds above the listing's TT, before fees.
    pub gross_markup: Option<f64>,
}

/// One tier's realised markup from confirmed sales, for the Tree Cutting
/// per-activity Realised figures.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealisedTierMarkup {
    pub yield_tier: HarvestYieldTier,
    pub net_markup: f64,
}

/// A realised inventory sale: the ledger entry it wrote (`None` for a
/// zero-delta sale) and the sold item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySale {
    pub ledger_entry: Option<LedgerRow>,
    pub sold_item: InventoryRow,
}

/// The Overview aggregate. Response numerics are in their float form
/// except the cycled split, which keeps its engine typing
/// ([`SqlNumber`]: an empty `COALESCE` sum is the exact integer zero).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewData {
    pub total_return_rate: f64,
    pub trend: &'static str,
    pub returns_breakdown: ReturnsData,
    pub losses_breakdown: LossesData,
    pub total_gains: f64,
    pub total_losses: f64,
    pub timeline: Vec<TimelinePoint>,
    pub monthly_breakdown: Vec<TimelinePoint>,
}

/// The liquid + progression returns breakdown.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnsData {
    pub loot_tt: f64,
    pub pes: f64,
    pub codex_pes: f64,
    pub quest_pes: f64,
    pub ledger: std::collections::BTreeMap<String, f64>,
}

/// The losses breakdown: tracking cost, its cycled split, and the
/// ledger expenses.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LossesData {
    pub tracking_cost: f64,
    pub cycled_breakdown: CycledData,
    pub ledger: std::collections::BTreeMap<String, f64>,
}

/// The per-family cycled-cost split, engine-typed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycledData {
    pub weapon: SqlNumber,
    pub healing: SqlNumber,
    pub enhancer: SqlNumber,
    pub armour: SqlNumber,
    pub dangling: SqlNumber,
    /// Harvesting (tree cutting) swing decay.
    pub harvest: SqlNumber,
}

/// One day or month of the Overview timeline; the caller labels the
/// bucket (`date` for days, `month` for months).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePoint {
    pub bucket: String,
    pub loot_tt: f64,
    pub pes: f64,
    pub codex_pes: f64,
    pub quest_pes: f64,
    pub ledger_gains: std::collections::BTreeMap<String, f64>,
    pub tracking_cost: f64,
    pub ledger_losses: std::collections::BTreeMap<String, f64>,
}

/// The Hunting aggregate: the per-mob and per-tag comparison tables.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HuntingData {
    pub mob_comparisons: Vec<ActivityRow>,
    pub tag_comparisons: Vec<ActivityRow>,
}

/// The Tree Cutting aggregate: effective yield tiers with their tool
/// strategies nested underneath.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestData {
    pub tier_comparisons: Vec<HarvestTierRow>,
}

/// One effective yield activity and its item composition.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestTierRow {
    pub yield_tier: HarvestYieldTier,
    pub swings: i64,
    pub cycled: f64,
    pub returns: f64,
    pub loot_rate: f64,
    pub loot_items: Vec<HarvestLootItemRow>,
    pub tool_comparisons: Vec<HarvestToolRow>,
}

/// One execution strategy within an effective yield tier. A missing
/// name preserves tool-attribution gaps instead of dropping their cost.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestToolRow {
    pub name: Option<String>,
    pub swings: i64,
    pub cycled: f64,
    pub returns: f64,
    pub loot_rate: f64,
    pub loot_items: Vec<HarvestLootItemRow>,
}

/// One item in a tool's harvest loot composition: realised TT figures
/// only (markup is the market layer's, merged in at the frontend).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestLootItemRow {
    pub item_name: String,
    pub quantity: i64,
    pub value_ped: f64,
}

/// One row of a Hunting comparison table; the caller labels the name
/// (`mobName` / `tagName`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRow {
    pub name: String,
    pub sessions: i64,
    pub kills: i64,
    pub hours: f64,
    pub cycled: f64,
    pub pes_per100_ped: f64,
    pub loot_rate: f64,
}

/// The analytics service's error surface. The two validation variants (a
/// malformed ledger cursor, an out-of-vocabulary preset type) carry their
/// pinned refusal messages verbatim and map to `ApiError::BadRequest` at
/// the facade; `Db` / `Storage` are the driver and rollup-refresh
/// failures, mapped to `ApiError::Internal`.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("type must be 'expense' or 'markup'")]
    InvalidPresetType,
    /// A caller-supplied value the domain rejects before it reaches storage.
    #[error("{0}")]
    InvalidInput(&'static str),
    #[error(transparent)]
    Storage(#[from] DbError),
}

impl AnalyticsService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }
}

const ACTIVITY_DOMINANCE_THRESHOLD: f64 = 0.6;

// ── Engine-typed numeric primitives (the quest-analytics siblings in
//    eo-services::quests; kept local so this module stays self-contained,
//    matching the sibling families' per-file formatter convention) ──

/// A SQLite-engine-typed number: a REAL read stays a float and an INTEGER
/// read (including the `COALESCE(SUM(...), 0)` empty case) stays an exact
/// integer, so the engine's numeric typing is visible in the type rather
/// than carried through untyped JSON.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum SqlNumber {
    Int(i64),
    Float(f64),
}

impl SqlNumber {
    /// Read a numeric column preserving the engine type: the stored
    /// value's affinity (`ValueRef`) drives the branch, so a REAL sum
    /// stays a float and an integer sum (the NULL-sum zeros) stays an
    /// integer.
    fn read(row: &rusqlite::Row, index: usize) -> SqlNumber {
        match row.get_ref_unwrap(index) {
            rusqlite::types::ValueRef::Real(value) => SqlNumber::Float(value),
            value => SqlNumber::Int(
                value
                    .as_i64()
                    .expect("SqlNumber::read reads a numeric column"),
            ),
        }
    }

    /// A window's family sum: an absent (all-NULL) family is the exact
    /// integer zero.
    fn from_family(sum: Option<f64>) -> SqlNumber {
        sum.map_or(SqlNumber::Int(0), SqlNumber::Float)
    }

    /// The sum of two engine-typed numbers, integer (exact) when both are.
    fn sum(self, other: SqlNumber) -> SqlNumber {
        match (self, other) {
            (SqlNumber::Int(left), SqlNumber::Int(right)) => SqlNumber::Int(left + right),
            _ => SqlNumber::Float(self.as_f64() + other.as_f64()),
        }
    }

    /// Banker's rounding on a float; an integer is already exact.
    fn rounded(self, places: usize) -> SqlNumber {
        match self {
            SqlNumber::Float(value) => {
                SqlNumber::Float(eo_wire::normalizer::round_half_even(value, places))
            }
            int => int,
        }
    }

    /// The float form (the response boundary's declared type).
    pub fn as_f64(self) -> f64 {
        match self {
            SqlNumber::Int(value) => value as f64,
            SqlNumber::Float(value) => value,
        }
    }
}

/// `float(value)` over an engine-typed number (the activity path, where every
/// numeric is summed in float space).
fn as_float(row: &rusqlite::Row, index: usize) -> f64 {
    SqlNumber::read(row, index).as_f64()
}

// ── Period + WHERE helpers (mirroring `_period_epoch` / `_where` /
//    `_where_iso` / `_epoch_to_iso`) ──

/// Epoch start for a named period, or `None` for all-time (and for any
/// unrecognised value, exactly as the reference's `dict.get` miss).
fn period_epoch(period: &str, now: f64) -> Option<f64> {
    let days = match period {
        "30d" => 30.0,
        "90d" => 90.0,
        "1y" => 365.0,
        _ => return None,
    };
    Some(now - days * 86400.0)
}

/// `datetime.fromtimestamp(epoch, tz=UTC).strftime("%Y-%m-%d")`.
fn epoch_to_iso(epoch: f64) -> String {
    chrono::DateTime::from_timestamp(epoch.floor() as i64, 0)
        .expect("epoch within range")
        .format("%Y-%m-%d")
        .to_string()
}

/// WHERE clause + epoch params for a numeric (unix-timestamp) column.
fn where_epoch(col: &str, start: Option<f64>, end: Option<f64>) -> (String, Vec<f64>) {
    let mut parts = Vec::new();
    let mut params = Vec::new();
    if let Some(s) = start {
        parts.push(format!("{col} >= ?"));
        params.push(s);
    }
    if let Some(e) = end {
        parts.push(format!("{col} < ?"));
        params.push(e);
    }
    (
        if parts.is_empty() {
            "1=1".to_string()
        } else {
            parts.join(" AND ")
        },
        params,
    )
}

// ── The hybrid rollup/raw window split ──

/// The hybrid split of an epoch window against the daily-rollup
/// watermark: whole days at or below the watermark aggregate from
/// `daily_rollups` (O(days), not O(rows)); partial edge days and
/// everything past the watermark aggregate from the raw tables
/// (bounded: at most one head day, plus the un-rolled tail from the
/// watermark on). The parts partition `[start, end)` exactly, so the
/// hybrid reproduces the raw-only aggregates.
struct HybridWindow {
    /// Inclusive rollup day-key range; a `None` lower bound is
    /// unbounded (the all-time period).
    rollup_days: Option<(Option<String>, String)>,
    /// The raw epoch ranges complementing the rollup days.
    raw_ranges: Vec<(Option<f64>, Option<f64>)>,
}

fn hybrid_window(start: Option<f64>, end: Option<f64>, watermark: &str) -> HybridWindow {
    let day_range = |day: &str| daily_rollup::day_range(day).expect("canonical day");
    // Everything past the watermark day's end is raw territory; rollups
    // can serve full days up to this cut.
    let boundary = day_range(watermark).1;
    let cut = end.map_or(boundary, |e| e.min(boundary));

    // First full day at or after start (None = unbounded below).
    let lo_day = start.map(|s| {
        let day = daily_rollup::epoch_day(s);
        let (d0, d1) = day_range(&day);
        if s <= d0 {
            day
        } else {
            daily_rollup::epoch_day(d1)
        }
    });
    // Last full day ending at or before the cut.
    let hi_day = daily_rollup::epoch_day(day_range(&daily_rollup::epoch_day(cut)).0 - 1.0);

    let full_days_exist = lo_day
        .as_ref()
        .is_none_or(|lo| lo.as_str() <= hi_day.as_str());
    if !full_days_exist {
        return HybridWindow {
            rollup_days: None,
            raw_ranges: vec![(start, end)],
        };
    }

    let mut raw_ranges = Vec::new();
    if let (Some(s), Some(lo)) = (start, &lo_day) {
        let lo_start = day_range(lo).0;
        if s < lo_start {
            raw_ranges.push((Some(s), Some(lo_start)));
        }
    }
    let hi_end = day_range(&hi_day).1;
    if end.is_none_or(|e| e > hi_end) {
        raw_ranges.push((Some(hi_end), end));
    }
    HybridWindow {
        rollup_days: Some((lo_day, hi_day)),
        raw_ranges,
    }
}

/// The eleven aggregate-family sums of one window part, position-matched
/// to the `daily_rollups` family columns (loot, weapon, enhancer,
/// armour, heal, dangling, skill, codex, quest, harvest loot, harvest
/// cost). A sum stays None when the part had no contributing rows, so
/// the merged result reproduces the raw engine typing: an all-empty
/// window leaves the wire as an integer zero, exactly as
/// `COALESCE(SUM(...), 0)` does.
type FamilySums = [Option<f64>; 11];

fn merge_family_sums(into: &mut FamilySums, from: FamilySums) {
    for (slot, value) in into.iter_mut().zip(from) {
        if let Some(value) = value {
            *slot = Some(slot.unwrap_or(0.0) + value);
        }
    }
}

/// The `daily_rollups` family-sum columns, position-matched to
/// [`FamilySums`] (loot, weapon, enhancer, armour, heal, dangling, skill,
/// codex, quest).
const ROLLUP_FAMILY_COLS: [&str; 11] = [
    "loot_tt",
    "weapon_cost",
    "enhancer_cost",
    "armour_cost",
    "heal_cost",
    "dangling_cost",
    "skill_tt",
    "codex_pes",
    "quest_pes",
    "harvest_loot_tt",
    "harvest_cost",
];

/// The rollup-side family sums for several windows in ONE conditional-
/// aggregation pass over `daily_rollups`, so the Overview reads the rollup
/// range once however many windows it reports (the period window plus the
/// two fixed trend windows), rather than re-scanning the rollups per
/// window. Each returned slot holds the same nine verbatim sums a
/// single-range [`rollup_family_sums`] would (NULL preserved as `None`), so
/// the per-window merge with the raw edges is unchanged and the response is
/// byte-identical. A window with no full rollup days contributes an
/// all-`None` slot.
///
/// Day keys are canonical `YYYY-MM-DD` (chrono `%Y-%m-%d`, produced by
/// [`daily_rollup::epoch_day`]/[`daily_rollup::day_range`]): they sort
/// lexically and carry no quotes, so they inline into the CASE guards the
/// same way this file's other composed statements inline column and period
/// expressions (`AssertSqlSafe`, never caller data).
fn rollup_family_sums_multi(
    conn: &rusqlite::Connection,
    windows: &[HybridWindow],
) -> Result<Vec<FamilySums>, DbError> {
    let mut out: Vec<FamilySums> = vec![[None; 11]; windows.len()];

    // The windows that actually cover full rollup days, paired with their
    // index back into `out`.
    let active: Vec<(usize, Option<&str>, &str)> = windows
        .iter()
        .enumerate()
        .filter_map(|(index, window)| {
            window
                .rollup_days
                .as_ref()
                .map(|(lo, hi)| (index, lo.as_deref(), hi.as_str()))
        })
        .collect();
    if active.is_empty() {
        return Ok(out);
    }

    // One CASE-guarded SUM per (window, family): the conditional-aggregation
    // pass. Columns are emitted window-major, family-minor, matching the
    // read-back below.
    let mut cols: Vec<String> = Vec::with_capacity(active.len() * 11);
    for (_, lo, hi) in &active {
        for col in ROLLUP_FAMILY_COLS {
            let guard = match lo {
                Some(lo) => format!("day >= '{lo}' AND day <= '{hi}'"),
                None => format!("day <= '{hi}'"),
            };
            cols.push(format!("SUM(CASE WHEN {guard} THEN {col} END)"));
        }
    }

    // Bound the single scan to the union of the windows' day ranges: up to
    // the greatest `hi`, and down to the least `lo` only when every active
    // window has a lower bound (an all-time window leaves the scan unbounded
    // below, exactly as the per-window `rollup_family_sums` did).
    let max_hi = active.iter().map(|(_, _, hi)| *hi).max().expect("active");
    let min_lo = active
        .iter()
        .all(|(_, lo, _)| lo.is_some())
        .then(|| active.iter().filter_map(|(_, lo, _)| *lo).min())
        .flatten();
    let mut where_clause = format!("day <= '{max_hi}'");
    if let Some(min_lo) = min_lo {
        where_clause.push_str(&format!(" AND day >= '{min_lo}'"));
    }

    let sql = format!(
        "SELECT {} FROM daily_rollups WHERE {where_clause}",
        cols.join(", ")
    );
    let per_slot = conn.query_row(&sql, [], |row| {
        let mut per_slot: Vec<FamilySums> = vec![[None; 11]; active.len()];
        for (slot, sums) in per_slot.iter_mut().enumerate() {
            let base = slot * 11;
            for (family, value) in sums.iter_mut().enumerate() {
                *value = row.get::<_, Option<f64>>(base + family)?;
            }
        }
        Ok(per_slot)
    })?;

    for ((out_index, _, _), sums) in active.iter().zip(per_slot) {
        out[*out_index] = sums;
    }
    Ok(out)
}

/// One raw part's family sums over `[start, end)`, verbatim (NULL kept)
/// so the merge preserves engine typing.
fn raw_family_sums(
    conn: &rusqlite::Connection,
    range: (Option<f64>, Option<f64>),
) -> Result<FamilySums, DbError> {
    fn fetch(
        conn: &rusqlite::Connection,
        sql: String,
        params: &[f64],
        sums: usize,
    ) -> Result<Vec<Option<f64>>, DbError> {
        let values = conn.query_row(&sql, rusqlite::params_from_iter(params), |row| {
            (0..sums)
                .map(|index| row.get::<_, Option<f64>>(index))
                .collect::<rusqlite::Result<Vec<Option<f64>>>>()
        })?;
        Ok(values)
    }

    let (start, end) = range;
    let mut sums: FamilySums = [None; 11];
    let (w, p) = where_epoch("timestamp", start, end);
    let kills = fetch(
        conn,
        format!("SELECT SUM(loot_total_ped), SUM(enhancer_cost) FROM kills WHERE {w}"),
        &p,
        2,
    )?;
    sums[0] = kills[0];
    sums[2] = kills[1];

    let (w, p) = where_epoch("k.timestamp", start, end);
    let weapon = fetch(
        conn,
        format!(
            "SELECT SUM(ts.cost_per_shot * ts.shots_fired) \
             FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id WHERE {w}"
        ),
        &p,
        1,
    )?;
    sums[1] = weapon[0];

    let (w, p) = where_epoch("started_at", start, end);
    let sessions = fetch(
        conn,
        format!(
            "SELECT SUM(armour_cost), SUM(heal_cost), SUM(dangling_cost) \
             FROM tracking_sessions WHERE {w}"
        ),
        &p,
        3,
    )?;
    sums[3] = sessions[0];
    sums[4] = sessions[1];
    sums[5] = sessions[2];

    let (w, p) = where_epoch("timestamp", start, end);
    sums[6] = fetch(
        conn,
        format!("SELECT SUM(ped_value) FROM skill_gains WHERE {w}"),
        &p,
        1,
    )?[0];
    let (w, p) = where_epoch("claimed_at", start, end);
    sums[7] = fetch(
        conn,
        format!("SELECT SUM(ped_value) FROM codex_claims WHERE {w}"),
        &p,
        1,
    )?[0];
    let (w, p) = where_epoch("claimed_at", start, end);
    sums[8] = fetch(
        conn,
        format!("SELECT SUM(ped_value) FROM quest_claims WHERE {w}"),
        &p,
        1,
    )?[0];
    let (w, p) = where_epoch("timestamp", start, end);
    let harvest = fetch(
        conn,
        format!("SELECT SUM(loot_total_ped), SUM(cost_ped) FROM harvest_events WHERE {w}"),
        &p,
        2,
    )?;
    sums[9] = harvest[0];
    sums[10] = harvest[1];
    Ok(sums)
}

/// A day/month-keyed aggregate (`SELECT <bucket>, COALESCE(SUM(...), 0) ...
/// GROUP BY <bucket>`) collected as `bucket -> engine-typed number`.
/// Consumers merge and look up by bucket key; no ordering is carried.
fn bucketed_epoch(
    conn: &rusqlite::Connection,
    sql: String,
    params: &[f64],
) -> Result<std::collections::BTreeMap<String, SqlNumber>, DbError> {
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
    let mut out = std::collections::BTreeMap::new();
    while let Some(row) = rows.next()? {
        out.insert(row.get::<_, String>(0)?, SqlNumber::read(row, 1));
    }
    Ok(out)
}

// ── _compute_metrics ──

/// The gains/losses breakdown for one window (`_compute_metrics`).
struct Metrics {
    /// Liquid loot TT: kill loot plus harvest (wood) loot.
    loot_tt: SqlNumber,
    skill_tt: SqlNumber,
    codex_pes: SqlNumber,
    quest_pes: SqlNumber,
    weapon: SqlNumber,
    healing: SqlNumber,
    enhancer: SqlNumber,
    armour: SqlNumber,
    dangling: SqlNumber,
    harvest: SqlNumber,
    tracking_cost: SqlNumber,
    ledger_gains: std::collections::BTreeMap<String, f64>,
    ledger_losses: std::collections::BTreeMap<String, f64>,
}

/// Per-tag ledger totals for a window, rounded to two places and
/// collected in tag order (the order the raw `GROUP BY le.tag` sorter
/// emits). Ledger windows are day-granular (`le.date` is TEXT), so the
/// split is purely lexical: date keys at or below the watermark are all
/// rolled up (the heal sweeps stray spellings); later keys read raw.
/// Part totals stay unrounded until the final merge.
fn ledger_by_tag(
    conn: &rusqlite::Connection,
    entry_type: &str,
    epoch_start: Option<f64>,
    epoch_end: Option<f64>,
    watermark: &str,
) -> Result<std::collections::BTreeMap<String, f64>, DbError> {
    let mut totals: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let bounds = |column: &str| {
        let mut sql = String::new();
        let mut params = Vec::new();
        if let Some(start) = epoch_start {
            sql.push_str(&format!(" AND {column} >= ?"));
            params.push(epoch_to_iso(start));
        }
        if let Some(end) = epoch_end {
            sql.push_str(&format!(" AND {column} < ?"));
            params.push(epoch_to_iso(end));
        }
        (sql, params)
    };

    fn accumulate(
        conn: &rusqlite::Connection,
        totals: &mut std::collections::BTreeMap<String, f64>,
        sql: &str,
        params: &[String],
    ) -> Result<(), DbError> {
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            *totals.entry(row.get::<_, String>(0)?).or_insert(0.0) += row.get::<_, f64>(1)?;
        }
        Ok(())
    }

    let (extra, params) = bounds("day");
    let sql = format!(
        "SELECT tag, SUM(amount) FROM daily_ledger_rollups \
         WHERE entry_type = ? AND day <= ?{extra} GROUP BY tag"
    );
    let mut p: Vec<String> = vec![entry_type.to_string(), watermark.to_string()];
    p.extend(params);
    accumulate(conn, &mut totals, &sql, &p)?;

    let (extra, params) = bounds("le.date");
    let sql = format!(
        "SELECT le.tag, SUM(le.amount) FROM ledger_entries le \
         WHERE le.type = ? AND le.date > ?{extra} GROUP BY le.tag"
    );
    let mut p: Vec<String> = vec![entry_type.to_string(), watermark.to_string()];
    p.extend(params);
    accumulate(conn, &mut totals, &sql, &p)?;

    let mut out = std::collections::BTreeMap::new();
    for (tag, total) in totals {
        out.insert(tag, eo_wire::normalizer::round_half_even(total, 2));
    }
    Ok(out)
}

/// One window's metrics through the hybrid split: full days at or
/// below the rollup watermark aggregate O(days) from `daily_rollups`;
/// the partial edge days and the un-rolled tail aggregate from bounded
/// raw windows.
/// One window's metrics, given its pre-computed hybrid split and the
/// rollup-side family sums from the batched [`rollup_family_sums_multi`]
/// pass. Adds the window's bounded raw edges and its per-window ledger
/// totals, then shapes the gains/losses breakdown exactly as the
/// single-pass path did (the batched rollup sums are identical to the
/// per-window ones, so the merge and the response are byte-for-byte
/// unchanged).
fn assemble_metrics(
    conn: &rusqlite::Connection,
    window: &HybridWindow,
    rollup_sums: FamilySums,
    epoch_start: Option<f64>,
    epoch_end: Option<f64>,
    watermark: &str,
) -> Result<Metrics, DbError> {
    let mut sums = rollup_sums;
    for range in &window.raw_ranges {
        merge_family_sums(&mut sums, raw_family_sums(conn, *range)?);
    }

    let kill_loot = SqlNumber::from_family(sums[0]);
    let weapon = SqlNumber::from_family(sums[1]);
    let enhancer = SqlNumber::from_family(sums[2]);
    let armour = SqlNumber::from_family(sums[3]);
    let healing = SqlNumber::from_family(sums[4]);
    let dangling = SqlNumber::from_family(sums[5]);
    let skill_tt = SqlNumber::from_family(sums[6]);
    let codex_pes = SqlNumber::from_family(sums[7]);
    let quest_pes = SqlNumber::from_family(sums[8]);
    let harvest_loot = SqlNumber::from_family(sums[9]);
    let harvest = SqlNumber::from_family(sums[10]);

    // Wood TT is liquid loot; the headline Loot TT carries both
    // activities.
    let loot_tt = kill_loot.sum(harvest_loot);

    // weapon + heal + enhancer + armour + dangling (the pinned order),
    // then harvest swing decay.
    let tracking_cost = weapon
        .sum(healing)
        .sum(enhancer)
        .sum(armour)
        .sum(dangling)
        .sum(harvest);

    let ledger_gains = ledger_by_tag(conn, "markup", epoch_start, epoch_end, watermark)?;
    let ledger_losses = ledger_by_tag(conn, "expense", epoch_start, epoch_end, watermark)?;

    Ok(Metrics {
        loot_tt,
        skill_tt,
        codex_pes,
        quest_pes,
        weapon,
        healing,
        enhancer,
        armour,
        dangling,
        harvest,
        tracking_cost,
        ledger_gains,
        ledger_losses,
    })
}

/// Sum of a ledger map's values in float space.
fn sum_values(map: &std::collections::BTreeMap<String, f64>) -> f64 {
    map.values().sum()
}

/// `_rate_from_metrics`: liquid gains over liquid losses (progression
/// excluded), 0.0 when losses are non-positive.
fn rate_from_metrics(m: &Metrics) -> f64 {
    let total_gains = m.loot_tt.as_f64() + sum_values(&m.ledger_gains);
    let total_losses = m.tracking_cost.as_f64() + sum_values(&m.ledger_losses);
    if total_losses > 0.0 {
        total_gains / total_losses
    } else {
        0.0
    }
}

/// `bucket -> {tag -> rounded amount}` for the timeline / monthly
/// breakdowns, hybrid over the same lexical watermark split as
/// [`ledger_by_tag`]: rolled date keys read `daily_ledger_rollups`,
/// later keys read raw. Both maps emit in (bucket, tag) order, the
/// order the raw `GROUP BY` sorter produced; part sums merge unrounded
/// (month buckets can span the split) and round once.
fn ledger_buckets(
    conn: &rusqlite::Connection,
    kind: BucketKind,
    entry_type: &str,
    epoch_start: Option<f64>,
    watermark: &str,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>, DbError> {
    let mut sums: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>> =
        std::collections::BTreeMap::new();
    let start_iso = epoch_start.map(epoch_to_iso);

    fn accumulate(
        conn: &rusqlite::Connection,
        sums: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
        sql: &str,
        params: &[String],
    ) -> Result<(), DbError> {
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
        while let Some(row) = rows.next()? {
            *sums
                .entry(row.get::<_, String>(0)?)
                .or_default()
                .entry(row.get::<_, String>(1)?)
                .or_insert(0.0) += row.get::<_, f64>(2)?;
        }
        Ok(())
    }

    let bucket_expr = match kind {
        BucketKind::Day => "day",
        BucketKind::Month => "strftime('%Y-%m', day)",
    };
    let extra = if start_iso.is_some() {
        " AND day >= ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {bucket_expr} AS bucket, tag, SUM(amount) FROM daily_ledger_rollups \
         WHERE entry_type = ? AND day <= ?{extra} GROUP BY bucket, tag"
    );
    let mut p: Vec<String> = vec![entry_type.to_string(), watermark.to_string()];
    if let Some(start) = &start_iso {
        p.push(start.clone());
    }
    accumulate(conn, &mut sums, &sql, &p)?;

    let bucket_expr = match kind {
        BucketKind::Day => "le.date",
        BucketKind::Month => "strftime('%Y-%m', le.date)",
    };
    let extra = if start_iso.is_some() {
        " AND le.date >= ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {bucket_expr} AS bucket, le.tag, SUM(le.amount) FROM ledger_entries le \
         WHERE le.type = ? AND le.date > ?{extra} GROUP BY bucket, le.tag"
    );
    let mut p: Vec<String> = vec![entry_type.to_string(), watermark.to_string()];
    if let Some(start) = &start_iso {
        p.push(start.clone());
    }
    accumulate(conn, &mut sums, &sql, &p)?;

    let mut out: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>> =
        std::collections::BTreeMap::new();
    for (bucket, tags) in sums {
        let entry = out.entry(bucket).or_default();
        for (tag, amount) in tags {
            entry.insert(tag, eo_wire::normalizer::round_half_even(amount, 2));
        }
    }
    Ok(out)
}

// ── overview_impl ──

/// The Overview aggregate. Scaling is O(days), not O(rows): the heal
/// brings the daily rollups current (steady-state, a single metadata
/// read), and every window then aggregates rollup rows plus bounded raw
/// edges (see [`hybrid_window`]).
async fn overview_impl(db: &Db, now: f64, period: &str) -> Result<OverviewData, DbError> {
    // The lazy rollup heal is a write (route it to the writer, never a
    // reader-held connection); every subsequent aggregate is a plain read,
    // run as one synchronous unit on a reader-core connection.
    let watermark = db
        .with_writer(move |conn| daily_rollup::heal_rollups(conn, now))
        .await?;
    let epoch_start = period_epoch(period, now);
    db.with_reader(move |conn| overview_read(conn, now, epoch_start, &watermark))
        .await
}

/// The Overview aggregate proper: a single synchronous read pass over a
/// reader-core connection (the caller heals the rollups first, on the
/// writer). Every step is a plain read, so the whole aggregate runs
/// without an executor between its statements.
fn overview_read(
    conn: &rusqlite::Connection,
    now: f64,
    epoch_start: Option<f64>,
    watermark: &str,
) -> Result<OverviewData, DbError> {
    // The Overview reports three metric windows: the requested period
    // window, and the two fixed trend windows (recent-30d, prior-30d, the
    // pair independent of the period). Their rollup-side family sums come
    // from a single conditional-aggregation pass over `daily_rollups` (the
    // Overview reads the rollup range once); each window then adds its own
    // bounded raw edges and per-window ledger totals.
    let day_30 = now - 30.0 * 86400.0;
    let day_60 = now - 60.0 * 86400.0;
    let window_bounds = [
        (epoch_start, None),
        (Some(day_30), None),
        (Some(day_60), Some(day_30)),
    ];
    let windows: Vec<HybridWindow> = window_bounds
        .iter()
        .map(|&(start, end)| hybrid_window(start, end, watermark))
        .collect();
    let rollup_sums = rollup_family_sums_multi(conn, &windows)?;
    let mut metrics = Vec::with_capacity(window_bounds.len());
    for ((start, end), (window, sums)) in window_bounds.iter().zip(windows.iter().zip(rollup_sums))
    {
        metrics.push(assemble_metrics(
            conn, window, sums, *start, *end, watermark,
        )?);
    }
    let mut metrics = metrics.into_iter();
    let m = metrics.next().expect("period window metrics");
    let rate_30d = rate_from_metrics(&metrics.next().expect("recent-30d window metrics"));
    let rate_prior = rate_from_metrics(&metrics.next().expect("prior-30d window metrics"));

    let total_ledger_gains = sum_values(&m.ledger_gains);
    let total_ledger_losses = sum_values(&m.ledger_losses);
    let total_gains = m.loot_tt.as_f64() + total_ledger_gains;
    let total_losses = m.tracking_cost.as_f64() + total_ledger_losses;
    let return_rate = if total_losses > 0.0 {
        total_gains / total_losses
    } else {
        0.0
    };

    // Trend: always recent-30d vs prior-30d, independent of period.
    let trend = if rate_30d > 0.0 && rate_prior > 0.0 {
        if rate_30d > rate_prior * 1.02 {
            "improving"
        } else if rate_30d < rate_prior * 0.98 {
            "declining"
        } else {
            "stable"
        }
    } else {
        "stable"
    };

    // Daily breakdown (the facade labels the point key "date"; monthly
    // points label it "month").
    let timeline = breakdown_points(conn, watermark, epoch_start, BucketKind::Day)?;
    let monthly = breakdown_points(conn, watermark, epoch_start, BucketKind::Month)?;

    Ok(OverviewData {
        total_return_rate: eo_wire::normalizer::round_half_even(return_rate, 4),
        trend,
        returns_breakdown: ReturnsData {
            loot_tt: m.loot_tt.rounded(2).as_f64(),
            pes: m.skill_tt.rounded(2).as_f64(),
            codex_pes: m.codex_pes.rounded(2).as_f64(),
            quest_pes: m.quest_pes.rounded(2).as_f64(),
            ledger: m.ledger_gains,
        },
        losses_breakdown: LossesData {
            tracking_cost: m.tracking_cost.rounded(2).as_f64(),
            cycled_breakdown: CycledData {
                weapon: m.weapon.rounded(2),
                healing: m.healing.rounded(2),
                enhancer: m.enhancer.rounded(2),
                armour: m.armour.rounded(2),
                dangling: m.dangling.rounded(2),
                harvest: m.harvest.rounded(2),
            },
            ledger: m.ledger_losses,
        },
        total_gains: eo_wire::normalizer::round_half_even(total_gains, 2),
        total_losses: eo_wire::normalizer::round_half_even(total_losses, 2),
        timeline,
        monthly_breakdown: monthly,
    })
}

#[derive(Clone, Copy)]
enum BucketKind {
    Day,
    Month,
}

/// The seven per-bucket family maps plus the bucket-membership set the
/// point loop consumes. Buckets with rows whose sums are NULL matter
/// only for membership (their emitted values coincide with the absent
/// key's integer-zero default), so the rollup side contributes NULL
/// family sums as absent keys and membership through `has_rows`.
#[derive(Default)]
struct BreakdownMaps {
    loot: std::collections::BTreeMap<String, SqlNumber>,
    weapon: std::collections::BTreeMap<String, SqlNumber>,
    enhancer: std::collections::BTreeMap<String, SqlNumber>,
    sess: std::collections::BTreeMap<String, SqlNumber>,
    skill: std::collections::BTreeMap<String, SqlNumber>,
    codex: std::collections::BTreeMap<String, SqlNumber>,
    quest: std::collections::BTreeMap<String, SqlNumber>,
    /// Harvest swing decay (wood loot merges into `loot`).
    harvest_cost: std::collections::BTreeMap<String, SqlNumber>,
    members: BTreeSet<String>,
}

impl BreakdownMaps {
    /// Merge one bucket's value into a family map. Day buckets never
    /// collide across parts (the hybrid ranges partition the timeline);
    /// month buckets can span the split and sum engine-typed.
    fn merge(
        map: &mut std::collections::BTreeMap<String, SqlNumber>,
        bucket: &str,
        value: SqlNumber,
    ) {
        match map.get(bucket) {
            Some(existing) => {
                let total = existing.sum(value);
                map.insert(bucket.to_string(), total);
            }
            None => {
                map.insert(bucket.to_string(), value);
            }
        }
    }
}

/// Collect the rollup side of the breakdown maps: one pass over the
/// rolled days (or their month groups).
fn rollup_breakdown(
    conn: &rusqlite::Connection,
    maps: &mut BreakdownMaps,
    kind: BucketKind,
    lo: Option<&str>,
    hi: &str,
) -> Result<(), DbError> {
    let extra = if lo.is_some() { " AND day >= ?" } else { "" };
    let sql = match kind {
        BucketKind::Day => format!(
            "SELECT day AS bucket, has_rows, loot_tt, weapon_cost, enhancer_cost, \
             armour_cost, heal_cost, dangling_cost, skill_tt, codex_pes, quest_pes, \
             harvest_loot_tt, harvest_cost \
             FROM daily_rollups WHERE day <= ?{extra} ORDER BY bucket"
        ),
        BucketKind::Month => format!(
            "SELECT strftime('%Y-%m', day) AS bucket, MAX(has_rows), SUM(loot_tt), \
             SUM(weapon_cost), SUM(enhancer_cost), SUM(armour_cost), SUM(heal_cost), \
             SUM(dangling_cost), SUM(skill_tt), SUM(codex_pes), SUM(quest_pes), \
             SUM(harvest_loot_tt), SUM(harvest_cost) \
             FROM daily_rollups WHERE day <= ?{extra} GROUP BY bucket ORDER BY bucket"
        ),
    };
    let mut params: Vec<&str> = vec![hi];
    if let Some(lo) = lo {
        params.push(lo);
    }
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
    while let Some(row) = rows.next()? {
        let bucket = row.get::<_, String>(0)?;
        if row.get::<_, i64>(1)? != 0 {
            maps.members.insert(bucket.clone());
        }
        let family = |index: usize| row.get::<_, Option<f64>>(index);
        for (map, index) in [
            (&mut maps.loot, 2),
            (&mut maps.weapon, 3),
            (&mut maps.enhancer, 4),
            (&mut maps.skill, 8),
            (&mut maps.codex, 9),
            (&mut maps.quest, 10),
            (&mut maps.harvest_cost, 12),
        ] {
            if let Some(value) = family(index)? {
                BreakdownMaps::merge(map, &bucket, SqlNumber::Float(value));
            }
        }
        // Wood loot joins the loot family (a second merge into the
        // same map, so it sits outside the disjoint-borrow loop).
        if let Some(value) = family(11)? {
            BreakdownMaps::merge(&mut maps.loot, &bucket, SqlNumber::Float(value));
        }
        // The session-cost leg mirrors the raw query's
        // COALESCE(SUM(armour),0) + COALESCE(SUM(heal),0) +
        // COALESCE(SUM(dangling),0): integer zeros for NULL legs, a
        // bucket only when any session existed (subsumed by has_rows
        // membership; an absent key emits the same integer zero).
        let armour = family(5)?;
        let heal = family(6)?;
        let dangling = family(7)?;
        if armour.is_some() || heal.is_some() || dangling.is_some() {
            let total = SqlNumber::from_family(armour)
                .sum(SqlNumber::from_family(heal))
                .sum(SqlNumber::from_family(dangling));
            BreakdownMaps::merge(&mut maps.sess, &bucket, total);
        }
    }
    Ok(())
}

/// Collect one raw range's side of the breakdown maps: the original
/// per-source bucketed queries, windowed to the range.
fn raw_breakdown(
    conn: &rusqlite::Connection,
    maps: &mut BreakdownMaps,
    kind: BucketKind,
    range: (Option<f64>, Option<f64>),
) -> Result<(), DbError> {
    let (start, end) = range;
    let ts_bucket = |col: &str| match kind {
        BucketKind::Day => format!("date({col}, 'unixepoch')"),
        BucketKind::Month => format!("strftime('%Y-%m', {col}, 'unixepoch')"),
    };
    let (enc_w, enc_p) = where_epoch("k.timestamp", start, end);
    let (sg_w, sg_p) = where_epoch("sg.timestamp", start, end);
    let (cc_w, cc_p) = where_epoch("cc.claimed_at", start, end);
    let (qc_w, qc_p) = where_epoch("qc.claimed_at", start, end);
    let (sess_w, sess_p) = where_epoch("s.started_at", start, end);
    let (hv_w, hv_p) = where_epoch("h.timestamp", start, end);

    let sources: [(
        &mut std::collections::BTreeMap<String, SqlNumber>,
        String,
        &Vec<f64>,
    ); 8] = [
        (
            &mut maps.loot,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(k.loot_total_ped), 0) FROM kills k WHERE {enc_w} GROUP BY bucket",
                ts_bucket("k.timestamp")
            ),
            &enc_p,
        ),
        (
            &mut maps.weapon,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(ts.cost_per_shot * ts.shots_fired), 0) \
                 FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id WHERE {enc_w} GROUP BY bucket",
                ts_bucket("k.timestamp")
            ),
            &enc_p,
        ),
        (
            &mut maps.enhancer,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(k.enhancer_cost), 0) FROM kills k WHERE {enc_w} GROUP BY bucket",
                ts_bucket("k.timestamp")
            ),
            &enc_p,
        ),
        (
            &mut maps.sess,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(s.armour_cost), 0) + COALESCE(SUM(s.heal_cost), 0) \
                 + COALESCE(SUM(s.dangling_cost), 0) FROM tracking_sessions s WHERE {sess_w} GROUP BY bucket",
                ts_bucket("s.started_at")
            ),
            &sess_p,
        ),
        (
            &mut maps.skill,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(sg.ped_value), 0) FROM skill_gains sg WHERE {sg_w} GROUP BY bucket",
                ts_bucket("sg.timestamp")
            ),
            &sg_p,
        ),
        (
            &mut maps.codex,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(cc.ped_value), 0) FROM codex_claims cc WHERE {cc_w} GROUP BY bucket",
                ts_bucket("cc.claimed_at")
            ),
            &cc_p,
        ),
        (
            &mut maps.quest,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(qc.ped_value), 0) FROM quest_claims qc WHERE {qc_w} GROUP BY bucket",
                ts_bucket("qc.claimed_at")
            ),
            &qc_p,
        ),
        (
            &mut maps.harvest_cost,
            format!(
                "SELECT {} as bucket, COALESCE(SUM(h.cost_ped), 0) FROM harvest_events h WHERE {hv_w} GROUP BY bucket",
                ts_bucket("h.timestamp")
            ),
            &hv_p,
        ),
    ];
    for (map, sql, params) in sources {
        let buckets = bucketed_epoch(conn, sql, params)?;
        for (bucket, value) in buckets {
            maps.members.insert(bucket.clone());
            BreakdownMaps::merge(map, &bucket, value);
        }
    }
    // Wood loot joins the loot family (the harvest table feeds two
    // families, so its second source sits outside the disjoint-borrow
    // array).
    let wood = bucketed_epoch(
        conn,
        format!(
            "SELECT {} as bucket, COALESCE(SUM(h.loot_total_ped), 0) FROM harvest_events h WHERE {hv_w} GROUP BY bucket",
            ts_bucket("h.timestamp")
        ),
        &hv_p,
    )?;
    for (bucket, value) in wood {
        maps.members.insert(bucket.clone());
        BreakdownMaps::merge(&mut maps.loot, &bucket, value);
    }
    Ok(())
}

/// Build the timeline / monthly breakdown: per-source bucketed sums merged
/// over the union of all buckets, then one point per bucket in sorted order.
/// Hybrid over the rollup watermark, exactly as [`assemble_metrics`].
fn breakdown_points(
    conn: &rusqlite::Connection,
    watermark: &str,
    epoch_start: Option<f64>,
    kind: BucketKind,
) -> Result<Vec<TimelinePoint>, DbError> {
    let window = hybrid_window(epoch_start, None, watermark);
    let mut maps = BreakdownMaps::default();
    if let Some((lo, hi)) = &window.rollup_days {
        rollup_breakdown(conn, &mut maps, kind, lo.as_deref(), hi)?;
    }
    for range in &window.raw_ranges {
        raw_breakdown(conn, &mut maps, kind, *range)?;
    }

    // cost = weapon + enhancer + sess + harvest over the union of
    // their buckets.
    let mut cost: std::collections::BTreeMap<String, SqlNumber> = std::collections::BTreeMap::new();
    let mut cost_keys: BTreeSet<String> = BTreeSet::new();
    for k in maps
        .weapon
        .keys()
        .chain(maps.enhancer.keys())
        .chain(maps.sess.keys())
        .chain(maps.harvest_cost.keys())
    {
        cost_keys.insert(k.clone());
    }
    let zero = SqlNumber::Int(0);
    for key in &cost_keys {
        let total = maps
            .weapon
            .get(key)
            .copied()
            .unwrap_or(zero)
            .sum(maps.enhancer.get(key).copied().unwrap_or(zero))
            .sum(maps.sess.get(key).copied().unwrap_or(zero))
            .sum(maps.harvest_cost.get(key).copied().unwrap_or(zero));
        cost.insert(key.clone(), total);
    }

    let gains = ledger_buckets(conn, kind, "markup", epoch_start, watermark)?;
    let losses = ledger_buckets(conn, kind, "expense", epoch_start, watermark)?;

    // all buckets, sorted (lexicographic == chronological for these forms).
    let mut all: BTreeSet<String> = maps.members;
    for k in gains.keys().chain(losses.keys()) {
        all.insert(k.clone());
    }

    let family = |map: &std::collections::BTreeMap<String, SqlNumber>, bucket: &String| {
        map.get(bucket).copied().unwrap_or(zero).rounded(4).as_f64()
    };
    let mut points = Vec::new();
    for bucket in &all {
        points.push(TimelinePoint {
            bucket: bucket.clone(),
            loot_tt: family(&maps.loot, bucket),
            pes: family(&maps.skill, bucket),
            codex_pes: family(&maps.codex, bucket),
            quest_pes: family(&maps.quest, bucket),
            ledger_gains: gains.get(bucket).cloned().unwrap_or_default(),
            tracking_cost: family(&cost, bucket),
            ledger_losses: losses.get(bucket).cloned().unwrap_or_default(),
        });
    }
    Ok(points)
}

// ── hunting_impl / harvest_impl ──

/// One completed session's aggregates (`_load_activity_sessions`).
#[derive(Default)]
struct SessionAgg {
    duration_hours: f64,
    armour_cost: f64,
    heal_cost: f64,
    dangling_cost: f64,
    weapon_cost: f64,
    enhancer_cost: f64,
    weapon_shots: f64,
    kills: i64,
    loot_tt: f64,
    skill_tt: f64,
    dominant_mob: Option<String>,
    dominant_mob_kills: i64,
    dominant_tag: Option<String>,
    dominant_tag_kills: i64,
    cycled_ped: f64,
}

async fn load_activity_sessions(db: &Db) -> Result<Vec<SessionAgg>, DbError> {
    // Read the per-session aggregates from the materialised summaries instead
    // of re-aggregating the raw tables on every request. Heal first (a write,
    // routed to the writer) so a read after a summary-version bump (or on a
    // fresh install) sees current rows; the read itself runs as one
    // synchronous unit on a reader-core connection.
    db.with_writer(|conn| crate::session_summary::heal_summaries(conn))
        .await?;
    db.with_reader(activity_sessions_read).await
}

/// The Activity per-session aggregates, read in one synchronous pass over a
/// reader-core connection (the caller heals the summaries first, on the
/// writer).
fn activity_sessions_read(conn: &mut rusqlite::Connection) -> Result<Vec<SessionAgg>, DbError> {
    let mut sessions = read_summary_activity_aggs(conn)?;

    // Reconcile the sessions Activity counts but a summary never holds: an
    // ended session with kills and cost but no skill gains qualifies for
    // Activity yet fails the summary's gains requirement, so it has no summary
    // row. Rare (usually none); computed raw only for those ids, so the cost
    // scales with the divergence, not the whole history. The divergent ids are
    // collected first, so the streaming read is done before the per-id raw
    // aggregates prepare their own statements on the same connection.
    let divergent: Vec<(String, f64, f64, f64, f64, f64)> = {
        let mut stmt = conn.prepare(
            "SELECT s.id, s.started_at, s.ended_at, COALESCE(s.armour_cost, 0), \
             COALESCE(s.heal_cost, 0), COALESCE(s.dangling_cost, 0) \
             FROM tracking_sessions s \
             LEFT JOIN session_summaries ss ON ss.session_id = s.id \
             WHERE s.ended_at IS NOT NULL AND ss.session_id IS NULL",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1).unwrap_or(0.0),
                row.get::<_, f64>(2).unwrap_or(0.0),
                as_float(row, 3),
                as_float(row, 4),
                as_float(row, 5),
            ));
        }
        out
    };
    for (id, started, ended, armour, heal, dangling) in divergent {
        let agg = raw_session_agg(conn, &id, started, ended, armour, heal, dangling)?;
        sessions.insert(id, agg);
    }

    // Activity's own qualifying filter. Order-independent: the slice builders
    // regroup and re-sort by (-kills, -cycled, name), and each group's name is
    // unique, so the map iteration order never reaches the response.
    Ok(sessions
        .into_values()
        .filter(|s| s.duration_hours > 0.0 && s.cycled_ped > 0.0 && s.kills > 0)
        .collect())
}

/// The per-session Activity aggregates read straight from `session_summaries`,
/// keyed by session id. Every field the slice builders read comes from a stored
/// column (the cost components that only fed `cycled_ped` are left at their
/// defaults, since the stored `cycled_ped` already carries their rounded sum).
fn read_summary_activity_aggs(
    conn: &rusqlite::Connection,
) -> Result<std::collections::HashMap<String, SessionAgg>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT session_id, duration_hours, kills, loot_tt, cycled_ped, activity_skill_tt, \
         dominant_mob, dominant_tag, dominant_mob_kills, dominant_tag_kills \
         FROM session_summaries",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = std::collections::HashMap::new();
    while let Some(row) = rows.next()? {
        let id = row.get::<_, String>(0)?;
        out.insert(
            id,
            SessionAgg {
                duration_hours: as_float(row, 1),
                kills: row.get::<_, i64>(2).unwrap_or(0),
                loot_tt: as_float(row, 3),
                cycled_ped: as_float(row, 4),
                skill_tt: as_float(row, 5),
                dominant_mob: row.get::<_, Option<String>>(6)?,
                dominant_tag: row.get::<_, Option<String>>(7)?,
                dominant_mob_kills: row.get::<_, i64>(8).unwrap_or(0),
                dominant_tag_kills: row.get::<_, i64>(9).unwrap_or(0),
                ..SessionAgg::default()
            },
        );
    }
    Ok(out)
}

/// Compute one session's Activity aggregate directly from the raw tables, for
/// the reconciliation path (an ended session with no summary row). Mirrors the
/// summary's own per-session computation query for query, so an included
/// no-gains session carries the same numbers a summary would if it held one.
fn raw_session_agg(
    conn: &rusqlite::Connection,
    session_id: &str,
    started_at: f64,
    ended_at: f64,
    armour_cost: f64,
    heal_cost: f64,
    dangling_cost: f64,
) -> Result<SessionAgg, DbError> {
    let mut agg = SessionAgg {
        duration_hours: (ended_at - started_at).max(0.0) / 3600.0,
        armour_cost,
        heal_cost,
        dangling_cost,
        ..SessionAgg::default()
    };

    let (kills, loot_tt, enhancer_cost) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(loot_total_ped), 0), COALESCE(SUM(enhancer_cost), 0) \
         FROM kills WHERE session_id = ?",
        rusqlite::params![session_id],
        |row| Ok((row.get::<_, i64>(0)?, as_float(row, 1), as_float(row, 2))),
    )?;
    agg.kills = kills;
    agg.loot_tt = loot_tt;
    agg.enhancer_cost = enhancer_cost;

    let (weapon_cost, weapon_shots) = conn.query_row(
        "SELECT COALESCE(SUM(ts.cost_per_shot * ts.shots_fired), 0), \
         COALESCE(SUM(ts.shots_fired), 0) FROM kill_tool_stats ts \
         JOIN kills k ON k.id = ts.kill_id WHERE k.session_id = ?",
        rusqlite::params![session_id],
        |row| Ok((as_float(row, 0), as_float(row, 1))),
    )?;
    agg.weapon_cost = weapon_cost;
    agg.weapon_shots = weapon_shots;

    agg.skill_tt = conn.query_row(
        "SELECT COALESCE(SUM(ped_value), 0) FROM skill_gains \
         WHERE session_id = ? AND ped_value IS NOT NULL",
        rusqlite::params![session_id],
        |row| Ok(as_float(row, 0)),
    )?;

    let mob_rows: Vec<(String, String, String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT mob_name, COALESCE(mob_species, ''), COALESCE(mob_maturity, ''), COUNT(*) \
             FROM kills WHERE session_id = ? AND mob_name IS NOT NULL AND mob_name != 'Unknown' \
             GROUP BY mob_name, mob_species, mob_maturity ORDER BY COUNT(*) DESC, mob_name ASC",
        )?;
        let mapped = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !mob_rows.is_empty() {
        let total_known: i64 = mob_rows.iter().map(|r| r.3).sum();
        if total_known > 0 {
            let (top_name, top_species, top_maturity, top_count) = mob_rows[0].clone();
            if top_count as f64 / total_known as f64 >= ACTIVITY_DOMINANCE_THRESHOLD {
                if !top_species.is_empty() || !top_maturity.is_empty() {
                    agg.dominant_mob = Some(top_name);
                    agg.dominant_mob_kills = top_count;
                } else {
                    agg.dominant_tag = Some(top_name);
                    agg.dominant_tag_kills = top_count;
                }
            }
        }
    }

    agg.cycled_ped = eo_wire::normalizer::round_half_even(
        agg.weapon_cost + agg.enhancer_cost + agg.armour_cost + agg.heal_cost + agg.dangling_cost,
        4,
    );
    Ok(agg)
}

/// `_build_activity_slice_rows`: group sessions by a dominant field, sum the
/// per-group stats, and sort by (-kills, -cycled, name).
fn build_activity_slice_rows(
    sessions: &[SessionAgg],
    select: impl Fn(&SessionAgg) -> Option<String>,
    kills_of: impl Fn(&SessionAgg) -> i64,
) -> Vec<ActivityRow> {
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::HashMap<String, Vec<&SessionAgg>> =
        std::collections::HashMap::new();
    for session in sessions {
        if let Some(value) = select(session) {
            if value.is_empty() {
                continue;
            }
            grouped.entry(value.clone()).or_insert_with(|| {
                order.push(value.clone());
                Vec::new()
            });
            grouped.get_mut(&value).unwrap().push(session);
        }
    }

    let mut rows: Vec<(i64, f64, String, ActivityRow)> = Vec::new();
    for value in &order {
        let matched = &grouped[value];
        let sessions_count = matched.len() as i64;
        let kills: i64 = matched.iter().map(|s| kills_of(s)).sum();
        let hours: f64 = matched.iter().map(|s| s.duration_hours).sum();
        let cycled: f64 = matched.iter().map(|s| s.cycled_ped).sum();
        let loot_tt: f64 = matched.iter().map(|s| s.loot_tt).sum();
        let skill_tt: f64 = matched.iter().map(|s| s.skill_tt).sum();
        let hours_r = eo_wire::normalizer::round_half_even(hours, 2);
        let cycled_r = eo_wire::normalizer::round_half_even(cycled, 2);
        let pes_per_100 = if cycled > 0.0 {
            eo_wire::normalizer::round_half_even((skill_tt / cycled) * 100.0, 2)
        } else {
            0.0
        };
        let loot_rate = if cycled > 0.0 {
            eo_wire::normalizer::round_half_even(loot_tt / cycled, 4)
        } else {
            0.0
        };
        let row = ActivityRow {
            name: value.clone(),
            sessions: sessions_count,
            kills,
            hours: hours_r,
            cycled: cycled_r,
            pes_per100_ped: pes_per_100,
            loot_rate,
        };
        rows.push((kills, cycled, value.clone(), row));
    }
    // sort by (-kills, -cycled, name)
    rows.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.2.cmp(&b.2))
    });
    rows.into_iter().map(|(_, _, _, row)| row).collect()
}

async fn hunting_impl(db: &Db) -> Result<HuntingData, DbError> {
    let sessions = load_activity_sessions(db).await?;
    let mob = build_activity_slice_rows(
        &sessions,
        |s| s.dominant_mob.clone(),
        |s| s.dominant_mob_kills,
    );
    let tag = build_activity_slice_rows(
        &sessions,
        |s| s.dominant_tag.clone(),
        |s| s.dominant_tag_kills,
    );
    Ok(HuntingData {
        mob_comparisons: mob,
        tag_comparisons: tag,
    })
}

/// One tier/tool group: tier, optional tool, swing count, cost, loot TT.
type HarvestToolTotals = (String, Option<String>, i64, f64, f64);

/// One tier/tool item group: tier, optional tool, item, quantity, TT.
type HarvestToolItemTotals = (String, Option<String>, String, i64, f64);

struct HarvestTierBuilder {
    swings: i64,
    cost: f64,
    loot_tt: f64,
    tools: Vec<(i64, f64, String, HarvestToolRow)>,
}

/// The Tree Cutting tier-first aggregate, grouped straight off the
/// durable event attribution. A tab-open read, not a hot path: the scan
/// is O(total harvest events), acceptable at harvesting volumes.
async fn harvest_impl(db: &Db, epoch_start: Option<f64>) -> Result<HarvestData, DbError> {
    let (raw, composition): (Vec<HarvestToolTotals>, Vec<HarvestToolItemTotals>) = db
        .with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT yield_tier, NULLIF(tool_name, ''), COUNT(*), \
                 COALESCE(SUM(cost_ped), 0), \
                 COALESCE(SUM(loot_total_ped), 0) FROM harvest_events h \
                 WHERE (?1 IS NULL OR h.timestamp >= ?1) \
                 GROUP BY yield_tier, NULLIF(tool_name, '')",
            )?;
            let raw = stmt
                .query_map([epoch_start], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        as_float(row, 3),
                        as_float(row, 4),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            // Active item composition follows the durable tier and tool.
            // Deactivated rows stay evidence for migration but never enter
            // the accounting totals served here.
            let mut comp_stmt = conn.prepare(
                "SELECT h.yield_tier, NULLIF(h.tool_name, ''), l.item_name, \
                 SUM(l.quantity), \
                 COALESCE(SUM(l.value_ped), 0) \
                 FROM harvest_loot_items l JOIN harvest_events h ON h.id = l.harvest_id \
                 WHERE (?1 IS NULL OR h.timestamp >= ?1) \
                   AND l.deactivated_at IS NULL \
                 GROUP BY h.yield_tier, NULLIF(h.tool_name, ''), l.item_name",
            )?;
            let composition = comp_stmt
                .query_map([epoch_start], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3).unwrap_or(0),
                        as_float(row, 4),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((raw, composition))
        })
        .await?;

    let mut items_by_tool: std::collections::HashMap<
        (HarvestYieldTier, Option<String>),
        Vec<HarvestLootItemRow>,
    > = std::collections::HashMap::new();
    let mut tier_item_totals: std::collections::HashMap<(HarvestYieldTier, String), (i64, f64)> =
        std::collections::HashMap::new();
    for (tier, tool, item_name, quantity, value_ped) in composition {
        let tier = HarvestYieldTier::from_db(&tier);
        items_by_tool
            .entry((tier, tool))
            .or_default()
            .push(HarvestLootItemRow {
                item_name: item_name.clone(),
                quantity,
                value_ped: eo_wire::normalizer::round_half_even(value_ped, 2),
            });
        let total = tier_item_totals.entry((tier, item_name)).or_default();
        total.0 += quantity;
        total.1 += value_ped;
    }
    for items in items_by_tool.values_mut() {
        items.sort_by(|a, b| {
            b.value_ped
                .partial_cmp(&a.value_ped)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.item_name.cmp(&b.item_name))
        });
    }

    let mut tiers: std::collections::HashMap<HarvestYieldTier, HarvestTierBuilder> =
        std::collections::HashMap::new();
    for (tier, name, swings, cost, loot_tt) in raw {
        let tier = HarvestYieldTier::from_db(&tier);
        let row = HarvestToolRow {
            name: name.clone(),
            swings,
            cycled: eo_wire::normalizer::round_half_even(cost, 2),
            returns: eo_wire::normalizer::round_half_even(loot_tt, 2),
            loot_rate: if cost > 0.0 {
                eo_wire::normalizer::round_half_even(loot_tt / cost, 4)
            } else {
                0.0
            },
            loot_items: items_by_tool
                .remove(&(tier, name.clone()))
                .unwrap_or_default(),
        };
        let builder = tiers.entry(tier).or_insert_with(|| HarvestTierBuilder {
            swings: 0,
            cost: 0.0,
            loot_tt: 0.0,
            tools: Vec::new(),
        });
        builder.swings += swings;
        builder.cost += cost;
        builder.loot_tt += loot_tt;
        builder
            .tools
            .push((swings, cost, name.unwrap_or_default(), row));
    }

    let mut tier_comparisons: Vec<HarvestTierRow> = tiers
        .into_iter()
        .map(|(tier, mut builder)| {
            builder.tools.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.0.cmp(&a.0))
                    .then(a.2.cmp(&b.2))
            });
            let mut loot_items: Vec<HarvestLootItemRow> = tier_item_totals
                .iter()
                .filter(|((item_tier, _), _)| *item_tier == tier)
                .map(
                    |((_, item_name), (quantity, value_ped))| HarvestLootItemRow {
                        item_name: item_name.clone(),
                        quantity: *quantity,
                        value_ped: eo_wire::normalizer::round_half_even(*value_ped, 2),
                    },
                )
                .collect();
            loot_items.sort_by(|a, b| {
                b.value_ped
                    .partial_cmp(&a.value_ped)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.item_name.cmp(&b.item_name))
            });
            HarvestTierRow {
                yield_tier: tier,
                swings: builder.swings,
                cycled: eo_wire::normalizer::round_half_even(builder.cost, 2),
                returns: eo_wire::normalizer::round_half_even(builder.loot_tt, 2),
                loot_rate: if builder.cost > 0.0 {
                    eo_wire::normalizer::round_half_even(builder.loot_tt / builder.cost, 4)
                } else {
                    0.0
                },
                loot_items,
                tool_comparisons: builder
                    .tools
                    .into_iter()
                    .map(|(_, _, _, row)| row)
                    .collect(),
            }
        })
        .collect();
    tier_comparisons.sort_by_key(|row| row.yield_tier.sort_rank());
    Ok(HarvestData { tier_comparisons })
}

// ── The Overview and Activity aggregates ──

impl AnalyticsService {
    /// The Overview aggregate for a named period (`30d` / `90d` / `1y`, or
    /// all-time for any other value).
    ///
    /// Scales O(days), not O(kills): the aggregates read the daily rollup
    /// projection for completed days and touch the raw tables only for the
    /// partial edge days (see [`overview_impl`]).
    pub async fn overview(&self, period: &str) -> Result<OverviewData, AnalyticsError> {
        let now = naive_to_epoch(self.clock.now());
        Ok(overview_impl(&self.db, now, period).await?)
    }

    /// The Hunting aggregate: the per-mob / per-tag comparison tables
    /// over the completed sessions.
    pub async fn hunting(&self) -> Result<HuntingData, AnalyticsError> {
        Ok(hunting_impl(&self.db).await?)
    }

    /// The Tree Cutting aggregate for a named period (`30d` / `90d` /
    /// `1y`, or all-time for any other value): effective yield tiers,
    /// each with its tool strategies and matching loot composition.
    pub async fn harvest(&self, period: &str) -> Result<HarvestData, AnalyticsError> {
        let now = naive_to_epoch(self.clock.now());
        Ok(harvest_impl(&self.db, period_epoch(period, now)).await?)
    }

    /// The whole-ledger summary for a named period (`30d` / `90d` / `1y`,
    /// or all-time for any other value): the per-tag markup and expense
    /// totals over EVERY ledger entry in the window, independent of the
    /// paginated list. Serves the Ledger tab's net-impact and source
    /// cards, through the same hybrid rollup + raw-edge split as the
    /// Overview (O(days), not O(entries)).
    pub async fn ledger_summary(&self, period: &str) -> Result<LedgerSummaryData, AnalyticsError> {
        let now = naive_to_epoch(self.clock.now());
        let watermark = self
            .db
            .with_writer(move |conn| daily_rollup::heal_rollups(conn, now))
            .await?;
        let epoch_start = period_epoch(period, now);
        let (gains, losses) = self
            .db
            .with_reader(move |conn| {
                Ok((
                    ledger_by_tag(conn, "markup", epoch_start, None, &watermark)?,
                    ledger_by_tag(conn, "expense", epoch_start, None, &watermark)?,
                ))
            })
            .await?;
        Ok(LedgerSummaryData { gains, losses })
    }
}

/// The whole-ledger per-tag summary: markup (gain) and expense (loss)
/// totals by tag over the requested window.
pub struct LedgerSummaryData {
    pub gains: std::collections::BTreeMap<String, f64>,
    pub losses: std::collections::BTreeMap<String, f64>,
}

// ── Ledger / presets / inventory writes (the CRUD surface) ──

const INVENTORY_SALE_TAG: &str = "inventory_sale";

/// The ledger and preset rows both select
/// (id, name-or-date, type, description, amount, tag); the amount reads
/// with float coercion (an INTEGER-affinity amount leaves as its float).
fn ledger_item(row: &rusqlite::Row) -> LedgerRow {
    LedgerRow {
        id: row.get_unwrap::<_, String>(0),
        date: row.get_unwrap::<_, String>(1),
        kind: row.get_unwrap::<_, String>(2),
        description: row.get_unwrap::<_, String>(3),
        amount: row.get_unwrap::<_, f64>(4),
        tag: row.get_unwrap::<_, String>(5),
    }
}

fn preset_item(row: &rusqlite::Row) -> PresetRow {
    PresetRow {
        id: row.get_unwrap::<_, String>(0),
        name: row.get_unwrap::<_, String>(1),
        kind: row.get_unwrap::<_, String>(2),
        description: row.get_unwrap::<_, String>(3),
        amount: row.get_unwrap::<_, f64>(4),
        tag: row.get_unwrap::<_, String>(5),
    }
}

/// The default ledger page size when the client names no `limit`.
const LEDGER_PAGE_DEFAULT: i64 = 50;
/// The largest ledger page a client may request; larger `limit` values clamp
/// here, bounding the work a single request can ask for.
const LEDGER_PAGE_MAX: i64 = 200;

/// The opaque keyset cursor over `[date, id]` of the last row on a page
/// (the shared [`crate::keyset`] codec).
fn encode_ledger_cursor(date: &str, id: &str) -> String {
    crate::keyset::encode_cursor(&[date, id])
}

/// Decode a keyset cursor back to its `(date, id)` seek key, or `None` for a
/// malformed token (which the handler answers as a 400).
fn decode_ledger_cursor(token: &str) -> Option<(String, String)> {
    let [date, id]: [String; 2] = crate::keyset::decode_cursor(token)?;
    Some((date, id))
}

/// The inventory row: (id, name, tt_value, markup_paid, notes, acquired_at).
fn inventory_item(row: &rusqlite::Row) -> InventoryRow {
    InventoryRow {
        id: row.get_unwrap::<_, String>(0),
        name: row.get_unwrap::<_, String>(1),
        tt_value: row.get_unwrap::<_, f64>(2),
        markup_paid: row.get_unwrap::<_, f64>(3),
        notes: row.get_unwrap::<_, Option<String>>(4),
        acquired_at: row.get_unwrap::<_, String>(5),
    }
}

/// Sub-hundredth tolerance for treating a stock or money residual as
/// closed. Positions and PED both round to hundredths at the display edge.
const STOCK_EPSILON: f64 = 1e-9;

/// The auction-listing column list, in the order [`listing_from_row`] reads.
const LISTING_COLUMNS: &str = "id, item_name, quantity, attributed_qty, unattributed_qty, \
     tt_value, attributed_tt, starting_bid, buyout, listing_fee, listed_at, status, \
     final_price, sale_fee, resolved_at";

/// One auction listing from a row over [`LISTING_COLUMNS`], with its
/// realised figures derived rather than read: a stored copy could drift from
/// the price and fees it was resolved with.
fn listing_from_row(row: &rusqlite::Row) -> AuctionListingRow {
    let tt_value = row.get_unwrap::<_, f64>(5);
    let attributed_tt = row.get_unwrap::<_, f64>(6);
    let listing_fee = row.get_unwrap::<_, f64>(9);
    let status = row.get_unwrap::<_, String>(11);
    let final_price = row.get_unwrap::<_, Option<f64>>(12);
    let sale_fee = row.get_unwrap::<_, Option<f64>>(13);

    let outcome = (status == "sold").then(|| {
        stock_allocation::resolve_sale(
            tt_value,
            attributed_tt,
            final_price.unwrap_or(0.0),
            listing_fee,
            sale_fee.unwrap_or(0.0),
        )
    });

    AuctionListingRow {
        id: row.get_unwrap::<_, String>(0),
        item_name: row.get_unwrap::<_, String>(1),
        quantity: row.get_unwrap::<_, f64>(2),
        attributed_qty: row.get_unwrap::<_, f64>(3),
        unattributed_qty: row.get_unwrap::<_, f64>(4),
        tt_value,
        attributed_tt,
        starting_bid: row.get_unwrap::<_, f64>(7),
        buyout: row.get_unwrap::<_, Option<f64>>(8),
        listing_fee,
        listed_at: row.get_unwrap::<_, String>(10),
        status,
        final_price,
        sale_fee,
        resolved_at: row.get_unwrap::<_, Option<String>>(14),
        activity_net_markup: outcome.map(|outcome| outcome.activity_net_markup),
        gross_markup: outcome.map(|outcome| outcome.gross_markup),
    }
}

/// Read one listing by id, or `None` when it does not exist.
fn read_listing(
    conn: &rusqlite::Connection,
    listing_id: &str,
) -> rusqlite::Result<Option<AuctionListingRow>> {
    use rusqlite::OptionalExtension as _;
    conn.query_row(
        &format!("SELECT {LISTING_COLUMNS} FROM auction_listings WHERE id = ?"),
        rusqlite::params![listing_id],
        |row| Ok(listing_from_row(row)),
    )
    .optional()
}

/// Append one signed stock movement. Rows are never updated or deleted: a
/// correction is a new fact, so the original allocation stays auditable.
#[allow(clippy::too_many_arguments)]
fn insert_movement(
    conn: &rusqlite::Connection,
    item_name: &str,
    movement_kind: &str,
    ref_id: Option<&str>,
    yield_tier: Option<HarvestYieldTier>,
    quantity: f64,
    tt_value: f64,
    occurred_at: &str,
    created_at: f64,
) -> rusqlite::Result<()> {
    // A movement with no tier is stock whose provenance is genuinely
    // unknown; it is consumed like any other but funds no activity.
    let source_kind = match (movement_kind, yield_tier) {
        (_, None) => "unattributed",
        ("conversion_in", _) => "conversion",
        _ => "harvest",
    };
    conn.execute(
        "INSERT INTO stock_movements ( \
             item_name, movement_kind, ref_id, source_kind, source_event_id, \
             yield_tier, quantity, tt_value, occurred_at, created_at) \
         VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?)",
        rusqlite::params![
            item_name,
            movement_kind,
            ref_id,
            source_kind,
            yield_tier.map(HarvestYieldTier::as_str),
            quantity,
            tt_value,
            occurred_at,
            created_at,
        ],
    )?;
    Ok(())
}

/// One item's open positions per yield tier, plus its TT per unit.
///
/// Recorded loot is the acquisition base, so nothing here duplicates it; the
/// movement ledger only adds what has since left, returned, or been produced
/// by a conversion. Unit TT comes from recorded loot where there is any, and
/// otherwise from what a conversion produced, which is the only other place
/// an item's value is known.
fn item_positions(
    conn: &rusqlite::Connection,
    item_name: &str,
) -> rusqlite::Result<(Vec<stock_allocation::TierPosition>, f64)> {
    // Keyed on the durable database spelling, with the empty string standing
    // for "no tier", so the merge is ordered and total.
    let mut by_tier: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut base_qty = 0.0_f64;
    let mut base_tt = 0.0_f64;

    {
        let mut stmt = conn.prepare(
            "SELECT e.yield_tier, SUM(l.quantity), SUM(l.value_ped) \
             FROM harvest_loot_items AS l \
             JOIN harvest_events AS e ON e.id = l.harvest_id \
             WHERE l.item_name = ? AND l.deactivated_at IS NULL \
             GROUP BY e.yield_tier",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![item_name], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (tier, quantity, tt_value) in rows {
            base_qty += quantity;
            base_tt += tt_value;
            *by_tier.entry(tier.unwrap_or_default()).or_insert(0.0) += quantity;
        }
    }

    let mut produced_qty = 0.0_f64;
    let mut produced_tt = 0.0_f64;
    {
        let mut stmt = conn.prepare(
            "SELECT yield_tier, SUM(quantity), SUM(tt_value) FROM stock_movements \
             WHERE item_name = ? GROUP BY yield_tier",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![item_name], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (tier, quantity, tt_value) in rows {
            if quantity > 0.0 {
                produced_qty += quantity;
                produced_tt += tt_value;
            }
            *by_tier.entry(tier.unwrap_or_default()).or_insert(0.0) += quantity;
        }
    }

    let unit_tt = if base_qty > STOCK_EPSILON {
        base_tt / base_qty
    } else if produced_qty > STOCK_EPSILON {
        produced_tt / produced_qty
    } else {
        0.0
    };

    let positions = by_tier
        .into_iter()
        .filter(|(_, quantity)| *quantity > STOCK_EPSILON)
        .map(|(tier, quantity)| stock_allocation::TierPosition {
            yield_tier: (!tier.is_empty()).then(|| HarvestYieldTier::from_db(&tier)),
            quantity,
        })
        .collect();
    Ok((positions, unit_tt))
}

impl AnalyticsService {
    /// The clock's instant as a UTC YYYY-MM-DD date.
    fn default_date(&self) -> String {
        epoch_to_iso(naive_to_epoch(self.clock.now()))
    }

    /// Keyset (seek) pagination over the ledger, newest first. Returns a
    /// [`LedgerPage`]: the page of entries (each an `id, date, type,
    /// description, amount, tag` object) plus the opaque cursor for the
    /// following page (`None` on the last page), so the list no longer
    /// reads the whole table on every request. Without a cursor the first
    /// page is served; `limit` bounds the page (default
    /// [`LEDGER_PAGE_DEFAULT`], capped at [`LEDGER_PAGE_MAX`]). A malformed
    /// cursor is [`AnalyticsError::InvalidCursor`].
    pub async fn list_ledger(
        &self,
        cursor: Option<&str>,
        limit: Option<i64>,
    ) -> Result<LedgerPage, AnalyticsError> {
        let page = limit
            .unwrap_or(LEDGER_PAGE_DEFAULT)
            .clamp(1, LEDGER_PAGE_MAX);
        let seek = match cursor {
            None => None,
            Some(token) => match decode_ledger_cursor(token) {
                Some(key) => Some(key),
                None => return Err(AnalyticsError::InvalidCursor),
            },
        };

        // The seek predicate reproduces the (date DESC, id DESC) order past
        // the cursor row; one extra row is fetched to detect a further page.
        let mut sql =
            String::from("SELECT id, date, type, description, amount, tag FROM ledger_entries");
        if seek.is_some() {
            sql.push_str(" WHERE date < ? OR (date = ? AND id < ?)");
        }
        sql.push_str(" ORDER BY date DESC, id DESC LIMIT ?");

        // Each fetched row as (date, id, wire shape) plus the whole-table
        // count; the cursor is cut from the last kept row's (date, id).
        let (rows, total): (Vec<(String, String, LedgerRow)>, i64) = self
            .db
            .with_reader(move |conn| {
                let total: i64 =
                    conn.query_row("SELECT COUNT(*) FROM ledger_entries", [], |row| row.get(0))?;
                let mut stmt = conn.prepare(&sql)?;
                let map_row =
                    |row: &rusqlite::Row| -> rusqlite::Result<(String, String, LedgerRow)> {
                        Ok((
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(0)?,
                            ledger_item(row),
                        ))
                    };
                let rows = match &seek {
                    Some((date, id)) => stmt
                        .query_map(rusqlite::params![date, date, id, page + 1], map_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                    None => stmt
                        .query_map(rusqlite::params![page + 1], map_row)?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                };
                Ok((rows, total))
            })
            .await?;

        // A full extra row means another page follows: drop it and cut the
        // next cursor from the last row actually served.
        let has_more = rows.len() as i64 > page;
        let kept = if has_more {
            &rows[..page as usize]
        } else {
            &rows[..]
        };
        let entries: Vec<LedgerRow> = kept.iter().map(|(_, _, item)| item.clone()).collect();
        let next_cursor = has_more
            .then(|| kept.last())
            .flatten()
            .map(|(date, id, _)| encode_ledger_cursor(date, id));
        Ok(LedgerPage {
            entries,
            next_cursor,
            total,
        })
    }

    /// Create a ledger entry, relanding its (possibly backdated) day's
    /// rollup in the same transaction. Returns the created entry's wire
    /// shape (the input echoed with its generated id).
    pub async fn create_ledger_entry(
        &self,
        date: &str,
        kind: &str,
        description: &str,
        amount: f64,
        tag: &str,
    ) -> Result<LedgerRow, AnalyticsError> {
        let id = Uuid::new_v4().to_string();
        // One transaction over the insert and the rollup refresh: a
        // backdated entry relands its day's rollup with the write.
        let (id_c, date_c, kind_c, desc_c, tag_c) = (
            id.clone(),
            date.to_string(),
            kind.to_string(),
            description.to_string(),
            tag.to_string(),
        );
        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    rusqlite::params![id_c, date_c, kind_c, desc_c, amount, tag_c],
                )?;
                daily_rollup::refresh_days(&tx, [date_c.as_str()])?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(LedgerRow {
            id,
            date: date.to_string(),
            kind: kind.to_string(),
            description: description.to_string(),
            amount,
            tag: tag.to_string(),
        })
    }

    /// Delete a ledger entry, relanding its day's rollup in the same
    /// transaction. Returns whether a row existed (`false` = not found).
    pub async fn delete_ledger_entry(&self, entry_id: &str) -> Result<bool, AnalyticsError> {
        // Capture the entry's day before deleting so its rollup relands
        // in the same transaction; a vanished entry reports not-found.
        let entry_id = entry_id.to_string();
        let existed = self
            .db
            .with_writer(move |conn| {
                use rusqlite::OptionalExtension as _;
                let tx = conn.transaction()?;
                let date: Option<String> = tx
                    .query_row(
                        "SELECT date FROM ledger_entries WHERE id = ?",
                        rusqlite::params![entry_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(date) = date else {
                    return Ok(false);
                };
                tx.execute(
                    "DELETE FROM ledger_entries WHERE id = ?",
                    rusqlite::params![entry_id],
                )?;
                daily_rollup::refresh_days(&tx, [date])?;
                tx.commit()?;
                Ok(true)
            })
            .await?;
        Ok(existed)
    }

    /// The ledger presets, in creation order.
    pub async fn list_ledger_presets(&self) -> Result<Vec<PresetRow>, AnalyticsError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, type, description, amount, tag FROM ledger_presets \
                     ORDER BY created_at ASC, id ASC",
                )?;
                let rows = stmt
                    .query_map([], |row| Ok(preset_item(row)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// Create a ledger preset. The type is a closed vocabulary
    /// (`expense` / `markup`); anything else is
    /// [`AnalyticsError::InvalidPresetType`].
    pub async fn create_ledger_preset(
        &self,
        name: &str,
        kind: &str,
        description: &str,
        amount: f64,
        tag: &str,
    ) -> Result<PresetRow, AnalyticsError> {
        if kind != "expense" && kind != "markup" {
            return Err(AnalyticsError::InvalidPresetType);
        }
        let id = Uuid::new_v4().to_string();
        {
            let (id, name, kind, description, tag) = (
                id.clone(),
                name.to_string(),
                kind.to_string(),
                description.to_string(),
                tag.to_string(),
            );
            self.db
                .with_writer(move |conn| {
                    conn.execute(
                        "INSERT INTO ledger_presets (id, name, type, description, amount, tag) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                        rusqlite::params![id, name, kind, description, amount, tag],
                    )?;
                    Ok(())
                })
                .await?;
        }
        Ok(PresetRow {
            id,
            name: name.to_string(),
            kind: kind.to_string(),
            description: description.to_string(),
            amount,
            tag: tag.to_string(),
        })
    }

    /// Delete a ledger preset. Returns whether a row existed.
    pub async fn delete_ledger_preset(&self, preset_id: &str) -> Result<bool, AnalyticsError> {
        let preset_id = preset_id.to_string();
        let affected = self
            .db
            .with_writer(move |conn| {
                Ok(conn.execute(
                    "DELETE FROM ledger_presets WHERE id = ?",
                    rusqlite::params![preset_id],
                )?)
            })
            .await?;
        Ok(affected != 0)
    }

    /// The inventory items, newest acquisition first.
    pub async fn list_inventory(&self) -> Result<Vec<InventoryRow>, AnalyticsError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, tt_value, markup_paid, notes, acquired_at \
                     FROM inventory_items ORDER BY acquired_at DESC, id DESC",
                )?;
                let rows = stmt
                    .query_map([], |row| Ok(inventory_item(row)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// Every canonical item the player currently holds, most valuable first.
    ///
    /// Recorded loot is the acquisition base and the movement ledger says
    /// what has since left or returned; an item whose position has closed is
    /// dropped rather than reported as a zero row.
    pub async fn stock_positions(&self) -> Result<Vec<StockPositionRow>, AnalyticsError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut items: Vec<String> = Vec::new();
                {
                    let mut stmt = conn.prepare(
                        "SELECT item_name FROM harvest_loot_items WHERE deactivated_at IS NULL \
                         UNION SELECT item_name FROM stock_movements",
                    )?;
                    let names = stmt
                        .query_map([], |row| row.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    items.extend(names);
                }

                let mut rows = Vec::new();
                for item_name in items {
                    let (positions, _unit_tt) = item_positions(conn, &item_name)?;
                    let quantity: f64 = positions.iter().map(|p| p.quantity).sum();
                    if quantity <= STOCK_EPSILON {
                        continue;
                    }
                    let tt_value: f64 = conn.query_row(
                        "SELECT COALESCE(( \
                             SELECT SUM(l.value_ped) FROM harvest_loot_items AS l \
                             WHERE l.item_name = ?1 AND l.deactivated_at IS NULL), 0) \
                         + COALESCE(( \
                             SELECT SUM(m.tt_value) FROM stock_movements AS m \
                             WHERE m.item_name = ?1), 0)",
                        rusqlite::params![item_name],
                        |row| row.get(0),
                    )?;
                    let listed_quantity: f64 = conn.query_row(
                        "SELECT COALESCE(SUM(quantity), 0) FROM auction_listings \
                         WHERE item_name = ? AND status = 'pending'",
                        rusqlite::params![item_name],
                        |row| row.get(0),
                    )?;
                    rows.push(StockPositionRow {
                        item_name,
                        quantity,
                        tt_value,
                        listed_quantity,
                    });
                }
                rows.sort_by(|a, b| {
                    b.tt_value
                        .partial_cmp(&a.tt_value)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.item_name.cmp(&b.item_name))
                });
                Ok(rows)
            })
            .await?)
    }

    /// Every auction listing, unresolved first and newest within each group.
    pub async fn auction_listings(&self) -> Result<Vec<AuctionListingRow>, AnalyticsError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {LISTING_COLUMNS} FROM auction_listings \
                     ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END, \
                              listed_at DESC, id DESC"
                ))?;
                let rows = stmt
                    .query_map([], |row| Ok(listing_from_row(row)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?)
    }

    /// List stock on the auction.
    ///
    /// The listed quantity leaves holdings immediately, because in game it
    /// has left the player's inventory the moment the listing exists. The
    /// starting-bid fee is spent immediately too, and is written to the
    /// ledger dated to the listing: it is gone whether or not the item ever
    /// sells. No markup is realised here. That waits for confirmation.
    pub async fn create_auction_listing(
        &self,
        item_name: &str,
        quantity: f64,
        starting_bid: f64,
        buyout: Option<f64>,
        listing_fee: f64,
        listed_at: Option<&str>,
    ) -> Result<AuctionListingRow, AnalyticsError> {
        if quantity <= 0.0 {
            return Err(AnalyticsError::InvalidInput(
                "a listing needs a positive quantity",
            ));
        }
        if starting_bid < 0.0 || listing_fee < 0.0 {
            return Err(AnalyticsError::InvalidInput(
                "auction prices and fees cannot be negative",
            ));
        }

        let id = Uuid::new_v4().to_string();
        let listed_at = listed_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        let now = naive_to_epoch(self.clock.now());
        let (id_c, item_c, listed_c) = (id.clone(), item_name.to_string(), listed_at.clone());

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let (positions, unit_tt) = item_positions(&tx, &item_c)?;
                let plan = stock_allocation::allocate(&positions, quantity, unit_tt);

                tx.execute(
                    "INSERT INTO auction_listings ( \
                         id, item_name, profession, quantity, attributed_qty, unattributed_qty, \
                         tt_value, attributed_tt, starting_bid, buyout, listing_fee, listed_at, \
                         status, created_at, updated_at) \
                     VALUES (?, ?, 'harvesting', ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                    rusqlite::params![
                        id_c,
                        item_c,
                        quantity,
                        plan.attributed_qty,
                        plan.unattributed_qty,
                        quantity * unit_tt,
                        plan.attributed_tt,
                        starting_bid,
                        buyout,
                        listing_fee,
                        listed_c,
                        now,
                        now,
                    ],
                )?;

                for allocation in &plan.allocations {
                    insert_movement(
                        &tx,
                        &item_c,
                        "listing",
                        Some(&id_c),
                        allocation.yield_tier,
                        -allocation.quantity,
                        -allocation.tt_value,
                        &listed_c,
                        now,
                    )?;
                }

                if listing_fee > 0.0 {
                    let entry_id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                         VALUES (?, ?, 'expense', ?, ?, 'market')",
                        rusqlite::params![
                            entry_id,
                            listed_c,
                            format!("Auction Fee: {item_c}"),
                            listing_fee,
                        ],
                    )?;
                    tx.execute(
                        "UPDATE auction_listings SET fee_entry_id = ? WHERE id = ?",
                        rusqlite::params![entry_id, id_c],
                    )?;
                    daily_rollup::refresh_days(&tx, [listed_c.as_str()])?;
                }

                let row = read_listing(&tx, &id_c)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
                tx.commit()?;
                Ok(row)
            })
            .await
            .map_err(AnalyticsError::from)
    }

    /// Confirm a listing sold, at the price it actually fetched.
    ///
    /// This is the recognition boundary: markup becomes realised here and
    /// nowhere earlier, because until the auction closes neither the sale
    /// nor its price is a fact. The ledger gains the money that was not
    /// already booked as loot TT, plus the point-of-sale fee.
    pub async fn confirm_auction_listing(
        &self,
        listing_id: &str,
        final_price: f64,
        sale_fee: f64,
        resolved_at: Option<&str>,
    ) -> Result<Option<AuctionListingRow>, AnalyticsError> {
        if final_price < 0.0 || sale_fee < 0.0 {
            return Err(AnalyticsError::InvalidInput(
                "a sale price and fee cannot be negative",
            ));
        }
        let listing_id = listing_id.to_string();
        let resolved_at = resolved_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let Some(listing) = read_listing(&tx, &listing_id)? else {
                    return Ok(None);
                };
                if listing.status != "pending" {
                    return Ok(None);
                }

                let outcome = stock_allocation::resolve_sale(
                    listing.tt_value,
                    listing.attributed_tt,
                    final_price,
                    listing.listing_fee,
                    sale_fee,
                );

                tx.execute(
                    "UPDATE auction_listings SET status = 'sold', final_price = ?, \
                         sale_fee = ?, resolved_at = ? WHERE id = ?",
                    rusqlite::params![final_price, sale_fee, resolved_at, listing_id],
                )?;

                // Only the TT of tracked stock was booked as loot when it
                // dropped, so only that is netted off here; untracked units
                // were never counted and their full proceeds are new money.
                if outcome.ledger_income.abs() > STOCK_EPSILON {
                    let entry_id = Uuid::new_v4().to_string();
                    let kind = if outcome.ledger_income > 0.0 {
                        "markup"
                    } else {
                        "expense"
                    };
                    tx.execute(
                        "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                         VALUES (?, ?, ?, ?, ?, 'market')",
                        rusqlite::params![
                            entry_id,
                            resolved_at,
                            kind,
                            format!("Auction Sale: {}", listing.item_name),
                            outcome.ledger_income.abs(),
                        ],
                    )?;
                    tx.execute(
                        "UPDATE auction_listings SET sale_entry_id = ? WHERE id = ?",
                        rusqlite::params![entry_id, listing_id],
                    )?;
                }

                if sale_fee > 0.0 {
                    let entry_id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                         VALUES (?, ?, 'expense', ?, ?, 'market')",
                        rusqlite::params![
                            entry_id,
                            resolved_at,
                            format!("Auction Fee: {}", listing.item_name),
                            sale_fee,
                        ],
                    )?;
                    tx.execute(
                        "UPDATE auction_listings SET sale_fee_entry_id = ? WHERE id = ?",
                        rusqlite::params![entry_id, listing_id],
                    )?;
                }

                daily_rollup::refresh_days(&tx, [resolved_at.as_str()])?;
                let row = read_listing(&tx, &listing_id)?;
                tx.commit()?;
                Ok(row)
            })
            .await
            .map_err(AnalyticsError::from)
    }

    /// Mark a listing expired: the stock comes back, the fee stays spent.
    ///
    /// The returning movements are written as new rows rather than by
    /// deleting the original allocation, so what was attributed at listing
    /// time stays auditable. Nothing reaches the activity: the loot returned
    /// intact, and a fee lost to an auction that did not clear describes
    /// market execution rather than the gameplay.
    pub async fn expire_auction_listing(
        &self,
        listing_id: &str,
        resolved_at: Option<&str>,
    ) -> Result<Option<AuctionListingRow>, AnalyticsError> {
        let listing_id = listing_id.to_string();
        let resolved_at = resolved_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        let now = naive_to_epoch(self.clock.now());

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let Some(listing) = read_listing(&tx, &listing_id)? else {
                    return Ok(None);
                };
                if listing.status != "pending" {
                    return Ok(None);
                }

                let returning: Vec<(Option<String>, f64, f64)> = {
                    let mut stmt = tx.prepare(
                        "SELECT yield_tier, quantity, tt_value FROM stock_movements \
                         WHERE ref_id = ? AND movement_kind = 'listing'",
                    )?;
                    let rows = stmt
                        .query_map(rusqlite::params![listing_id], |row| {
                            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    rows
                };
                for (tier, quantity, tt_value) in returning {
                    insert_movement(
                        &tx,
                        &listing.item_name,
                        "listing_return",
                        Some(&listing_id),
                        tier.as_deref().map(HarvestYieldTier::from_db),
                        -quantity,
                        -tt_value,
                        &resolved_at,
                        now,
                    )?;
                }

                tx.execute(
                    "UPDATE auction_listings SET status = 'expired', resolved_at = ? WHERE id = ?",
                    rusqlite::params![resolved_at, listing_id],
                )?;
                let row = read_listing(&tx, &listing_id)?;
                tx.commit()?;
                Ok(row)
            })
            .await
            .map_err(AnalyticsError::from)
    }

    /// Recycle stock into another item at 1:1 TT.
    ///
    /// A transformation, not a sale: no markup is realised and the ledger is
    /// untouched. The consumed stock's activity composition rides forward
    /// into the produced item, so selling the result still attributes back to
    /// the activities that grew it.
    pub async fn convert_stock(
        &self,
        source_item: &str,
        target_item: &str,
        quantity: f64,
        converted_at: Option<&str>,
    ) -> Result<(), AnalyticsError> {
        if quantity <= 0.0 {
            return Err(AnalyticsError::InvalidInput(
                "a conversion needs a positive quantity",
            ));
        }
        let id = Uuid::new_v4().to_string();
        let (source_c, target_c) = (source_item.to_string(), target_item.to_string());
        let converted_at = converted_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        let now = naive_to_epoch(self.clock.now());

        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                let (positions, unit_tt) = item_positions(&tx, &source_c)?;
                // Converting more than is tracked is allowed for the same
                // reason selling more is: the player may hold stock from
                // before tracking began. The excess rides forward explicitly
                // unattributed rather than being refused or invented.
                let plan = stock_allocation::allocate(&positions, quantity, unit_tt);
                let converted_tt = quantity * unit_tt;

                tx.execute(
                    "INSERT INTO stock_conversions \
                         (id, source_item, target_item, quantity, tt_value, converted_at, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![id, source_c, target_c, quantity, converted_tt, converted_at, now],
                )?;

                for allocation in &plan.allocations {
                    insert_movement(
                        &tx,
                        &source_c,
                        "conversion_out",
                        Some(&id),
                        allocation.yield_tier,
                        -allocation.quantity,
                        -allocation.tt_value,
                        &converted_at,
                        now,
                    )?;
                    // TT is preserved exactly, and the produced units are
                    // measured in PED of TT: the game's recycling ratio is
                    // 1:1 in value, and no per-unit TT for the produced item
                    // is recorded anywhere to divide by.
                    insert_movement(
                        &tx,
                        &target_c,
                        "conversion_in",
                        Some(&id),
                        allocation.yield_tier,
                        allocation.tt_value,
                        allocation.tt_value,
                        &converted_at,
                        now,
                    )?;
                }

                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    /// Net realised markup per yield tier, from confirmed sales only.
    ///
    /// Each sold listing's activity-claimable markup is divided across the
    /// tiers that supplied it, in proportion to the TT each contributed.
    /// Pending listings realise nothing, and expired ones never realise.
    pub async fn realised_markup_by_tier(&self) -> Result<Vec<RealisedTierMarkup>, AnalyticsError> {
        Ok(self
            .db
            .with_reader(|conn| {
                let sold: Vec<(String, f64, f64, f64, f64, f64)> = {
                    let mut stmt = conn.prepare(
                        "SELECT id, tt_value, attributed_tt, COALESCE(final_price, 0), \
                                listing_fee, COALESCE(sale_fee, 0) \
                         FROM auction_listings WHERE status = 'sold'",
                    )?;
                    let rows = stmt
                        .query_map([], |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    rows
                };

                let mut totals: std::collections::BTreeMap<&'static str, f64> =
                    std::collections::BTreeMap::new();
                for (id, tt_value, attributed_tt, final_price, listing_fee, sale_fee) in sold {
                    let outcome = stock_allocation::resolve_sale(
                        tt_value,
                        attributed_tt,
                        final_price,
                        listing_fee,
                        sale_fee,
                    );
                    if outcome.activity_net_markup.abs() <= STOCK_EPSILON {
                        continue;
                    }
                    let contributions: Vec<(String, f64)> = {
                        let mut stmt = conn.prepare(
                            "SELECT yield_tier, SUM(-tt_value) FROM stock_movements \
                             WHERE ref_id = ? AND movement_kind = 'listing' \
                               AND yield_tier IS NOT NULL \
                             GROUP BY yield_tier",
                        )?;
                        let rows = stmt
                            .query_map(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                        rows
                    };
                    let contributed_tt: f64 = contributions.iter().map(|(_, tt)| tt).sum();
                    if contributed_tt <= STOCK_EPSILON {
                        continue;
                    }
                    for (tier, tt) in contributions {
                        let share = tt / contributed_tt;
                        *totals
                            .entry(HarvestYieldTier::from_db(&tier).as_str())
                            .or_insert(0.0) += outcome.activity_net_markup * share;
                    }
                }

                let mut rows: Vec<RealisedTierMarkup> = totals
                    .into_iter()
                    .map(|(tier, net_markup)| RealisedTierMarkup {
                        yield_tier: HarvestYieldTier::from_db(tier),
                        net_markup,
                    })
                    .collect();
                rows.sort_by_key(|row| row.yield_tier.sort_rank());
                Ok(rows)
            })
            .await?)
    }

    /// The stored inventory row re-read and shaped (the create / patch
    /// reply). A row that has vanished since the write is a driver-level
    /// invariant break, surfaced as [`AnalyticsError::Storage`].
    async fn inventory_row(&self, item_id: &str) -> Result<InventoryRow, AnalyticsError> {
        let item_id = item_id.to_string();
        Ok(self
            .db
            .with_reader(move |conn| {
                conn.query_row(
                    "SELECT id, name, tt_value, markup_paid, notes, acquired_at \
                     FROM inventory_items WHERE id = ?",
                    rusqlite::params![item_id],
                    |row| Ok(inventory_item(row)),
                )
                .map_err(DbError::from)
            })
            .await?)
    }

    /// Create an inventory item; an empty / absent `acquired_at` defaults
    /// to the clock's UTC date. Returns the stored row's wire shape.
    pub async fn create_inventory_item(
        &self,
        name: &str,
        tt_value: f64,
        markup_paid: f64,
        notes: Option<&str>,
        acquired_at: Option<&str>,
    ) -> Result<InventoryRow, AnalyticsError> {
        let id = Uuid::new_v4().to_string();
        // acquired_at falls back to today's UTC date: the original's `or`
        // treats an empty string as falsy, so "" defaults to the clock date.
        let date = acquired_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        {
            let (id, name, notes, date) = (
                id.clone(),
                name.to_string(),
                notes.map(str::to_string),
                date.clone(),
            );
            self.db
                .with_writer(move |conn| {
                    conn.execute(
                        "INSERT INTO inventory_items \
                         (id, name, tt_value, markup_paid, notes, acquired_at) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                        rusqlite::params![id, name, tt_value, markup_paid, notes, date],
                    )?;
                    Ok(())
                })
                .await?;
        }
        self.inventory_row(&id).await
    }

    /// Update an inventory item: only provided (`Some`) fields change,
    /// bumping `updated_at`; an all-`None` patch still re-reads and returns
    /// the row. `None` (not `Some(Value::Null)`) reports a missing item.
    pub async fn update_inventory_item(
        &self,
        item_id: &str,
        name: Option<&str>,
        tt_value: Option<f64>,
        markup_paid: Option<f64>,
        notes: Option<&str>,
    ) -> Result<Option<InventoryRow>, AnalyticsError> {
        // The existence check and the (possibly empty) update run together on
        // the writer connection, which reads as well as writes.
        let updated = {
            let item_id = item_id.to_string();
            let name = name.map(str::to_string);
            let notes = notes.map(str::to_string);
            self.db
                .with_writer(move |conn| {
                    use rusqlite::OptionalExtension as _;
                    let exists = conn
                        .query_row(
                            "SELECT 1 FROM inventory_items WHERE id = ?",
                            rusqlite::params![item_id],
                            |_| Ok(()),
                        )
                        .optional()?;
                    if exists.is_none() {
                        return Ok(false);
                    }

                    let mut sets: Vec<&str> = Vec::new();
                    if name.is_some() {
                        sets.push("name = ?");
                    }
                    if tt_value.is_some() {
                        sets.push("tt_value = ?");
                    }
                    if markup_paid.is_some() {
                        sets.push("markup_paid = ?");
                    }
                    if notes.is_some() {
                        sets.push("notes = ?");
                    }
                    if !sets.is_empty() {
                        sets.push("updated_at = unixepoch('now')");
                        let sql = format!(
                            "UPDATE inventory_items SET {} WHERE id = ?",
                            sets.join(", ")
                        );
                        let mut params: Vec<rusqlite::types::Value> = Vec::new();
                        if let Some(value) = &name {
                            params.push(value.clone().into());
                        }
                        if let Some(value) = tt_value {
                            params.push(value.into());
                        }
                        if let Some(value) = markup_paid {
                            params.push(value.into());
                        }
                        if let Some(value) = &notes {
                            params.push(value.clone().into());
                        }
                        params.push(item_id.clone().into());
                        conn.execute(&sql, rusqlite::params_from_iter(params))?;
                    }
                    Ok(true)
                })
                .await?
        };
        if !updated {
            return Ok(None);
        }
        Ok(Some(self.inventory_row(item_id).await?))
    }

    /// Delete an inventory item. Returns whether a row existed.
    pub async fn delete_inventory_item(&self, item_id: &str) -> Result<bool, AnalyticsError> {
        let item_id = item_id.to_string();
        let affected = self
            .db
            .with_writer(move |conn| {
                Ok(conn.execute(
                    "DELETE FROM inventory_items WHERE id = ?",
                    rusqlite::params![item_id],
                )?)
            })
            .await?;
        Ok(affected != 0)
    }

    /// Sell an inventory item: emit the realised delta to the ledger and
    /// remove the row, atomically; a zero-delta sale skips the ledger row
    /// (`ledgerEntry` null). Returns the `{ ledgerEntry, soldItem }` shape,
    /// or `None` for a missing item.
    pub async fn sell_inventory_item(
        &self,
        item_id: &str,
        sale_price: f64,
        description: Option<&str>,
        sold_at: Option<&str>,
    ) -> Result<Option<InventorySale>, AnalyticsError> {
        // The item is read on a reader-core connection; the realised sale then
        // writes its ledger row and removes the item in one writer transaction
        // (the rollup refresh must commit atomically with the ledger insert).
        let fetched = {
            let item_id = item_id.to_string();
            self.db
                .with_reader(move |conn| {
                    use rusqlite::OptionalExtension as _;
                    conn.query_row(
                        "SELECT id, name, tt_value, markup_paid, notes, acquired_at \
                         FROM inventory_items WHERE id = ?",
                        rusqlite::params![item_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(1)?,
                                row.get::<_, f64>(2)?,
                                row.get::<_, f64>(3)?,
                                inventory_item(row),
                            ))
                        },
                    )
                    .optional()
                    .map_err(DbError::from)
                })
                .await?
        };
        let Some((name, tt_value, markup_paid, sold_item)) = fetched else {
            return Ok(None);
        };

        let cost_basis = tt_value + markup_paid;
        let delta = sale_price - cost_basis;
        // sold_at falls back to today's UTC date; an empty string counts as absent.
        let sold_at = sold_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());

        // The realised sale writes its ledger row (when non-zero) and removes
        // the item in one writer-core transaction; the rollup refresh commits
        // atomically with the ledger insert.
        let ledger_write: Option<(String, String, &'static str, String, f64)> = if delta != 0.0 {
            let entry_id = Uuid::new_v4().to_string();
            let entry_type = if delta > 0.0 { "markup" } else { "expense" };
            let amount = delta.abs();
            // `payload.description or "Inventory Sale: {name}"`: "" is falsy.
            let description = description
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Inventory Sale: {name}"));
            Some((entry_id, sold_at, entry_type, description, amount))
        } else {
            None
        };
        let item_id_owned = item_id.to_string();
        let ledger_for_closure = ledger_write.clone();
        self.db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                if let Some((entry_id, sold_at, entry_type, description, amount)) =
                    &ledger_for_closure
                {
                    tx.execute(
                        "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                        rusqlite::params![
                            entry_id,
                            sold_at,
                            entry_type,
                            description,
                            amount,
                            INVENTORY_SALE_TAG
                        ],
                    )?;
                    daily_rollup::refresh_days(&tx, [sold_at.as_str()])?;
                }
                tx.execute(
                    "DELETE FROM inventory_items WHERE id = ?",
                    rusqlite::params![item_id_owned],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        let ledger_entry = ledger_write.map(
            |(entry_id, sold_at, entry_type, description, amount)| LedgerRow {
                id: entry_id,
                date: sold_at,
                kind: entry_type.to_string(),
                description,
                amount,
                tag: INVENTORY_SALE_TAG.to_string(),
            },
        );
        Ok(Some(InventorySale {
            ledger_entry,
            sold_item,
        }))
    }
}

#[cfg(test)]
impl AnalyticsService {
    /// The database handle, for tests that drive the synchronous core
    /// (the rollup heal) directly, or seed/read state through it.
    fn db(&self) -> &Db {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eo_wire::normalizer::to_wire_json;
    use serde_json::{json, Value};

    /// The wire shape of a typed value, for byte-shape assertions.
    fn to_json<T: Serialize>(value: T) -> Value {
        serde_json::to_value(value).expect("analytics value serialises")
    }

    /// A real database (the synchronous core) over a temp file. A temp file
    /// (not `:memory:`) is required: the synchronous core opens its own
    /// connections, which an in-memory database cannot share. The reads
    /// under test run on the core; the seeds commit through the same handle
    /// and are visible to the core's readers under WAL.
    async fn open_env() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("entropia_orme.db"))
            .await
            .unwrap();
        (dir, db)
    }

    /// An [`AnalyticsService`] over a real temp-file database, its clock frozen
    /// so the UTC default date is deterministic
    /// (2026-06-01).
    async fn write_service() -> (tempfile::TempDir, AnalyticsService) {
        use crate::clock::MockClock;
        let (dir, db) = open_env().await;
        let naive =
            chrono::NaiveDateTime::parse_from_str("2026-06-01T12:00:00", "%Y-%m-%dT%H:%M:%S")
                .unwrap();
        (
            dir,
            AnalyticsService::new(db, Arc::new(MockClock::new(Some(naive), 0.0))),
        )
    }

    /// 2026-06-05T00:00:00Z: heals the rollup watermark past the
    /// backdated days these tests write to, so the write hooks are
    /// observable.
    async fn heal_to_june_fifth(db: &Db) {
        db.with_writer(move |conn| daily_rollup::heal_rollups(conn, 1_780_617_600.0))
            .await
            .unwrap();
    }

    async fn ledger_rollup(db: &Db, day: &str, tag: &str) -> Option<(String, f64)> {
        use rusqlite::OptionalExtension;
        let day = day.to_string();
        let tag = tag.to_string();
        db.with_reader(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT entry_type, amount FROM daily_ledger_rollups WHERE day = ?1 AND tag = ?2",
                    rusqlite::params![day, tag],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
                )
                .optional()?)
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn ledger_create_and_delete_reland_their_days_rollups() {
        let (_dir, service) = write_service().await;
        heal_to_june_fifth(service.db()).await;

        // A backdated create lands its day's rollup with the insert.
        let body = to_json(
            service
                .create_ledger_entry("2026-06-02", "expense", "ammo restock", 12.5, "manual")
                .await
                .unwrap(),
        );
        assert_eq!(
            ledger_rollup(service.db(), "2026-06-02", "manual").await,
            Some(("expense".into(), 12.5))
        );

        // The delete relands it empty; a missing id reports not-found.
        let id = body["id"].as_str().unwrap().to_string();
        assert!(service.delete_ledger_entry(&id).await.unwrap());
        assert_eq!(
            ledger_rollup(service.db(), "2026-06-02", "manual").await,
            None
        );
        assert!(!service.delete_ledger_entry("missing").await.unwrap());
    }

    #[tokio::test]
    async fn inventory_sale_relands_the_sold_days_rollup() {
        let (_dir, service) = write_service().await;
        heal_to_june_fifth(service.db()).await;
        service
            .db()
            .with_writer(|conn| {
                conn.execute(
                    "INSERT INTO inventory_items (id, name, tt_value, markup_paid, notes, acquired_at) \
                     VALUES ('i1', 'Gun', 10.0, 2.0, NULL, '2026-05-01')",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        // Sold at a backdated date for an 8.0 markup delta.
        service
            .sell_inventory_item("i1", 20.0, None, Some("2026-06-02"))
            .await
            .unwrap()
            .expect("the item exists");
        assert_eq!(
            ledger_rollup(service.db(), "2026-06-02", INVENTORY_SALE_TAG).await,
            Some(("markup".into(), 8.0))
        );
    }

    #[tokio::test]
    async fn empty_overview_emits_the_engine_typed_zeros() {
        let (_dir, db) = open_env().await;
        let value = to_json(overview_impl(&db, 1_800_000_000.0, "all").await.unwrap());
        // cycledBreakdown is an `Any` field: empty COALESCE sums leave the
        // integer zero on the wire, while the float-declared aggregates coerce.
        assert_eq!(
            to_wire_json(&value),
            "{\"totalReturnRate\":0.0,\"trend\":\"stable\",\"returnsBreakdown\":{\"lootTt\":0.0,\
             \"pes\":0.0,\"codexPes\":0.0,\"questPes\":0.0,\"ledger\":{}},\"lossesBreakdown\":\
             {\"trackingCost\":0.0,\"cycledBreakdown\":{\"weapon\":0,\"healing\":0,\"enhancer\":0,\
             \"armour\":0,\"dangling\":0,\"harvest\":0},\"ledger\":{}},\"totalGains\":0.0,\"totalLosses\":0.0,\
             \"timeline\":[],\"monthlyBreakdown\":[]}"
        );
    }

    #[tokio::test]
    async fn empty_hunting_emits_two_empty_tables() {
        let (_dir, db) = open_env().await;
        let value = to_json(hunting_impl(&db).await.unwrap());
        assert_eq!(
            to_wire_json(&value),
            "{\"mobComparisons\":[],\"tagComparisons\":[]}"
        );
    }

    #[tokio::test]
    async fn empty_harvest_emits_an_empty_tier_table() {
        let (_dir, db) = open_env().await;
        let value = to_json(harvest_impl(&db, None).await.unwrap());
        assert_eq!(to_wire_json(&value), "{\"tierComparisons\":[]}");
    }

    /// Tier is the source activity; tool is a nested strategy. One tool may
    /// span tiers, several tools may share a tier, and tool gaps remain in
    /// the conserved totals.
    #[tokio::test]
    async fn harvest_groups_by_tier_then_tool_without_dropping_toolless_swings() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at) VALUES('hs',1000.0,4600.0)",
                [],
            )?;
            type HarvestFixtureRow<'a> = (&'a str, &'a str, Option<&'a str>, i64, f64, f64);
            let rows: [HarvestFixtureRow<'_>; 6] = [
                ("h1", "long", Some("PH-3"), 1, 0.1, 0.3),
                ("h2", "long", Some("PH-3"), 0, 0.1, 0.0),
                ("h3", "huge", Some("PH-3"), 1, 0.1, 0.06),
                ("h4", "huge", Some("PH-4"), 1, 0.2, 0.1),
                ("h5", "huge", None, 1, 0.05, 0.04),
                ("h6", "short", Some(""), 1, 0.03, 0.02),
            ];
            for (id, tier, tool, success, cost, loot) in rows {
                conn.execute(
                    "INSERT INTO harvest_events(id,session_id,timestamp,success,tool_name,\
                     yield_tier,cost_ped,loot_total_ped) \
                     VALUES(?1,'hs',1000.0,?2,?3,?4,?5,?6)",
                    rusqlite::params![id, success, tool, tier, cost, loot],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        let result = harvest_impl(&db, None).await.unwrap();
        assert_eq!(
            result
                .tier_comparisons
                .iter()
                .map(|tier| tier.swings)
                .sum::<i64>(),
            6
        );
        assert!(
            (result
                .tier_comparisons
                .iter()
                .map(|tier| tier.cycled)
                .sum::<f64>()
                - 0.58)
                .abs()
                < 1e-9
        );
        assert!(
            (result
                .tier_comparisons
                .iter()
                .map(|tier| tier.returns)
                .sum::<f64>()
                - 0.52)
                .abs()
                < 1e-9
        );
        for tier in &result.tier_comparisons {
            assert_eq!(
                tier.tool_comparisons
                    .iter()
                    .map(|tool| tool.swings)
                    .sum::<i64>(),
                tier.swings
            );
            assert!(
                (tier
                    .tool_comparisons
                    .iter()
                    .map(|tool| tool.cycled)
                    .sum::<f64>()
                    - tier.cycled)
                    .abs()
                    < 1e-9
            );
            assert!(
                (tier
                    .tool_comparisons
                    .iter()
                    .map(|tool| tool.returns)
                    .sum::<f64>()
                    - tier.returns)
                    .abs()
                    < 1e-9
            );
        }
        let v = to_json(&result);
        let tiers = v["tierComparisons"].as_array().unwrap();
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0]["yieldTier"], json!("short"));
        assert_eq!(
            tiers[0]["toolComparisons"][0]["name"],
            serde_json::Value::Null
        );
        assert_eq!(tiers[1]["yieldTier"], json!("long"));
        assert_eq!(tiers[1]["swings"], json!(2));
        assert_eq!(tiers[1]["cycled"], json!(0.2));
        assert_eq!(tiers[1]["returns"], json!(0.3));
        assert_eq!(tiers[1]["toolComparisons"][0]["name"], json!("PH-3"));
        assert_eq!(tiers[2]["yieldTier"], json!("huge"));
        assert_eq!(tiers[2]["swings"], json!(3));
        assert_eq!(tiers[2]["cycled"], json!(0.35));
        assert_eq!(tiers[2]["returns"], json!(0.2));
        let huge_tools = tiers[2]["toolComparisons"].as_array().unwrap();
        assert_eq!(huge_tools.len(), 3);
        assert_eq!(huge_tools[0]["name"], json!("PH-4"));
        assert_eq!(huge_tools[1]["name"], json!("PH-3"));
        assert_eq!(huge_tools[2]["name"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn harvest_period_scopes_totals_and_loot_composition_to_the_same_events() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at) VALUES('hs',100.0,2000.0)",
                [],
            )?;
            for (id, timestamp, tool, cost, loot) in [
                ("old", 100.0, "Old Tool", 1.0, 2.0),
                ("recent", 1000.0, "Recent Tool", 3.0, 6.0),
            ] {
                conn.execute(
                    "INSERT INTO harvest_events(id,session_id,timestamp,success,tool_name,\
                     yield_tier,cost_ped,loot_total_ped) \
                     VALUES(?1,'hs',?2,1,?3,'long',?4,?5)",
                    rusqlite::params![id, timestamp, tool, cost, loot],
                )?;
                conn.execute(
                    "INSERT INTO harvest_loot_items(harvest_id,item_name,quantity,value_ped) \
                     VALUES(?1,?2,1,?3)",
                    rusqlite::params![id, format!("{tool} Loot"), loot],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();

        let period = harvest_impl(&db, Some(500.0)).await.unwrap();
        assert_eq!(period.tier_comparisons.len(), 1);
        let row = &period.tier_comparisons[0];
        assert_eq!(row.yield_tier, HarvestYieldTier::Long);
        assert_eq!(row.cycled, 3.0);
        assert_eq!(row.returns, 6.0);
        assert_eq!(row.loot_items.len(), 1);
        assert_eq!(row.loot_items[0].item_name, "Recent Tool Loot");
        assert_eq!(row.loot_items[0].value_ped, 6.0);
        assert_eq!(row.tool_comparisons.len(), 1);
        assert_eq!(row.tool_comparisons[0].name.as_deref(), Some("Recent Tool"));
    }

    /// Per-tool loot composition: active items only, grouped by the
    /// recording tool and ordered TT-descending, with deactivated loot
    /// excluded.
    #[tokio::test]
    async fn harvest_composition_groups_active_loot_by_tool() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at) VALUES('hs',1000.0,4600.0)",
                [],
            )?;
            // Two swings on Axe A, one on Axe B.
            for (id, tier, tool) in [
                ("h1", "huge", "Axe A"),
                ("h2", "huge", "Axe A"),
                ("h3", "short", "Axe B"),
            ] {
                conn.execute(
                    "INSERT INTO harvest_events(id,session_id,timestamp,success,tool_name,\
                     yield_tier,cost_ped,loot_total_ped) \
                     VALUES(?1,'hs',1000.0,1,?2,?3,0.1,1.0)",
                    rusqlite::params![id, tool, tier],
                )?;
            }
            // Loot: Axe A pulled Long Moonleaf Board (higher TT) and Wood
            // Shavings; one deactivated Wood Shavings row is excluded. Axe B
            // pulled Short Moonleaf Board only.
            let loot: [(&str, &str, i64, f64, Option<f64>); 4] = [
                ("h1", "Long Moonleaf Board", 2, 0.8, None),
                ("h1", "Wood Shavings", 5, 0.2, None),
                ("h2", "Wood Shavings", 3, 0.15, Some(2000.0)),
                ("h3", "Short Moonleaf Board", 4, 0.5, None),
            ];
            for (hid, item, qty, val, deact) in loot {
                conn.execute(
                    "INSERT INTO harvest_loot_items(harvest_id,item_name,quantity,value_ped,deactivated_at) \
                     VALUES(?1,?2,?3,?4,?5)",
                    rusqlite::params![hid, item, qty, val, deact],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        let result = harvest_impl(&db, None).await.unwrap();
        for tier in &result.tier_comparisons {
            let mut tool_items = std::collections::BTreeMap::<String, (i64, f64)>::new();
            for tool in &tier.tool_comparisons {
                for item in &tool.loot_items {
                    let total = tool_items.entry(item.item_name.clone()).or_default();
                    total.0 += item.quantity;
                    total.1 += item.value_ped;
                }
            }
            let tier_items: std::collections::BTreeMap<_, _> = tier
                .loot_items
                .iter()
                .map(|item| (item.item_name.clone(), (item.quantity, item.value_ped)))
                .collect();
            assert_eq!(tool_items, tier_items);
        }
        let v = to_json(&result);
        let tiers = v["tierComparisons"].as_array().unwrap();
        // Axe A: two active items, TT-desc (Long Moonleaf Board first);
        // the deactivated Wood Shavings row is excluded from its total.
        let short = &tiers[0];
        assert_eq!(short["yieldTier"], json!("short"));
        let huge = &tiers[1];
        assert_eq!(huge["yieldTier"], json!("huge"));
        let a = &huge["toolComparisons"][0];
        assert_eq!(a["name"], json!("Axe A"));
        let a_items = a["lootItems"].as_array().unwrap();
        assert_eq!(a_items.len(), 2);
        assert_eq!(a_items[0]["itemName"], json!("Long Moonleaf Board"));
        assert_eq!(a_items[0]["quantity"], json!(2));
        assert_eq!(a_items[0]["valuePed"], json!(0.8));
        assert_eq!(a_items[1]["itemName"], json!("Wood Shavings"));
        assert_eq!(a_items[1]["quantity"], json!(5));
        assert_eq!(a_items[1]["valuePed"], json!(0.2));
        // Axe B: one item.
        let b = &short["toolComparisons"][0];
        assert_eq!(b["name"], json!("Axe B"));
        let b_items = b["lootItems"].as_array().unwrap();
        assert_eq!(b_items.len(), 1);
        assert_eq!(b_items[0]["itemName"], json!("Short Moonleaf Board"));
    }

    /// Seed the representative scenario the live probe grounded, with the
    /// window relative to a fixed `now`, and assert the computed aggregates,
    /// the trend, dominance, and the filters.
    async fn seed_scenario(db: &Db, now: f64) {
        let day = 86400.0;
        let recent = now - 11.0 * day; // inside the 30d window
        let prior = now - 37.0 * day; // inside the 30-60d window
        let recent_iso = epoch_to_iso(recent);
        let prior_iso = epoch_to_iso(prior);
        db.with_writer(move |conn| {
            // sessions
            for (id, start, armour, heal, dangling) in [
                ("sess-a", recent, 1.0, 2.0, 0.5),
                ("sess-b", prior, 0.5, 1.0, 0.0),
                ("sess-z", recent, 0.0, 0.0, 0.0), // zero-kill, zero-cost: filtered from activity
            ] {
                conn.execute(
                    "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                     VALUES(?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![id, start, start + 3600.0, armour, heal, dangling],
                )
                .expect("seed");
            }
            for i in 0..5 {
                let kid = format!("k-a-{i}");
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    rusqlite::params![kid, "sess-a", "Atrox", "Atrox", "Young", recent + i as f64, 0.1, 10.0],
                )
                .expect("seed");
                conn.execute(
                    "INSERT INTO kill_tool_stats(kill_id,tool_name,shots_fired,cost_per_shot) VALUES(?1,?2,?3,?4)",
                    rusqlite::params![kid, "Opalo", 50_i64, 0.011],
                )
                .expect("seed");
            }
            for i in 0..3 {
                let kid = format!("k-b-{i}");
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,?2,?3,NULL,NULL,?4,?5,?6)",
                    rusqlite::params![kid, "sess-b", "Thing", prior + i as f64, 0.0, 5.0],
                )
                .expect("seed");
                conn.execute(
                    "INSERT INTO kill_tool_stats(kill_id,tool_name,shots_fired,cost_per_shot) VALUES(?1,?2,?3,?4)",
                    rusqlite::params![kid, "Opalo", 30_i64, 0.01],
                )
                .expect("seed");
            }
            conn.execute(
                "INSERT INTO skill_gains(session_id,timestamp,skill_name,amount,ped_value) \
                 VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params!["sess-a", recent, "Laser Weaponry Technology", 1.0, 3.0],
            )
            .expect("seed");
            conn.execute(
                "INSERT INTO skill_gains(session_id,timestamp,skill_name,amount,ped_value) \
                 VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params!["sess-b", prior, "Laser Weaponry Technology", 1.0, 1.0],
            )
            .expect("seed");
            conn.execute(
                "INSERT INTO codex_claims(species_name,rank,skill_name,claimed_at,ped_value) \
                 VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params!["Atrox", 1_i64, "Rifle", recent, 7.0],
            )
            .expect("seed");
            conn.execute(
                "INSERT INTO quest_claims(quest_name,claimed_at,ped_value) VALUES(?1,?2,?3)",
                rusqlite::params!["A Quest", recent, 4.0],
            )
            .expect("seed");
            // ledger: a recent markup and a prior expense, dated by the ISO form.
            conn.execute(
                "INSERT INTO ledger_entries(id,date,type,description,amount,tag) VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params!["led-1", recent_iso, "markup", "Sold hides", 12.5, "loot_sale"],
            )
            .expect("seed");
            conn.execute(
                "INSERT INTO ledger_entries(id,date,type,description,amount,tag) VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params!["led-2", prior_iso, "expense", "Deposit", 8.0, "deposit"],
            )
            .expect("seed");
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn seeded_overview_aggregates_match() {
        let now = 1_800_000_000.0;
        let (_dir, db) = open_env().await;
        seed_scenario(&db, now).await;
        let v = to_json(overview_impl(&db, now, "all").await.unwrap());
        assert_eq!(v["returnsBreakdown"]["lootTt"], json!(65.0));
        assert_eq!(v["returnsBreakdown"]["pes"], json!(4.0));
        assert_eq!(v["returnsBreakdown"]["codexPes"], json!(7.0));
        assert_eq!(v["returnsBreakdown"]["ledger"]["loot_sale"], json!(12.5));
        assert_eq!(v["lossesBreakdown"]["trackingCost"], json!(9.15));
        assert_eq!(
            v["lossesBreakdown"]["cycledBreakdown"]["weapon"],
            json!(3.65)
        );
        assert_eq!(
            v["lossesBreakdown"]["cycledBreakdown"]["armour"],
            json!(1.5)
        );
        assert_eq!(v["lossesBreakdown"]["ledger"]["deposit"], json!(8.0));
        // totalGains = loot 65 + markup 12.5; totalLosses = cost 9.15 + expense 8.0.
        assert_eq!(v["totalGains"], json!(77.5));
        assert_eq!(v["totalLosses"], json!(17.15));
        assert_eq!(v["totalReturnRate"], json!(4.519));
        // timeline points carry the day bucket; monthly points the month
        // (the facade labels them "date" / "month").
        assert!(v["timeline"][0]["bucket"]
            .as_str()
            .is_some_and(|b| b.len() == 10));
        assert!(v["monthlyBreakdown"][0]["bucket"]
            .as_str()
            .is_some_and(|b| b.len() == 7));
        // trend: recent-30d rate exceeds prior-30d rate beyond the 2% band.
        assert_eq!(v["trend"], json!("improving"));
        // period filter: 30d keeps only the recent window (markup in, expense out).
        let v30 = to_json(overview_impl(&db, now, "30d").await.unwrap());
        assert_eq!(v30["returnsBreakdown"]["lootTt"], json!(50.0));
        assert_eq!(v30["returnsBreakdown"]["ledger"]["loot_sale"], json!(12.5));
        assert_eq!(v30["lossesBreakdown"]["ledger"], json!({}));
        assert_eq!(v30["timeline"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn seeded_hunting_dominance_and_filters() {
        let now = 1_800_000_000.0;
        let (_dir, db) = open_env().await;
        seed_scenario(&db, now).await;
        let v = to_json(hunting_impl(&db).await.unwrap());
        // sess-z (zero kills) filtered out; sess-a -> dominant mob, sess-b -> tag.
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["name"], json!("Atrox"));
        assert_eq!(mobs[0]["kills"], json!(5));
        assert_eq!(mobs[0]["hours"], json!(1.0)); // 3600s / 3600
        assert_eq!(mobs[0]["cycled"], json!(6.75));
        // pesPer100Ped = (skill 3.0 / cycled 6.75) * 100; lootRate = loot 50 / cycled.
        assert_eq!(mobs[0]["pesPer100Ped"], json!(44.44));
        assert_eq!(mobs[0]["lootRate"], json!(7.4074));
        let tags = v["tagComparisons"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0]["name"], json!("Thing"));
        assert_eq!(tags[0]["kills"], json!(3));
        assert_eq!(tags[0]["cycled"], json!(2.4));
        assert_eq!(tags[0]["pesPer100Ped"], json!(41.67));
        assert_eq!(tags[0]["lootRate"], json!(6.25));
    }

    /// The activity filter drops a session failing ANY of the three guards
    /// (duration > 0, cycled > 0, kills > 0); `||` not `&&`. Three sessions,
    /// each dominated by its own mob, each failing exactly one guard except
    /// the keeper: only the keeper's mob survives.
    #[tokio::test]
    async fn activity_filter_drops_a_session_failing_any_single_guard() {
        let (_dir, db) = open_env().await;
        // keeper: kills, duration, cost all positive.
        seed_filter_session(&db, "keep", "Keeper", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // zero cost -> cycled 0 -> dropped by the cycled guard alone.
        seed_filter_session(&db, "zcost", "Zerocost", 1000.0, 1000.0 + 3600.0, 0.0, 2).await;
        // zero duration (start == end) -> dropped by the duration guard alone.
        seed_filter_session(&db, "zdur", "Zerodur", 1000.0, 1000.0, 5.0, 2).await;
        let v = to_json(hunting_impl(&db).await.unwrap());
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1, "only the keeper survives the OR filter");
        assert_eq!(mobs[0]["name"], json!("Keeper"));
    }

    async fn seed_filter_session(
        db: &Db,
        id: &str,
        mob: &str,
        start: f64,
        end: f64,
        armour: f64,
        kills: i64,
    ) {
        let id = id.to_string();
        let mob = mob.to_string();
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                 VALUES(?1,?2,?3,?4,0,0)",
                rusqlite::params![id, start, end, armour],
            )
            .expect("seed");
            for i in 0..kills {
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    rusqlite::params![format!("{id}-k{i}"), id, mob, "Spec", "Young", start + i as f64, 0.0, 1.0],
                )
                .expect("seed");
            }
            Ok(())
        })
        .await
        .unwrap();
    }

    /// Seed one session (cost via armour) and `kills` loot rows at `ts`, so a
    /// window's rate is loot_total / armour_cost.
    async fn seed_rate(db: &Db, id: &str, ts: f64, cost: f64, kills: i64, loot: f64) {
        let id = id.to_string();
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                 VALUES(?1,?2,?3,?4,0,0)",
                rusqlite::params![id, ts, ts + 3600.0, cost],
            )
            .expect("seed");
            for i in 0..kills {
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,?2,?3,?4,0,?5)",
                    rusqlite::params![format!("{id}-k{i}"), id, "M", ts + i as f64, loot],
                )
                .expect("seed");
            }
            Ok(())
        })
        .await
        .unwrap();
    }

    /// The trend compares the recent-30d rate against the prior-30d rate with
    /// a +/-2% band, guarded by both rates being positive.
    #[tokio::test]
    async fn overview_trend_bands() {
        let now = 1_800_000_000.0;
        let day = 86400.0;
        let trend = |v: OverviewData| json!(v.trend);

        // declining: recent rate 1.0 (10/10) below prior 2.0 (20/10) * 0.98.
        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("declining")
        );

        // improving: recent 2.0 above prior 1.0 * 1.02.
        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("improving")
        );

        // stable: recent equals prior, inside the band.
        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("stable")
        );

        // zero recent rate: the positivity guard short-circuits to stable
        // (a mutated guard would fall through into the banding and declare a
        // direction).
        let (_dir, db) = open_env().await;
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("stable")
        );

        // zero prior rate: the other half of the guard.
        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("stable")
        );
    }

    /// Dominance needs the top group at or above 60% of known kills, and the
    /// species/maturity presence decides mob vs tag.
    #[tokio::test]
    async fn activity_dominance_threshold_and_tag_split() {
        // Non-dominant: three distinct mobs, one kill each (33% each, below
        // the 0.6 floor) -> no dominant element, no comparison rows.
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                 VALUES('nd',1000.0,4600.0,5.0,0,0)",
                [],
            )?;
            for (i, mob) in ["Alpha", "Bravo", "Charlie"].iter().enumerate() {
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,'nd',?2,'Spec','Young',?3,0,1.0)",
                    rusqlite::params![format!("nd-{i}"), mob, 1000.0 + i as f64],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        let v = to_json(hunting_impl(&db).await.unwrap());
        assert_eq!(v["mobComparisons"].as_array().unwrap().len(), 0);
        assert_eq!(v["tagComparisons"].as_array().unwrap().len(), 0);

        // Asymmetric: species present, maturity empty -> still a mob (the
        // presence test is OR, not AND), so it lands in mobComparisons.
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                 VALUES('as',1000.0,4600.0,5.0,0,0)",
                [],
            )?;
            for i in 0..2 {
                conn.execute(
                    "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                     VALUES(?1,'as','Foo','Bar','',?2,0,1.0)",
                    rusqlite::params![format!("as-{i}"), 1000.0 + i as f64],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        let v = to_json(hunting_impl(&db).await.unwrap());
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["name"], json!("Foo"));
        assert_eq!(v["tagComparisons"].as_array().unwrap().len(), 0);
    }

    /// A kill referencing a session that does not exist (representable with
    /// foreign keys off, as the app runs) is not counted: it belongs to no
    /// session, so it never enters any session's aggregate.
    #[tokio::test]
    async fn activity_ignores_a_kill_for_a_missing_session() {
        let (_dir, db) = open_env().await;
        // A valid completed session with one dominant-mob kill.
        seed_filter_session(&db, "ok", "Real", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // An orphan kill whose session_id matches no tracking_sessions row.
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES('orphan','ghost-session','Ghost','Spec','Young',1.0,0,9.0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        // Only the real session's mob is compared; the orphan is ignored.
        let v = to_json(hunting_impl(&db).await.unwrap());
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["name"], json!("Real"));
    }

    #[test]
    fn period_epoch_maps_named_windows_only() {
        let now = 1_000_000.0;
        assert_eq!(period_epoch("all", now), None);
        assert_eq!(period_epoch("bogus", now), None);
        assert_eq!(period_epoch("30d", now), Some(now - 30.0 * 86400.0));
        assert_eq!(period_epoch("90d", now), Some(now - 90.0 * 86400.0));
        assert_eq!(period_epoch("1y", now), Some(now - 365.0 * 86400.0));
    }

    #[test]
    fn sql_number_float_form_coerces_integers_only() {
        assert_eq!(SqlNumber::Int(0).as_f64(), 0.0);
        assert_eq!(SqlNumber::Int(3).as_f64(), 3.0);
        assert_eq!(SqlNumber::Float(1.5).as_f64(), 1.5);
    }

    #[test]
    fn sql_number_rounding_preserves_integers_and_banker_rounds_floats() {
        assert_eq!(SqlNumber::Int(0).rounded(2), SqlNumber::Int(0)); // int stays int
        assert_eq!(SqlNumber::Float(1.005).rounded(2), SqlNumber::Float(1.0)); // half-even
        assert_eq!(SqlNumber::Float(2.675).rounded(2), SqlNumber::Float(2.67));
    }

    #[test]
    fn sql_number_sum_is_integral_only_when_both_are() {
        assert_eq!(SqlNumber::Int(2).sum(SqlNumber::Int(3)), SqlNumber::Int(5));
        assert_eq!(
            SqlNumber::Int(2).sum(SqlNumber::Float(0.5)),
            SqlNumber::Float(2.5)
        );
        // The engine typing serialises untagged: ints as ints.
        assert_eq!(to_json(SqlNumber::Int(0)), json!(0));
        assert_eq!(to_json(SqlNumber::Float(0.0)), json!(0.0));
    }

    // ── Hermetic write-handler tests (the mutation campaign's kills) ──

    /// Create then list round-trips for the ledger: the create echoes the
    /// input plus a generated id, and the list reads it back.
    #[tokio::test]
    async fn ledger_create_and_list_round_trip() {
        let (_dir, service) = write_service().await;
        let body = to_json(
            service
                .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
                .await
                .unwrap(),
        );
        assert_eq!(body["date"], json!("2026-05-01"));
        assert_eq!(body["type"], json!("expense"));
        assert_eq!(body["amount"], json!(12.5));
        assert_eq!(body["tag"], json!("ammo"));
        assert!(body["id"].as_str().is_some(), "create generates an id");

        let page = service.list_ledger(None, None).await.unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].description, "Ammo");
        assert_eq!(json!(page.entries[0].id), body["id"]);
    }

    /// Keyset pagination walks the whole ledger newest-first, one bounded
    /// page at a time, following the `next_cursor` with no overlap and no
    /// gaps.
    #[tokio::test]
    async fn ledger_list_walks_every_entry_by_keyset_cursor() {
        let (_dir, service) = write_service().await;
        for day in ["01", "02", "03", "04", "05"] {
            service
                .create_ledger_entry(
                    &format!("2026-05-{day}"),
                    "expense",
                    &format!("e{day}"),
                    1.0,
                    "t",
                )
                .await
                .unwrap();
        }

        let mut seen: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        // A generous cap: three pages of two cover five entries; more than
        // this means the cursor is not converging.
        for _ in 0..10 {
            let page = service
                .list_ledger(cursor.as_deref(), Some(2))
                .await
                .unwrap();
            assert!(page.entries.len() <= 2, "the page is bounded by the limit");
            for row in &page.entries {
                seen.push(row.description.clone());
            }
            match page.next_cursor {
                Some(token) => cursor = Some(token),
                None => break,
            }
        }
        assert_eq!(
            seen,
            ["e05", "e04", "e03", "e02", "e01"],
            "every entry appears once, newest first, across pages"
        );
    }

    #[tokio::test]
    async fn ledger_list_rejects_a_malformed_cursor() {
        let (_dir, service) = write_service().await;
        assert!(matches!(
            service.list_ledger(Some("not a cursor!"), None).await,
            Err(AnalyticsError::InvalidCursor)
        ));
    }

    /// The preset type guard: only 'expense'/'markup' pass; anything else is
    /// [`AnalyticsError::InvalidPresetType`] and writes nothing.
    #[tokio::test]
    async fn preset_create_validates_type() {
        let (_dir, service) = write_service().await;
        for kind in ["expense", "markup"] {
            service
                .create_ledger_preset("P", kind, "d", 1.0, "t")
                .await
                .unwrap_or_else(|_| panic!("{kind} accepted"));
        }
        assert!(matches!(
            service
                .create_ledger_preset("Bad", "income", "d", 1.0, "t")
                .await,
            Err(AnalyticsError::InvalidPresetType)
        ));
        // Only the two valid presets were written.
        assert_eq!(service.list_ledger_presets().await.unwrap().len(), 2);
    }

    /// Seed one harvest session whose swings yield boards across two tiers,
    /// giving `Moonleaf Board` a 60/40 open position to allocate against.
    async fn seed_board_stock(service: &AnalyticsService) {
        service
            .db
            .with_writer(|conn| {
                for (id, tier, quantity, tt) in
                    [("hs1", "long", 60, 1.80), ("hs2", "huge", 40, 1.20)]
                {
                    conn.execute(
                        "INSERT INTO harvest_events(id,session_id,timestamp,success,tool_name,\
                         yield_tier,cost_ped,loot_total_ped) \
                         VALUES(?1,'sale-s',1000.0,1,'PH-3',?2,0.1,?3)",
                        rusqlite::params![id, tier, tt],
                    )?;
                    conn.execute(
                        "INSERT INTO harvest_loot_items(harvest_id,item_name,quantity,value_ped) \
                         VALUES(?1,'Moonleaf Board',?2,?3)",
                        rusqlite::params![id, quantity, tt],
                    )?;
                }
                Ok(())
            })
            .await
            .unwrap();
    }

    fn position(rows: &[StockPositionRow], item: &str) -> Option<StockPositionRow> {
        rows.iter().find(|row| row.item_name == item).cloned()
    }

    /// Listing removes the stock immediately, because in game it has left the
    /// player's inventory, and spends the starting-bid fee immediately too.
    /// Nothing is realised: the auction has not closed.
    #[tokio::test]
    async fn listing_removes_stock_and_spends_the_fee_without_realising_markup() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let before = service.stock_positions().await.unwrap();
        assert_eq!(position(&before, "Moonleaf Board").unwrap().quantity, 100.0);

        let listing = service
            .create_auction_listing(
                "Moonleaf Board",
                50.0,
                2.0,
                Some(4.0),
                0.5,
                Some("2026-07-20"),
            )
            .await
            .unwrap();
        assert_eq!(listing.status, "pending");
        assert!((listing.attributed_qty - 50.0).abs() < 1e-9);
        assert!(listing.unattributed_qty.abs() < 1e-9);
        assert_eq!(
            listing.activity_net_markup, None,
            "an open auction has no realised figure"
        );

        let after = position(&service.stock_positions().await.unwrap(), "Moonleaf Board").unwrap();
        assert!((after.quantity - 50.0).abs() < 1e-9);
        assert!(
            (after.listed_quantity - 50.0).abs() < 1e-9,
            "listed stock is reported, not just absent"
        );

        // The fee is a real, dated ledger expense the moment it is charged.
        let ledger = service.list_ledger(None, None).await.unwrap();
        let fee = ledger
            .entries
            .iter()
            .find(|entry| entry.description == "Auction Fee: Moonleaf Board")
            .expect("the listing fee reached the ledger");
        assert_eq!(fee.kind, "expense");
        assert!((fee.amount - 0.5).abs() < 1e-9);
        assert_eq!(fee.date, "2026-07-20");

        // Nothing is attributed to any activity yet.
        assert!(service.realised_markup_by_tier().await.unwrap().is_empty());
    }

    /// A listed quantity is drawn from every tier in proportion to what that
    /// tier still holds, never by picking source units or by FIFO.
    #[tokio::test]
    async fn listing_allocates_across_tiers_by_open_composition() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let listing = service
            .create_auction_listing("Moonleaf Board", 50.0, 2.0, None, 0.5, None)
            .await
            .unwrap();

        let split: Vec<(String, f64)> = service
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT yield_tier, -quantity FROM stock_movements \
                     WHERE ref_id = ? AND movement_kind = 'listing' ORDER BY yield_tier",
                )?;
                let rows = stmt
                    .query_map(rusqlite::params![listing.id], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .unwrap();

        // 60/40 open, so 50 listed splits 30/20.
        assert_eq!(split.len(), 2);
        let huge = split.iter().find(|(tier, _)| tier == "huge").unwrap().1;
        let long = split.iter().find(|(tier, _)| tier == "long").unwrap().1;
        assert!((long - 30.0).abs() < 1e-9, "long tier got {long}");
        assert!((huge - 20.0).abs() < 1e-9, "huge tier got {huge}");
    }

    /// Confirming a sale realises markup, writes only the money that was not
    /// already booked as loot TT, and divides the activity's share across the
    /// tiers that supplied the listing.
    #[tokio::test]
    async fn confirming_a_sale_realises_markup_and_attributes_it_by_tier() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let listing = service
            .create_auction_listing("Moonleaf Board", 50.0, 2.0, None, 0.5, Some("2026-07-20"))
            .await
            .unwrap();
        // 50 boards at 0.03 TT each.
        assert!((listing.tt_value - 1.5).abs() < 1e-9);

        let sold = service
            .confirm_auction_listing(&listing.id, 5.0, 0.2, Some("2026-07-22"))
            .await
            .unwrap()
            .expect("the pending listing resolved");
        assert_eq!(sold.status, "sold");
        // Gross 5.00 less 1.50 TT is 3.50, then less 0.70 of fees.
        assert!((sold.gross_markup.unwrap() - 3.5).abs() < 1e-9);
        assert!((sold.activity_net_markup.unwrap() - 2.8).abs() < 1e-9);

        // The ledger gains the uplift only: the TT counted as loot already.
        let ledger = service.list_ledger(None, None).await.unwrap();
        let sale = ledger
            .entries
            .iter()
            .find(|entry| entry.description == "Auction Sale: Moonleaf Board")
            .expect("the sale reached the ledger");
        assert_eq!(sale.kind, "markup");
        assert!((sale.amount - 3.5).abs() < 1e-9);
        assert_eq!(sale.date, "2026-07-22");

        // Both fees are ledger expenses, dated when each was charged.
        let fees = ledger
            .entries
            .iter()
            .filter(|entry| entry.description == "Auction Fee: Moonleaf Board")
            .count();
        assert_eq!(fees, 2, "listing fee and point-of-sale fee");

        // The activity's share divides 30/20 across the contributing tiers.
        let realised = service.realised_markup_by_tier().await.unwrap();
        let by_tier: std::collections::BTreeMap<&str, f64> = realised
            .iter()
            .map(|row| (row.yield_tier.as_str(), row.net_markup))
            .collect();
        assert!((by_tier["long"] - 2.8 * 0.6).abs() < 1e-9);
        assert!((by_tier["huge"] - 2.8 * 0.4).abs() < 1e-9);
        let total: f64 = realised.iter().map(|row| row.net_markup).sum();
        assert!(
            (total - 2.8).abs() < 1e-9,
            "the tier split sums to the whole"
        );
    }

    /// An expired listing returns the stock intact, keeps the fee spent, and
    /// attributes nothing: failing to sell describes market execution, not
    /// the gameplay that produced the loot.
    #[tokio::test]
    async fn expiring_a_listing_returns_stock_and_attributes_nothing() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let listing = service
            .create_auction_listing("Moonleaf Board", 50.0, 2.0, None, 0.5, Some("2026-07-20"))
            .await
            .unwrap();
        let expired = service
            .expire_auction_listing(&listing.id, Some("2026-07-25"))
            .await
            .unwrap()
            .expect("the pending listing resolved");
        assert_eq!(expired.status, "expired");
        assert_eq!(expired.activity_net_markup, None);

        let after = position(&service.stock_positions().await.unwrap(), "Moonleaf Board").unwrap();
        assert!(
            (after.quantity - 100.0).abs() < 1e-9,
            "the stock came back whole"
        );
        assert!(after.listed_quantity.abs() < 1e-9);

        // The fee stays spent, and no activity may claim the loss.
        let ledger = service.list_ledger(None, None).await.unwrap();
        assert!(ledger
            .entries
            .iter()
            .any(|entry| entry.description == "Auction Fee: Moonleaf Board"));
        assert!(service.realised_markup_by_tier().await.unwrap().is_empty());

        // The original allocation survives for audit rather than being erased.
        let listing_rows: i64 = service
            .db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM stock_movements WHERE movement_kind = 'listing'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(
            listing_rows, 2,
            "the listing's rows stay; the return is new rows"
        );
    }

    /// A resolved listing cannot resolve again, in either direction.
    #[tokio::test]
    async fn a_resolved_listing_cannot_be_resolved_twice() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let listing = service
            .create_auction_listing("Moonleaf Board", 10.0, 1.0, None, 0.5, None)
            .await
            .unwrap();
        service
            .confirm_auction_listing(&listing.id, 2.0, 0.0, None)
            .await
            .unwrap()
            .expect("first confirmation lands");

        assert!(service
            .confirm_auction_listing(&listing.id, 9.0, 0.0, None)
            .await
            .unwrap()
            .is_none());
        assert!(service
            .expire_auction_listing(&listing.id, None)
            .await
            .unwrap()
            .is_none());
    }

    /// Selling more than is tracked keeps the tracked part attributed and
    /// leaves the rest explicitly unattributed, so the activity is never
    /// credited with output it did not produce.
    #[tokio::test]
    async fn selling_beyond_tracked_stock_attributes_only_the_tracked_part() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        let listing = service
            .create_auction_listing("Moonleaf Board", 150.0, 3.0, None, 0.5, None)
            .await
            .unwrap();
        assert!((listing.attributed_qty - 100.0).abs() < 1e-9);
        assert!((listing.unattributed_qty - 50.0).abs() < 1e-9);

        let sold = service
            .confirm_auction_listing(&listing.id, 9.0, 0.0, None)
            .await
            .unwrap()
            .unwrap();
        // Two thirds of the listing was tracked, so two thirds of the net
        // markup may be claimed.
        let expected = (9.0 - 4.5 - 0.5) * (3.0 / 4.5);
        assert!((sold.activity_net_markup.unwrap() - expected).abs() < 1e-9);

        // The ledger records the full proceeds less only the TT that had
        // already been booked as loot.
        let ledger = service.list_ledger(None, None).await.unwrap();
        let sale = ledger
            .entries
            .iter()
            .find(|entry| entry.description == "Auction Sale: Moonleaf Board")
            .unwrap();
        assert!((sale.amount - (9.0 - 3.0)).abs() < 1e-9);
    }

    /// Recycling preserves TT exactly and carries the source's activity
    /// composition into the produced item, so selling the result still
    /// attributes back to the tiers that grew it.
    #[tokio::test]
    async fn conversion_preserves_tt_and_carries_provenance_forward() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        service
            .convert_stock("Moonleaf Board", "Nanocube", 50.0, Some("2026-07-21"))
            .await
            .unwrap();

        let rows = service.stock_positions().await.unwrap();
        let source = position(&rows, "Moonleaf Board").unwrap();
        let produced = position(&rows, "Nanocube").expect("the conversion created stock");
        assert!((source.quantity - 50.0).abs() < 1e-9);
        // 50 boards at 0.03 TT is 1.50 PED, preserved 1:1.
        assert!((source.tt_value - 1.5).abs() < 1e-9);
        assert!((produced.tt_value - 1.5).abs() < 1e-9);

        // Selling the produced stock attributes back to the original tiers.
        let listing = service
            .create_auction_listing("Nanocube", produced.quantity, 2.0, None, 0.0, None)
            .await
            .unwrap();
        service
            .confirm_auction_listing(&listing.id, 2.5, 0.0, None)
            .await
            .unwrap()
            .unwrap();

        let realised = service.realised_markup_by_tier().await.unwrap();
        let total: f64 = realised.iter().map(|row| row.net_markup).sum();
        assert_eq!(realised.len(), 2, "both source tiers still carry the sale");
        assert!(
            (total - 1.0).abs() < 1e-9,
            "a 2.50 sale on 1.50 TT with no fees"
        );
    }

    /// A position that has fully closed disappears rather than lingering as
    /// a zero row.
    #[tokio::test]
    async fn a_fully_sold_position_leaves_the_stock_list() {
        let (_dir, service) = write_service().await;
        seed_board_stock(&service).await;

        service
            .create_auction_listing("Moonleaf Board", 100.0, 3.0, None, 0.0, None)
            .await
            .unwrap();
        let rows = service.stock_positions().await.unwrap();
        assert!(position(&rows, "Moonleaf Board").is_none());
    }

    /// Create with the optional fields absent: notes is null and acquired_at
    /// defaults to the (frozen) clock's UTC date.
    #[tokio::test]
    async fn inventory_create_defaults_date_and_notes() {
        let (_dir, service) = write_service().await;
        let body = to_json(
            service
                .create_inventory_item("Imk2", 50.0, 5.0, None, None)
                .await
                .unwrap(),
        );
        // Response is camelCase even though the request is snake_case.
        assert_eq!(body["ttValue"], json!(50.0));
        assert_eq!(body["markupPaid"], json!(5.0));
        assert_eq!(body["notes"], Value::Null);
        assert_eq!(body["acquiredAt"], json!("2026-06-01"));

        // An explicit acquired_at / notes are honoured.
        let body = to_json(
            service
                .create_inventory_item("X", 1.0, 0.0, Some("spare"), Some("2026-01-02"))
                .await
                .unwrap(),
        );
        assert_eq!(body["notes"], json!("spare"));
        assert_eq!(body["acquiredAt"], json!("2026-01-02"));
    }

    /// PATCH field-selection: only PROVIDED (Some) fields update; a None
    /// field is left untouched, so the statement carries only the fields
    /// the patch actually names.
    #[tokio::test]
    async fn inventory_patch_updates_only_provided_fields() {
        let (_dir, service) = write_service().await;
        let created = to_json(
            service
                .create_inventory_item("Orig", 20.0, 3.0, Some("keep"), Some("2026-03-01"))
                .await
                .unwrap(),
        );
        let id = created["id"].as_str().unwrap().to_string();

        // Provide name + tt_value only: markup_paid and notes stay.
        let patched = to_json(
            service
                .update_inventory_item(&id, Some("Renamed"), Some(25.0), None, None)
                .await
                .unwrap()
                .expect("the item exists"),
        );
        assert_eq!(patched["name"], json!("Renamed"));
        assert_eq!(patched["ttValue"], json!(25.0));
        assert_eq!(patched["markupPaid"], json!(3.0), "untouched");
        assert_eq!(patched["notes"], json!("keep"), "untouched");

        // An all-None patch re-reads and returns the row unchanged.
        let same = to_json(
            service
                .update_inventory_item(&id, None, None, None, None)
                .await
                .unwrap()
                .expect("the item exists"),
        );
        assert_eq!(same, patched);

        // Patch a missing id -> not found.
        assert!(service
            .update_inventory_item("no-such", Some("Z"), None, None, None)
            .await
            .unwrap()
            .is_none());
    }

    /// Sell a created item, asserting the delta/type/description-default
    /// branch for profit / loss / zero-delta and the atomic item removal.
    #[tokio::test]
    async fn sell_emits_the_right_delta_branch() {
        // PROFIT: sale 20 over cost 12 -> markup 8.0; default description.
        let (_dir, service) = write_service().await;
        let item = to_json(
            service
                .create_inventory_item("Sword", 10.0, 2.0, None, Some("2026-02-01"))
                .await
                .unwrap(),
        );
        let id = item["id"].as_str().unwrap().to_string();
        let body = to_json(
            service
                .sell_inventory_item(&id, 20.0, None, Some("2026-05-10"))
                .await
                .unwrap()
                .expect("the item exists"),
        );
        let entry = &body["ledgerEntry"];
        assert_eq!(entry["type"], json!("markup"));
        assert_eq!(entry["amount"], json!(8.0));
        assert_eq!(entry["tag"], json!("inventory_sale"));
        assert_eq!(entry["date"], json!("2026-05-10"));
        assert_eq!(
            entry["description"],
            json!("Inventory Sale: Sword"),
            "default description form"
        );
        assert_eq!(body["soldItem"]["name"], json!("Sword"));
        // Item removed; the emitted ledger row is the only one.
        assert_eq!(service.list_inventory().await.unwrap().len(), 0);
        assert_eq!(
            service.list_ledger(None, None).await.unwrap().entries.len(),
            1
        );

        // LOSS: sale 5 under cost 12 -> expense 7.0; explicit description.
        let (_dir, service) = write_service().await;
        let item = to_json(
            service
                .create_inventory_item("Shield", 10.0, 2.0, None, Some("2026-02-01"))
                .await
                .unwrap(),
        );
        let id = item["id"].as_str().unwrap().to_string();
        let body = to_json(
            service
                .sell_inventory_item(&id, 5.0, Some("Dumped it"), None)
                .await
                .unwrap()
                .expect("the item exists"),
        );
        let entry = &body["ledgerEntry"];
        assert_eq!(entry["type"], json!("expense"));
        assert_eq!(entry["amount"], json!(7.0));
        assert_eq!(entry["description"], json!("Dumped it"));
        // Default sold_at is the frozen clock date.
        assert_eq!(entry["date"], json!("2026-06-01"));

        // ZERO-DELTA: sale == cost -> no ledger entry, item still removed.
        let (_dir, service) = write_service().await;
        let item = to_json(
            service
                .create_inventory_item("Even", 8.0, 2.0, None, Some("2026-02-01"))
                .await
                .unwrap(),
        );
        let id = item["id"].as_str().unwrap().to_string();
        let body = to_json(
            service
                .sell_inventory_item(&id, 10.0, None, None)
                .await
                .unwrap()
                .expect("the item exists"),
        );
        assert_eq!(body["ledgerEntry"], Value::Null);
        assert_eq!(body["soldItem"]["name"], json!("Even"));
        assert_eq!(
            service.list_ledger(None, None).await.unwrap().entries.len(),
            0,
            "no noise row"
        );
        assert_eq!(
            service.list_inventory().await.unwrap().len(),
            0,
            "item removed"
        );

        // Sell a missing id -> not found.
        let (_dir, service) = write_service().await;
        assert!(service
            .sell_inventory_item("no-such", 1.0, None, None)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn ledger_delete_removes_then_reports_missing() {
        let (_dir, service) = write_service().await;
        let created = to_json(
            service
                .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
                .await
                .unwrap(),
        );
        let id = created["id"].as_str().unwrap().to_string();
        // A successful delete reports true (the row existed); a second delete
        // reports false (nothing to remove).
        assert!(service.delete_ledger_entry(&id).await.unwrap());
        assert!(!service.delete_ledger_entry(&id).await.unwrap());
    }

    #[tokio::test]
    async fn preset_list_shapes_rows_then_delete_removes() {
        let (_dir, service) = write_service().await;
        let created = to_json(
            service
                .create_ledger_preset("Decay", "expense", "d", 0.5, "decay")
                .await
                .unwrap(),
        );
        let id = created["id"].as_str().unwrap().to_string();
        // The list shapes the row via preset_item (not an empty default).
        let rows = to_json(service.list_ledger_presets().await.unwrap());
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["name"], json!("Decay"));
        assert_eq!(rows[0]["amount"], json!(0.5));
        assert_eq!(rows[0]["tag"], json!("decay"));
        assert!(service.delete_ledger_preset(&id).await.unwrap());
        assert!(!service.delete_ledger_preset(&id).await.unwrap());
    }

    #[tokio::test]
    async fn inventory_delete_removes_then_reports_missing() {
        let (_dir, service) = write_service().await;
        let created = to_json(
            service
                .create_inventory_item("Sword", 10.0, 2.0, None, None)
                .await
                .unwrap(),
        );
        let id = created["id"].as_str().unwrap().to_string();
        assert!(service.delete_inventory_item(&id).await.unwrap());
        assert!(!service.delete_inventory_item(&id).await.unwrap());
    }

    /// The inventory list reads created rows back, newest acquisition first.
    #[tokio::test]
    async fn list_inventory_returns_created_rows_newest_first() {
        let (_dir, service) = write_service().await;
        service
            .create_inventory_item("Old", 1.0, 0.0, None, Some("2026-01-01"))
            .await
            .unwrap();
        service
            .create_inventory_item("New", 2.0, 0.0, None, Some("2026-02-01"))
            .await
            .unwrap();
        let rows = service.list_inventory().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "New");
        assert_eq!(rows[1].name, "Old");
    }

    /// A page whose row count exactly meets the limit ends the walk: the extra
    /// probe row finds nothing, so `has_more` is strictly greater-than, not
    /// greater-or-equal.
    #[tokio::test]
    async fn ledger_page_exactly_at_the_limit_has_no_next_cursor() {
        let (_dir, service) = write_service().await;
        service
            .create_ledger_entry("2026-05-01", "expense", "only", 1.0, "t")
            .await
            .unwrap();
        let page = service.list_ledger(None, Some(1)).await.unwrap();
        assert_eq!(page.entries.len(), 1);
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn hybrid_window_partitions_an_epoch_range_against_the_watermark() {
        let d = |day: &str| daily_rollup::day_range(day).unwrap();
        let (s11, _) = d("2026-03-11");
        let (s12, _) = d("2026-03-12");
        let (s15, _) = d("2026-03-15");
        let (_, e20) = d("2026-03-20");

        // Interior window with partial edge days at both ends: the full days
        // 12..=14 roll, the sub-day edges read raw.
        let a = hybrid_window(Some(s11 + 100.0), Some(s15 + 100.0), "2026-03-20");
        assert_eq!(
            a.rollup_days,
            Some((Some("2026-03-12".to_string()), "2026-03-14".to_string()))
        );
        assert_eq!(
            a.raw_ranges,
            vec![
                (Some(s11 + 100.0), Some(s12)),
                (Some(s15), Some(s15 + 100.0)),
            ]
        );

        // Bounds landing exactly on midnights: no raw edges at all.
        let b = hybrid_window(Some(s12), Some(s15), "2026-03-20");
        assert_eq!(
            b.rollup_days,
            Some((Some("2026-03-12".to_string()), "2026-03-14".to_string()))
        );
        assert_eq!(b.raw_ranges, Vec::<(Option<f64>, Option<f64>)>::new());

        // The all-time window: unbounded below, raw tail from the watermark on.
        let c = hybrid_window(None, None, "2026-03-20");
        assert_eq!(c.rollup_days, Some((None, "2026-03-20".to_string())));
        assert_eq!(c.raw_ranges, vec![(Some(e20), None)]);

        // A sub-day window spanning no full day is served entirely raw.
        let day = hybrid_window(Some(s12 + 100.0), Some(s12 + 200.0), "2026-03-20");
        assert_eq!(day.rollup_days, None);
        assert_eq!(day.raw_ranges, vec![(Some(s12 + 100.0), Some(s12 + 200.0))]);
    }

    #[test]
    fn merge_family_sums_adds_present_slots_and_leaves_the_rest() {
        let mut into: FamilySums = [
            Some(2.0),
            None,
            Some(1.0),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        merge_family_sums(
            &mut into,
            [
                Some(3.0),
                Some(4.0),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        );
        assert_eq!(into[0], Some(5.0)); // present + present
        assert_eq!(into[1], Some(4.0)); // an empty slot takes the incoming value
        assert_eq!(into[2], Some(1.0)); // an incoming None leaves the slot
        assert_eq!(into[3], None);
    }

    fn make_metrics(
        loot: f64,
        gains: &[(&str, f64)],
        cost: f64,
        losses: &[(&str, f64)],
    ) -> Metrics {
        let map = |pairs: &[(&str, f64)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect::<std::collections::BTreeMap<String, f64>>()
        };
        Metrics {
            loot_tt: SqlNumber::Float(loot),
            skill_tt: SqlNumber::Int(0),
            codex_pes: SqlNumber::Int(0),
            quest_pes: SqlNumber::Int(0),
            weapon: SqlNumber::Int(0),
            healing: SqlNumber::Int(0),
            enhancer: SqlNumber::Int(0),
            armour: SqlNumber::Int(0),
            dangling: SqlNumber::Int(0),
            harvest: SqlNumber::Int(0),
            tracking_cost: SqlNumber::Float(cost),
            ledger_gains: map(gains),
            ledger_losses: map(losses),
        }
    }

    #[test]
    fn rate_from_metrics_is_liquid_gains_over_liquid_losses() {
        // (loot 10 + markup 5) / (cost 4 + expense 1) = 3.0.
        let m = make_metrics(10.0, &[("markup", 5.0)], 4.0, &[("expense", 1.0)]);
        assert_eq!(rate_from_metrics(&m), 3.0);
        // Non-positive losses short-circuit to 0.0 (no division).
        let zero = make_metrics(10.0, &[("markup", 5.0)], 0.0, &[]);
        assert_eq!(rate_from_metrics(&zero), 0.0);
    }

    #[test]
    fn slice_rows_zero_cycled_group_yields_zero_rates() {
        // A group whose summed cycled PED is zero rates to 0.0, never a
        // divide-by-zero infinity.
        let agg = SessionAgg {
            duration_hours: 1.0,
            kills: 1,
            loot_tt: 5.0,
            skill_tt: 5.0,
            cycled_ped: 0.0,
            dominant_mob: Some("X".to_string()),
            dominant_mob_kills: 1,
            ..SessionAgg::default()
        };
        let rows =
            build_activity_slice_rows(&[agg], |s| s.dominant_mob.clone(), |s| s.dominant_mob_kills);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pes_per100_ped, 0.0);
        assert_eq!(rows[0].loot_rate, 0.0);
    }

    /// A summary row with zero kills is dropped by the Activity filter even
    /// when its duration and cycled PED are positive.
    #[tokio::test]
    async fn activity_filter_drops_a_zero_kill_summary_row() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            conn.execute(
                "INSERT INTO session_summaries \
                 (session_id, summary_version, started_at, ended_at, duration_hours, kills, \
                  loot_tt, weapon_cost, enhancer_cost, armour_cost, heal_cost, dangling_cost, \
                  cycled_ped, regular_skill_ped_json, attribute_levels_json, regular_skill_tt, \
                  attribute_levels_total, dominant_mob, dominant_tag, dominant_weapon, \
                  dominant_mob_kills, dominant_tag_kills, activity_skill_tt) \
                 VALUES ('ghost', 2, 0, 3600, 1.0, 0, 2.0, 0, 0, 0, 0, 0, 5.0, '{}', '{}', 0, 0, \
                         'Ghost', NULL, NULL, 3, 0, 1.0)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let sessions = db.with_reader(activity_sessions_read).await.unwrap();
        assert!(sessions.is_empty());
    }

    /// The Overview timeline carries per-bucket family totals from BOTH sides
    /// of the hybrid split (a rolled old day and a raw current day), including
    /// the session-cost leg of a rolled day whose heal/dangling sums are NULL.
    /// The period metrics fold the same raw edge in.
    #[tokio::test]
    async fn overview_timeline_carries_rolled_and_raw_family_totals() {
        let now = 1_800_000_000.0;
        let day = 86400.0;
        let (_dir, db) = open_env().await;
        let old = now - 40.0 * day;
        db.with_writer(move |conn| {
            // A rolled old day: a session with armour cost only (heal/dangling
            // NULL, so the rollup keeps them NULL) and a loot kill.
            conn.execute(
                "INSERT INTO tracking_sessions \
                 (id, started_at, ended_at, armour_cost, heal_cost, dangling_cost) \
                 VALUES ('old', ?1, ?2, 3.0, NULL, NULL)",
                rusqlite::params![old, old + 3600.0],
            )?;
            conn.execute(
                "INSERT INTO kills (id, session_id, mob_name, timestamp, enhancer_cost, loot_total_ped) \
                 VALUES ('ok', 'old', 'M', ?1, 0, 20.0)",
                rusqlite::params![old + 10.0],
            )?;
            // A raw kill dated at `now` (after the healed watermark).
            conn.execute(
                "INSERT INTO kills (id, session_id, mob_name, timestamp, enhancer_cost, loot_total_ped) \
                 VALUES ('rk', 'x', 'M', ?1, 0, 7.0)",
                rusqlite::params![now],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let data = overview_impl(&db, now, "all").await.unwrap();
        let loot: f64 = data.timeline.iter().map(|p| p.loot_tt).sum();
        let cost: f64 = data.timeline.iter().map(|p| p.tracking_cost).sum();
        // Rolled loot 20 (old day) + raw loot 7 (today).
        assert_eq!(loot, 27.0);
        // The armour-only rolled session's cost survives the session leg.
        assert_eq!(cost, 3.0);
        // The period metrics fold the raw edge into the rolled sums too.
        assert_eq!(data.returns_breakdown.loot_tt, 27.0);
    }

    /// A timeline point carries its per-tag ledger bucket totals.
    #[tokio::test]
    async fn overview_timeline_carries_ledger_bucket_totals() {
        let now = 1_800_000_000.0;
        let (_dir, db) = open_env().await;
        let today = epoch_to_iso(now);
        let today_c = today.clone();
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                 VALUES ('g', ?1, 'markup', 'sold', 12.5, 'loot_sale')",
                rusqlite::params![today_c],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let data = overview_impl(&db, now, "all").await.unwrap();
        let point = data
            .timeline
            .iter()
            .find(|p| p.bucket == today)
            .expect("the ledgered day has a timeline point");
        assert_eq!(point.ledger_gains.get("loot_sale"), Some(&12.5));
    }

    /// The trend bands are exclusive at their edges: a recent rate exactly at
    /// prior * 1.02 is not "improving", and exactly at prior * 0.98 is not
    /// "declining".
    #[tokio::test]
    async fn overview_trend_bands_are_exclusive_at_the_edges() {
        let now = 1_800_000_000.0;
        let day = 86400.0;

        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 50.0, 1, 51.0).await; // 51/50 = 1.02
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 10.0).await; // 1.0
        assert_eq!(
            overview_impl(&db, now, "all").await.unwrap().trend,
            "stable"
        );

        let (_dir, db) = open_env().await;
        seed_rate(&db, "r", now - 10.0 * day, 50.0, 1, 49.0).await; // 49/50 = 0.98
        seed_rate(&db, "p", now - 45.0 * day, 10.0, 1, 10.0).await; // 1.0
        assert_eq!(
            overview_impl(&db, now, "all").await.unwrap().trend,
            "stable"
        );
    }

    /// Every field the raw reconciliation aggregate computes for one session,
    /// hand-derived from the seed.
    #[tokio::test]
    async fn raw_session_agg_computes_every_field() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            for (kill, mob, species, maturity, loot, enhancer) in [
                ("k1", "Young Atrox", "Atrox", "Young", 2.0, 0.1),
                ("k2", "Young Atrox", "Atrox", "Young", 3.0, 0.0),
                ("k3", "Young Atrox", "Atrox", "Young", 4.0, 0.0),
                ("k4", "Snable", "Snable", "", 1.0, 0.0),
                ("k5", "Unknown", "", "", 0.5, 0.0),
            ] {
                conn.execute(
                    "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
                     timestamp, enhancer_cost, loot_total_ped) \
                     VALUES (?1, 'rs', ?2, ?3, ?4, 1500.0, ?5, ?6)",
                    rusqlite::params![kill, mob, species, maturity, enhancer, loot],
                )?;
            }
            conn.execute(
                "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, cost_per_shot) \
                 VALUES ('k1', 'Rifle', 30, 0.05), ('k2', 'Pistol', 10, 0.01)",
                [],
            )?;
            conn.execute(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
                 VALUES ('rs', 1100.0, 'Rifle', 1.0, 0.5), ('rs', 1200.0, 'Rifle', 1.0, 0.25), \
                        ('rs', 1300.0, 'Agility', 0.25, NULL)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let agg = db
            .with_reader(|conn| raw_session_agg(conn, "rs", 1000.0, 8200.0, 0.07, 0.11, 0.13))
            .await
            .unwrap();
        assert_eq!(agg.duration_hours, 2.0); // (8200 - 1000) / 3600
        assert_eq!(agg.armour_cost, 0.07);
        assert_eq!(agg.heal_cost, 0.11);
        assert_eq!(agg.dangling_cost, 0.13);
        assert_eq!(agg.kills, 5);
        assert_eq!(agg.loot_tt, 10.5);
        assert_eq!(agg.enhancer_cost, 0.1);
        assert_eq!(agg.weapon_cost, 1.6); // 30 @ 0.05 + 10 @ 0.01
        assert_eq!(agg.weapon_shots, 40.0);
        assert_eq!(agg.skill_tt, 0.75); // 0.5 + 0.25 (NULL excluded)
                                        // Atrox 3 of 4 known kills = 0.75, species present -> dominant mob.
        assert_eq!(agg.dominant_mob, Some("Young Atrox".to_string()));
        assert_eq!(agg.dominant_mob_kills, 3);
        assert_eq!(agg.dominant_tag, None);
        // weapon 1.6 + enhancer 0.1 + armour 0.07 + heal 0.11 + dangling 0.13.
        assert_eq!(agg.cycled_ped, 2.01);
    }

    /// Bare mob names (no species or maturity) classify as a tag, not a mob.
    #[tokio::test]
    async fn raw_session_agg_classifies_bare_names_as_tags() {
        let (_dir, db) = open_env().await;
        db.with_writer(|conn| {
            for i in 0..3 {
                conn.execute(
                    "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
                     timestamp, enhancer_cost, loot_total_ped) \
                     VALUES (?1, 'tg', 'Thing', NULL, NULL, ?2, 0, 1.0)",
                    rusqlite::params![format!("tg-{i}"), 1000.0 + i as f64],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
        let agg = db
            .with_reader(|conn| raw_session_agg(conn, "tg", 1000.0, 4600.0, 0.0, 0.0, 0.0))
            .await
            .unwrap();
        assert_eq!(agg.dominant_tag, Some("Thing".to_string()));
        assert_eq!(agg.dominant_mob, None);
    }

    /// The trend compares the recent-30d window (lower bound `now - 30*86400`)
    /// against the prior-30d window (bounds `now - 60*86400` .. `now -
    /// 30*86400`). A high recent rate over a low prior rate is "improving".
    /// The three seeded sessions pin BOTH window bounds: an ancient session
    /// older than 60 days sits outside the prior window, so mutating either
    /// bound's `-` to a `/` (which collapses the bound to near-epoch) folds
    /// extra sessions in and flips the verdict. Mutating the 30-day bound
    /// empties the prior window ("stable"); mutating the 60-day bound drags
    /// the prior rate up with the ancient session ("declining").
    #[tokio::test]
    async fn overview_trend_reads_the_thirty_and_sixty_day_window_bounds() {
        let now = 1_800_000_000.0;
        let day = 86400.0;
        let (_dir, db) = open_env().await;
        seed_rate(&db, "recent", now - 10.0 * day, 10.0, 1, 20.0).await; // rate 2.0
        seed_rate(&db, "prior", now - 45.0 * day, 10.0, 1, 10.0).await; // rate 1.0
        seed_rate(&db, "ancient", now - 90.0 * day, 10.0, 1, 100.0).await; // rate 10.0
        assert_eq!(
            overview_impl(&db, now, "all").await.unwrap().trend,
            "improving"
        );
    }

    /// A rolled old day whose session carries heal cost only (armour/dangling
    /// NULL, so the rollup keeps them NULL). The session-cost leg must survive:
    /// the family guard is a disjunction, and mutating its second `||` to `&&`
    /// (which binds tighter, parsing as `armour || (heal && dangling)`) would
    /// drop a heal-only day's cost.
    #[tokio::test]
    async fn overview_timeline_carries_a_heal_only_rolled_session_cost() {
        let now = 1_800_000_000.0;
        let day = 86400.0;
        let (_dir, db) = open_env().await;
        let old = now - 40.0 * day;
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions \
                 (id, started_at, ended_at, armour_cost, heal_cost, dangling_cost) \
                 VALUES ('old', ?1, ?2, NULL, 4.0, NULL)",
                rusqlite::params![old, old + 3600.0],
            )?;
            conn.execute(
                "INSERT INTO kills (id, session_id, mob_name, timestamp, enhancer_cost, loot_total_ped) \
                 VALUES ('ok', 'old', 'M', ?1, 0, 20.0)",
                rusqlite::params![old + 10.0],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let data = overview_impl(&db, now, "all").await.unwrap();
        let cost: f64 = data.timeline.iter().map(|p| p.tracking_cost).sum();
        assert_eq!(cost, 4.0);
    }
}
