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
//! The aggregates preserve the engine's numeric typing on the wire: an
//! empty `COALESCE(SUM(...), 0)` stays the integer `0` (`sql_number` reads
//! the engine type, `rounded` applies type-preserving rounding, and
//! `float_field` coerces to float only where the response model declares
//! one). The typed facade re-coerces these to its `f64` DTO fields at the
//! boundary; the demo surface renders the value verbatim.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{json, Map, Value};
#[cfg(test)]
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::clock::Clock;
use crate::daily_rollup;
use crate::db::{Db, DbError};
use crate::time::naive_to_epoch;

/// The analytics domain service over the shared database and injected
/// clock: the Overview / Activity aggregates and the ledger / preset /
/// inventory CRUD, ported from the router-resident handlers that hosted
/// them before the typed-command migration.
pub struct AnalyticsService {
    db: Db,
    clock: Arc<dyn Clock>,
}

/// A page of ledger entries (newest first) plus the opaque cursor for the
/// following page (`None` on the last page). The entries carry their wire
/// shape as `Value` (the caller projects them to its own type); the cursor
/// is the base64url keyset token.
pub struct LedgerPage {
    pub entries: Vec<Value>,
    pub next_cursor: Option<String>,
}

/// The analytics service's error surface. The two validation variants (a
/// malformed ledger cursor, an out-of-vocabulary preset type) were the
/// router's 400s and carry its verbatim detail; `Db` / `Storage` are the
/// driver and rollup-refresh failures (the router's 500s). The transports
/// map these onto their own error contracts.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("type must be 'expense' or 'markup'")]
    InvalidPresetType,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
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
//    eo-services::quests; kept local so this router stays self-contained,
//    matching the per-file formatter convention in hydration/character) ──

/// A SQLite numeric read preserving the engine type: a REAL decodes to a
/// float, an INTEGER (including the `COALESCE(SUM(...), 0)` empty case) to an
/// integer. The stored value's affinity (`ValueRef`) drives the branch, so a
/// REAL sum stays a float and an integer sum (the NULL-sum zeros) stays an
/// integer, exactly as the sqlx `try_get::<f64>`-then-`i64` cascade did.
fn sql_number(row: &rusqlite::Row, index: usize) -> Value {
    match row.get_ref_unwrap(index) {
        rusqlite::types::ValueRef::Real(value) => json!(value),
        value => json!(value.as_i64().expect("sql_number reads a numeric column")),
    }
}

/// The sum of two engine-typed numbers, integer when both are (Python's `+`).
fn number_sum(a: &Value, b: &Value) -> Value {
    match (a.as_i64(), b.as_i64()) {
        (Some(left), Some(right)) => json!(left + right),
        _ => json!(a.as_f64().unwrap_or(0.0) + b.as_f64().unwrap_or(0.0)),
    }
}

/// `round(value, places)`: banker's rounding on a float, an integer left as
/// an integer (Python keeps `round(int, n)` an int).
fn rounded(value: &Value, places: usize) -> Value {
    match value.as_f64() {
        Some(number) if value.is_f64() => {
            json!(eo_wire::normalizer::round_half_even(number, places))
        }
        _ => value.clone(),
    }
}

/// A model-declared `float` field: coerce an engine-typed integer to its
/// float form, so an integer zero leaves the wire as `0.0`.
fn float_field(value: Value) -> Value {
    match value.as_i64() {
        Some(integer) => json!(integer as f64),
        None => value,
    }
}

/// `float(value)` over an engine-typed number (the activity path, where every
/// numeric is summed in float space).
fn as_float(row: &rusqlite::Row, index: usize) -> f64 {
    sql_number(row, index).as_f64().unwrap_or(0.0)
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

/// The nine aggregate-family sums of one window part, position-matched
/// to the `daily_rollups` family columns (loot, weapon, enhancer,
/// armour, heal, dangling, skill, codex, quest). A sum stays None when
/// the part had no contributing rows, so the merged result reproduces
/// the raw engine typing: an all-empty window leaves the wire as an
/// integer zero, exactly as `COALESCE(SUM(...), 0)` does.
type FamilySums = [Option<f64>; 9];

fn merge_family_sums(into: &mut FamilySums, from: FamilySums) {
    for (slot, value) in into.iter_mut().zip(from) {
        if let Some(value) = value {
            *slot = Some(slot.unwrap_or(0.0) + value);
        }
    }
}

fn family_value(sum: Option<f64>) -> Value {
    sum.map_or(json!(0), |value| json!(value))
}

/// The `daily_rollups` family-sum columns, position-matched to
/// [`FamilySums`] (loot, weapon, enhancer, armour, heal, dangling, skill,
/// codex, quest).
const ROLLUP_FAMILY_COLS: [&str; 9] = [
    "loot_tt",
    "weapon_cost",
    "enhancer_cost",
    "armour_cost",
    "heal_cost",
    "dangling_cost",
    "skill_tt",
    "codex_pes",
    "quest_pes",
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
    let mut out: Vec<FamilySums> = vec![[None; 9]; windows.len()];

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
    let mut cols: Vec<String> = Vec::with_capacity(active.len() * 9);
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
        let mut per_slot: Vec<FamilySums> = vec![[None; 9]; active.len()];
        for (slot, sums) in per_slot.iter_mut().enumerate() {
            let base = slot * 9;
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
    let mut sums: FamilySums = [None; 9];
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
    Ok(sums)
}

/// A day/month-keyed aggregate (`SELECT <bucket>, COALESCE(SUM(...), 0) ...
/// GROUP BY <bucket>`) collected as `bucket -> engine-typed number`,
/// preserving the SQL row order.
fn bucketed_epoch(
    conn: &rusqlite::Connection,
    sql: String,
    params: &[f64],
) -> Result<Map<String, Value>, DbError> {
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(params))?;
    let mut out = Map::new();
    while let Some(row) = rows.next()? {
        out.insert(row.get::<_, String>(0)?, sql_number(row, 1));
    }
    Ok(out)
}

// ── _compute_metrics ──

/// The gains/losses breakdown for one window (`_compute_metrics`).
struct Metrics {
    loot_tt: Value,
    skill_tt: Value,
    codex_pes: Value,
    quest_pes: Value,
    weapon: Value,
    healing: Value,
    enhancer: Value,
    armour: Value,
    dangling: Value,
    tracking_cost: Value,
    ledger_gains: Map<String, Value>,
    ledger_losses: Map<String, Value>,
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
) -> Result<Map<String, Value>, DbError> {
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

    let mut out = Map::new();
    for (tag, total) in totals {
        out.insert(tag, rounded(&json!(total), 2));
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

    let loot_tt = family_value(sums[0]);
    let weapon = family_value(sums[1]);
    let enhancer = family_value(sums[2]);
    let armour = family_value(sums[3]);
    let healing = family_value(sums[4]);
    let dangling = family_value(sums[5]);
    let skill_tt = family_value(sums[6]);
    let codex_pes = family_value(sums[7]);
    let quest_pes = family_value(sums[8]);

    // weapon + heal + enhancer + armour + dangling (the reference's order).
    let tracking_cost = number_sum(
        &number_sum(
            &number_sum(&number_sum(&weapon, &healing), &enhancer),
            &armour,
        ),
        &dangling,
    );

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
        tracking_cost,
        ledger_gains,
        ledger_losses,
    })
}

/// Sum of a ledger map's values in float space.
fn sum_values(map: &Map<String, Value>) -> f64 {
    map.values().filter_map(Value::as_f64).sum()
}

/// `_rate_from_metrics`: liquid gains over liquid losses (progression
/// excluded), 0.0 when losses are non-positive.
fn rate_from_metrics(m: &Metrics) -> f64 {
    let total_gains = m.loot_tt.as_f64().unwrap_or(0.0) + sum_values(&m.ledger_gains);
    let total_losses = m.tracking_cost.as_f64().unwrap_or(0.0) + sum_values(&m.ledger_losses);
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
) -> Result<std::collections::BTreeMap<String, Map<String, Value>>, DbError> {
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

    let mut out: std::collections::BTreeMap<String, Map<String, Value>> =
        std::collections::BTreeMap::new();
    for (bucket, tags) in sums {
        let entry = out.entry(bucket).or_default();
        for (tag, amount) in tags {
            entry.insert(tag, rounded(&json!(amount), 2));
        }
    }
    Ok(out)
}

// ── overview_impl ──

/// The Overview aggregate. Scaling is O(days), not O(rows): the heal
/// brings the daily rollups current (steady-state, a single metadata
/// read), and every window then aggregates rollup rows plus bounded raw
/// edges (see [`hybrid_window`]).
async fn overview_impl(db: &Db, now: f64, period: &str) -> Result<Value, DbError> {
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
) -> Result<Value, DbError> {
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
    let total_gains = m.loot_tt.as_f64().unwrap_or(0.0) + total_ledger_gains;
    let total_losses = m.tracking_cost.as_f64().unwrap_or(0.0) + total_ledger_losses;
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

    // Daily breakdown (the point key is "date", the monthly point's is "month").
    let timeline = breakdown_points(conn, watermark, epoch_start, "date", BucketKind::Day)?;
    // Monthly breakdown.
    let monthly = breakdown_points(conn, watermark, epoch_start, "month", BucketKind::Month)?;

    let cycled_breakdown = json!({
        "weapon": rounded(&m.weapon, 2),
        "healing": rounded(&m.healing, 2),
        "enhancer": rounded(&m.enhancer, 2),
        "armour": rounded(&m.armour, 2),
        "dangling": rounded(&m.dangling, 2),
    });

    Ok(json!({
        "totalReturnRate": json!(eo_wire::normalizer::round_half_even(return_rate, 4)),
        "trend": trend,
        "returnsBreakdown": {
            "lootTt": float_field(rounded(&m.loot_tt, 2)),
            "pes": float_field(rounded(&m.skill_tt, 2)),
            "codexPes": float_field(rounded(&m.codex_pes, 2)),
            "questPes": float_field(rounded(&m.quest_pes, 2)),
            "ledger": coerce_ledger(&m.ledger_gains),
        },
        "lossesBreakdown": {
            "trackingCost": float_field(rounded(&m.tracking_cost, 2)),
            "cycledBreakdown": cycled_breakdown,
            "ledger": coerce_ledger(&m.ledger_losses),
        },
        "totalGains": json!(eo_wire::normalizer::round_half_even(total_gains, 2)),
        "totalLosses": json!(eo_wire::normalizer::round_half_even(total_losses, 2)),
        "timeline": timeline,
        "monthlyBreakdown": monthly,
    }))
}

/// A model `dict[str, float]` ledger map: coerce each value to its float form.
fn coerce_ledger(map: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    for (tag, amount) in map {
        out.insert(tag.clone(), float_field(amount.clone()));
    }
    Value::Object(out)
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
    loot: Map<String, Value>,
    weapon: Map<String, Value>,
    enhancer: Map<String, Value>,
    sess: Map<String, Value>,
    skill: Map<String, Value>,
    codex: Map<String, Value>,
    quest: Map<String, Value>,
    members: BTreeSet<String>,
}

impl BreakdownMaps {
    /// Merge one bucket's value into a family map. Day buckets never
    /// collide across parts (the hybrid ranges partition the timeline);
    /// month buckets can span the split and sum engine-typed.
    fn merge(map: &mut Map<String, Value>, bucket: &str, value: Value) {
        match map.get(bucket) {
            Some(existing) => {
                let total = number_sum(existing, &value);
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
             armour_cost, heal_cost, dangling_cost, skill_tt, codex_pes, quest_pes \
             FROM daily_rollups WHERE day <= ?{extra} ORDER BY bucket"
        ),
        BucketKind::Month => format!(
            "SELECT strftime('%Y-%m', day) AS bucket, MAX(has_rows), SUM(loot_tt), \
             SUM(weapon_cost), SUM(enhancer_cost), SUM(armour_cost), SUM(heal_cost), \
             SUM(dangling_cost), SUM(skill_tt), SUM(codex_pes), SUM(quest_pes) \
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
        ] {
            if let Some(value) = family(index)? {
                BreakdownMaps::merge(map, &bucket, json!(value));
            }
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
            let leg = |sum: Option<f64>| sum.map_or(json!(0), |value| json!(value));
            let total = number_sum(&number_sum(&leg(armour), &leg(heal)), &leg(dangling));
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

    let sources: [(&mut Map<String, Value>, String, &Vec<f64>); 7] = [
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
    ];
    for (map, sql, params) in sources {
        let buckets = bucketed_epoch(conn, sql, params)?;
        for (bucket, value) in buckets {
            maps.members.insert(bucket.clone());
            BreakdownMaps::merge(map, &bucket, value);
        }
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
    bucket_label: &str,
    kind: BucketKind,
) -> Result<Value, DbError> {
    let window = hybrid_window(epoch_start, None, watermark);
    let mut maps = BreakdownMaps::default();
    if let Some((lo, hi)) = &window.rollup_days {
        rollup_breakdown(conn, &mut maps, kind, lo.as_deref(), hi)?;
    }
    for range in &window.raw_ranges {
        raw_breakdown(conn, &mut maps, kind, *range)?;
    }

    // cost = weapon + enhancer + sess over the union of their buckets.
    let mut cost: Map<String, Value> = Map::new();
    let mut cost_keys: BTreeSet<String> = BTreeSet::new();
    for k in maps
        .weapon
        .keys()
        .chain(maps.enhancer.keys())
        .chain(maps.sess.keys())
    {
        cost_keys.insert(k.clone());
    }
    for key in &cost_keys {
        let zero = json!(0);
        let total = number_sum(
            &number_sum(
                maps.weapon.get(key).unwrap_or(&zero),
                maps.enhancer.get(key).unwrap_or(&zero),
            ),
            maps.sess.get(key).unwrap_or(&zero),
        );
        cost.insert(key.clone(), total);
    }

    let gains = ledger_buckets(conn, kind, "markup", epoch_start, watermark)?;
    let losses = ledger_buckets(conn, kind, "expense", epoch_start, watermark)?;

    // all buckets, sorted (lexicographic == chronological for these forms).
    let mut all: BTreeSet<String> = maps.members;
    for k in gains.keys().chain(losses.keys()) {
        all.insert(k.clone());
    }

    let zero = json!(0);
    let mut points = Vec::new();
    for bucket in &all {
        points.push(json!({
            bucket_label: bucket,
            "lootTt": float_field(rounded(maps.loot.get(bucket).unwrap_or(&zero), 4)),
            "pes": float_field(rounded(maps.skill.get(bucket).unwrap_or(&zero), 4)),
            "codexPes": float_field(rounded(maps.codex.get(bucket).unwrap_or(&zero), 4)),
            "questPes": float_field(rounded(maps.quest.get(bucket).unwrap_or(&zero), 4)),
            "ledgerGains": gains.get(bucket).cloned().map(Value::Object).unwrap_or_else(|| json!({})),
            "trackingCost": float_field(rounded(cost.get(bucket).unwrap_or(&zero), 4)),
            "ledgerLosses": losses.get(bucket).cloned().map(Value::Object).unwrap_or_else(|| json!({})),
        }));
    }
    Ok(Value::Array(points))
}

// ── activity_impl ──

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
    dominant_weapon: Option<String>,
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
         dominant_mob, dominant_tag, dominant_weapon, dominant_mob_kills, dominant_tag_kills \
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
                dominant_weapon: row.get::<_, Option<String>>(8)?,
                dominant_mob_kills: row.get::<_, i64>(9).unwrap_or(0),
                dominant_tag_kills: row.get::<_, i64>(10).unwrap_or(0),
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

    let tool_rows: Vec<(String, f64)> = {
        let mut stmt = conn.prepare(
            "SELECT ts.tool_name, COALESCE(SUM(ts.shots_fired), 0) \
             FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id \
             WHERE k.session_id = ? AND ts.tool_name IS NOT NULL AND ts.tool_name != 'Unknown' \
             GROUP BY ts.tool_name ORDER BY SUM(ts.shots_fired) DESC, ts.tool_name ASC",
        )?;
        let mapped = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok((row.get::<_, String>(0)?, as_float(row, 1)))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if !tool_rows.is_empty() {
        let total_shots: f64 = tool_rows.iter().map(|r| r.1).sum();
        let (top_name, top_shots) = tool_rows[0].clone();
        if total_shots > 0.0 && top_shots / total_shots >= ACTIVITY_DOMINANCE_THRESHOLD {
            agg.dominant_weapon = Some(top_name);
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
    name_field: &str,
) -> Vec<Value> {
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

    let mut rows: Vec<(i64, f64, String, Value)> = Vec::new();
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
        let row = json!({
            name_field: value,
            "sessions": sessions_count,
            "kills": kills,
            "hours": hours_r,
            "cycled": cycled_r,
            "pesPer100Ped": pes_per_100,
            "lootRate": loot_rate,
        });
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

async fn activity_impl(db: &Db) -> Result<Value, DbError> {
    let sessions = load_activity_sessions(db).await?;
    let mob = build_activity_slice_rows(
        &sessions,
        |s| s.dominant_mob.clone(),
        |s| s.dominant_mob_kills,
        "mobName",
    );
    let tag = build_activity_slice_rows(
        &sessions,
        |s| s.dominant_tag.clone(),
        |s| s.dominant_tag_kills,
        "tagName",
    );
    // Weapon comparisons inline the helper but key kills off the session
    // total (not a dominant-weapon kill count).
    let weapon = build_activity_slice_rows(
        &sessions,
        |s| s.dominant_weapon.clone(),
        |s| s.kills,
        "weaponName",
    );
    Ok(json!({
        "mobComparisons": mob,
        "tagComparisons": tag,
        "weaponComparisons": weapon,
    }))
}

// ── The Overview and Activity aggregates ──

impl AnalyticsService {
    /// The Overview aggregate for a named period (`30d` / `90d` / `1y`, or
    /// all-time for any other value).
    ///
    /// Scales O(days), not O(kills): the aggregates read the daily rollup
    /// projection for completed days and touch the raw tables only for the
    /// partial edge days (see [`overview_impl`]).
    pub async fn overview(&self, period: &str) -> Result<Value, AnalyticsError> {
        let now = naive_to_epoch(self.clock.now());
        Ok(overview_impl(&self.db, now, period).await?)
    }

    /// The Activity aggregate: the per-mob / per-tag / per-weapon
    /// comparison tables over the completed sessions.
    pub async fn activity(&self) -> Result<Value, AnalyticsError> {
        Ok(activity_impl(&self.db).await?)
    }
}

// ── Ledger / presets / inventory writes (the CRUD surface) ──

const INVENTORY_SALE_TAG: &str = "inventory_sale";

/// `LedgerItem` / `LedgerPresetItem` share a shape; both select
/// (id, name-or-date, type, description, amount, tag).
fn ledger_item(row: &rusqlite::Row) -> Value {
    json!({
        "id": row.get_unwrap::<_, String>(0),
        "date": row.get_unwrap::<_, String>(1),
        "type": row.get_unwrap::<_, String>(2),
        "description": row.get_unwrap::<_, String>(3),
        "amount": float_field(sql_number(row, 4)),
        "tag": row.get_unwrap::<_, String>(5),
    })
}

fn preset_item(row: &rusqlite::Row) -> Value {
    json!({
        "id": row.get_unwrap::<_, String>(0),
        "name": row.get_unwrap::<_, String>(1),
        "type": row.get_unwrap::<_, String>(2),
        "description": row.get_unwrap::<_, String>(3),
        "amount": float_field(sql_number(row, 4)),
        "tag": row.get_unwrap::<_, String>(5),
    })
}

/// The default ledger page size when the client names no `limit`.
const LEDGER_PAGE_DEFAULT: i64 = 50;
/// The largest ledger page a client may request; larger `limit` values clamp
/// here, bounding the work a single request can ask for.
const LEDGER_PAGE_MAX: i64 = 200;

/// The opaque keyset cursor: base64url (no padding) of the JSON `[date, id]`
/// of the last row on a page. Opaque so clients treat it as a token, and
/// robust to any characters a user-entered ledger date or a UUID id carries.
fn encode_ledger_cursor(date: &str, id: &str) -> String {
    use base64::Engine as _;
    let json = serde_json::to_vec(&[date, id]).expect("a cursor pair serialises");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

/// Decode a keyset cursor back to its `(date, id)` seek key, or `None` for a
/// malformed token (which the handler answers as a 400).
fn decode_ledger_cursor(token: &str) -> Option<(String, String)> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .ok()?;
    let [date, id]: [String; 2] = serde_json::from_slice(&bytes).ok()?;
    Some((date, id))
}

/// `_inventory_row_to_dict`: (id, name, tt_value, markup_paid, notes, acquired_at).
fn inventory_item(row: &rusqlite::Row) -> Value {
    json!({
        "id": row.get_unwrap::<_, String>(0),
        "name": row.get_unwrap::<_, String>(1),
        "ttValue": float_field(sql_number(row, 2)),
        "markupPaid": float_field(sql_number(row, 3)),
        "notes": row.get_unwrap::<_, Option<String>>(4),
        "acquiredAt": row.get_unwrap::<_, String>(5),
    })
}

impl AnalyticsService {
    /// `_utc_date_str(clock)`: the clock's instant as a UTC YYYY-MM-DD date.
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

        // Each fetched row as (date, id, wire shape); the cursor is cut from
        // the last kept row's (date, id).
        let rows: Vec<(String, String, Value)> = self
            .db
            .with_reader(move |conn| {
                let mut stmt = conn.prepare(&sql)?;
                let map_row = |row: &rusqlite::Row| -> rusqlite::Result<(String, String, Value)> {
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
                Ok(rows)
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
        let entries: Vec<Value> = kept.iter().map(|(_, _, item)| item.clone()).collect();
        let next_cursor = has_more
            .then(|| kept.last())
            .flatten()
            .map(|(date, id, _)| encode_ledger_cursor(date, id));
        Ok(LedgerPage {
            entries,
            next_cursor,
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
    ) -> Result<Value, AnalyticsError> {
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
        Ok(json!({
            "id": id, "date": date, "type": kind,
            "description": description, "amount": amount, "tag": tag,
        }))
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
    pub async fn list_ledger_presets(&self) -> Result<Vec<Value>, AnalyticsError> {
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
    ) -> Result<Value, AnalyticsError> {
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
        Ok(json!({
            "id": id, "name": name, "type": kind,
            "description": description, "amount": amount, "tag": tag,
        }))
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
    pub async fn list_inventory(&self) -> Result<Vec<Value>, AnalyticsError> {
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

    /// The stored inventory row re-read and shaped (the create / patch
    /// reply). A row that has vanished since the write is a driver-level
    /// invariant break, surfaced as [`AnalyticsError::Storage`].
    async fn inventory_row(&self, item_id: &str) -> Result<Value, AnalyticsError> {
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
    ) -> Result<Value, AnalyticsError> {
        let id = Uuid::new_v4().to_string();
        // `item.acquired_at or _utc_date_str(clock)`: the reference's `or`
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
    ) -> Result<Option<Value>, AnalyticsError> {
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
    ) -> Result<Option<Value>, AnalyticsError> {
        // The item is read on a reader-core connection; the realised sale then
        // writes its ledger row and removes the item in one sqlx transaction
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
                                sql_number(row, 2).as_f64().unwrap_or(0.0),
                                sql_number(row, 3).as_f64().unwrap_or(0.0),
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
        // `payload.sold_at or _utc_date_str(clock)`: empty string is falsy.
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
        let ledger_entry = match ledger_write {
            Some((entry_id, sold_at, entry_type, description, amount)) => json!({
                "id": entry_id, "date": sold_at, "type": entry_type,
                "description": description, "amount": amount, "tag": INVENTORY_SALE_TAG,
            }),
            None => Value::Null,
        };
        Ok(Some(
            json!({"ledgerEntry": ledger_entry, "soldItem": sold_item}),
        ))
    }
}

#[cfg(test)]
impl AnalyticsService {
    /// The reader pool, for tests that inspect projection state directly.
    fn read(&self) -> &SqlitePool {
        self.db.read()
    }

    /// The writer pool, for the sqlx-side seeding the write-handler tests do.
    fn write(&self) -> &SqlitePool {
        self.db.write()
    }

    /// The database handle, for tests that drive the synchronous core
    /// (the rollup heal) directly.
    fn db(&self) -> &Db {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eo_wire::normalizer::to_wire_json;

    /// A real database (writer/reader pools plus the synchronous core) over a
    /// temp file, with its writer pool handed back for the sqlx-side seeding
    /// the tests still do. A temp file (not `:memory:`) is required: the
    /// synchronous core opens its own connections, which an in-memory pool
    /// cannot share. The reads under test run on the core; the seeds commit on
    /// the writer pool and are visible to the core's readers under WAL.
    async fn open_env() -> (tempfile::TempDir, Db, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("entropia_orme.db"))
            .await
            .unwrap();
        let pool = db.write().clone();
        (dir, db, pool)
    }

    /// An [`AnalyticsService`] over a real temp-file database, its clock frozen
    /// so `default_date()` (`_utc_date_str(clock)`) is deterministic
    /// (2026-06-01).
    async fn write_service() -> (tempfile::TempDir, AnalyticsService) {
        use crate::clock::MockClock;
        let (dir, db, _pool) = open_env().await;
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

    async fn ledger_rollup(pool: &SqlitePool, day: &str, tag: &str) -> Option<(String, f64)> {
        sqlx::query_as(
            "SELECT entry_type, amount FROM daily_ledger_rollups WHERE day = ? AND tag = ?",
        )
        .bind(day)
        .bind(tag)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn ledger_create_and_delete_reland_their_days_rollups() {
        let (_dir, service) = write_service().await;
        heal_to_june_fifth(service.db()).await;

        // A backdated create lands its day's rollup with the insert.
        let body = service
            .create_ledger_entry("2026-06-02", "expense", "ammo restock", 12.5, "manual")
            .await
            .unwrap();
        assert_eq!(
            ledger_rollup(service.read(), "2026-06-02", "manual").await,
            Some(("expense".into(), 12.5))
        );

        // The delete relands it empty; a missing id reports not-found.
        let id = body["id"].as_str().unwrap().to_string();
        assert!(service.delete_ledger_entry(&id).await.unwrap());
        assert_eq!(
            ledger_rollup(service.read(), "2026-06-02", "manual").await,
            None
        );
        assert!(!service.delete_ledger_entry("missing").await.unwrap());
    }

    #[tokio::test]
    async fn inventory_sale_relands_the_sold_days_rollup() {
        let (_dir, service) = write_service().await;
        heal_to_june_fifth(service.db()).await;
        sqlx::query(
            "INSERT INTO inventory_items (id, name, tt_value, markup_paid, notes, acquired_at) \
             VALUES ('i1', 'Gun', 10.0, 2.0, NULL, '2026-05-01')",
        )
        .execute(service.write())
        .await
        .unwrap();

        // Sold at a backdated date for an 8.0 markup delta.
        service
            .sell_inventory_item("i1", 20.0, None, Some("2026-06-02"))
            .await
            .unwrap()
            .expect("the item exists");
        assert_eq!(
            ledger_rollup(service.read(), "2026-06-02", INVENTORY_SALE_TAG).await,
            Some(("markup".into(), 8.0))
        );
    }

    #[tokio::test]
    async fn empty_overview_emits_the_engine_typed_zeros() {
        let (_dir, db, _pool) = open_env().await;
        let value = overview_impl(&db, 1_800_000_000.0, "all").await.unwrap();
        // cycledBreakdown is an `Any` field: empty COALESCE sums leave the
        // integer zero on the wire, while the float-declared aggregates coerce.
        assert_eq!(
            to_wire_json(&value),
            "{\"totalReturnRate\":0.0,\"trend\":\"stable\",\"returnsBreakdown\":{\"lootTt\":0.0,\
             \"pes\":0.0,\"codexPes\":0.0,\"questPes\":0.0,\"ledger\":{}},\"lossesBreakdown\":\
             {\"trackingCost\":0.0,\"cycledBreakdown\":{\"weapon\":0,\"healing\":0,\"enhancer\":0,\
             \"armour\":0,\"dangling\":0},\"ledger\":{}},\"totalGains\":0.0,\"totalLosses\":0.0,\
             \"timeline\":[],\"monthlyBreakdown\":[]}"
        );
    }

    #[tokio::test]
    async fn empty_activity_emits_three_empty_tables() {
        let (_dir, db, _pool) = open_env().await;
        let value = activity_impl(&db).await.unwrap();
        assert_eq!(
            to_wire_json(&value),
            "{\"mobComparisons\":[],\"tagComparisons\":[],\"weaponComparisons\":[]}"
        );
    }

    /// Seed the representative scenario the live probe grounded, with the
    /// window relative to a fixed `now`, and assert the computed aggregates,
    /// the trend, dominance, and the filters.
    async fn seed_scenario(pool: &SqlitePool, now: f64) {
        let day = 86400.0;
        let recent = now - 11.0 * day; // inside the 30d window
        let prior = now - 37.0 * day; // inside the 30-60d window
                                      // sessions
        for (id, start, armour, heal, dangling) in [
            ("sess-a", recent, 1.0, 2.0, 0.5),
            ("sess-b", prior, 0.5, 1.0, 0.0),
            ("sess-z", recent, 0.0, 0.0, 0.0), // zero-kill, zero-cost: filtered from activity
        ] {
            sqlx::query(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
                 VALUES(?,?,?,?,?,?)",
            )
            .bind(id)
            .bind(start)
            .bind(start + 3600.0)
            .bind(armour)
            .bind(heal)
            .bind(dangling)
            .execute(pool)
            .await
            .expect("seed");
        }
        for i in 0..5 {
            let kid = format!("k-a-{i}");
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(&kid).bind("sess-a").bind("Atrox").bind("Atrox").bind("Young")
            .bind(recent + i as f64).bind(0.1).bind(10.0)
            .execute(pool).await.expect("seed");
            sqlx::query("INSERT INTO kill_tool_stats(kill_id,tool_name,shots_fired,cost_per_shot) VALUES(?,?,?,?)")
                .bind(&kid).bind("Opalo").bind(50_i64).bind(0.011)
                .execute(pool).await.expect("seed");
        }
        for i in 0..3 {
            let kid = format!("k-b-{i}");
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,?,?,NULL,NULL,?,?,?)",
            )
            .bind(&kid).bind("sess-b").bind("Thing")
            .bind(prior + i as f64).bind(0.0).bind(5.0)
            .execute(pool).await.expect("seed");
            sqlx::query("INSERT INTO kill_tool_stats(kill_id,tool_name,shots_fired,cost_per_shot) VALUES(?,?,?,?)")
                .bind(&kid).bind("Opalo").bind(30_i64).bind(0.01)
                .execute(pool).await.expect("seed");
        }
        sqlx::query(
            "INSERT INTO skill_gains(session_id,timestamp,skill_name,amount,ped_value) \
             VALUES(?,?,?,?,?)",
        )
        .bind("sess-a")
        .bind(recent)
        .bind("Laser Weaponry Technology")
        .bind(1.0)
        .bind(3.0)
        .execute(pool)
        .await
        .expect("seed");
        sqlx::query(
            "INSERT INTO skill_gains(session_id,timestamp,skill_name,amount,ped_value) \
             VALUES(?,?,?,?,?)",
        )
        .bind("sess-b")
        .bind(prior)
        .bind("Laser Weaponry Technology")
        .bind(1.0)
        .bind(1.0)
        .execute(pool)
        .await
        .expect("seed");
        sqlx::query(
            "INSERT INTO codex_claims(species_name,rank,skill_name,claimed_at,ped_value) \
             VALUES(?,?,?,?,?)",
        )
        .bind("Atrox")
        .bind(1_i64)
        .bind("Rifle")
        .bind(recent)
        .bind(7.0)
        .execute(pool)
        .await
        .expect("seed");
        sqlx::query("INSERT INTO quest_claims(quest_name,claimed_at,ped_value) VALUES(?,?,?)")
            .bind("A Quest")
            .bind(recent)
            .bind(4.0)
            .execute(pool)
            .await
            .expect("seed");
        // ledger: a recent markup and a prior expense, dated by the ISO form.
        sqlx::query(
            "INSERT INTO ledger_entries(id,date,type,description,amount,tag) VALUES(?,?,?,?,?,?)",
        )
        .bind("led-1")
        .bind(epoch_to_iso(recent))
        .bind("markup")
        .bind("Sold hides")
        .bind(12.5)
        .bind("loot_sale")
        .execute(pool)
        .await
        .expect("seed");
        sqlx::query(
            "INSERT INTO ledger_entries(id,date,type,description,amount,tag) VALUES(?,?,?,?,?,?)",
        )
        .bind("led-2")
        .bind(epoch_to_iso(prior))
        .bind("expense")
        .bind("Deposit")
        .bind(8.0)
        .bind("deposit")
        .execute(pool)
        .await
        .expect("seed");
    }

    #[tokio::test]
    async fn seeded_overview_aggregates_match() {
        let now = 1_800_000_000.0;
        let (_dir, db, pool) = open_env().await;
        seed_scenario(&pool, now).await;
        let v = overview_impl(&db, now, "all").await.unwrap();
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
        // timeline points key the day as "date"; monthly points as "month".
        assert!(v["timeline"][0].get("date").is_some());
        assert!(v["monthlyBreakdown"][0].get("month").is_some());
        // trend: recent-30d rate exceeds prior-30d rate beyond the 2% band.
        assert_eq!(v["trend"], json!("improving"));
        // period filter: 30d keeps only the recent window (markup in, expense out).
        let v30 = overview_impl(&db, now, "30d").await.unwrap();
        assert_eq!(v30["returnsBreakdown"]["lootTt"], json!(50.0));
        assert_eq!(v30["returnsBreakdown"]["ledger"]["loot_sale"], json!(12.5));
        assert_eq!(v30["lossesBreakdown"]["ledger"], json!({}));
        assert_eq!(v30["timeline"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn seeded_activity_dominance_and_filters() {
        let now = 1_800_000_000.0;
        let (_dir, db, pool) = open_env().await;
        seed_scenario(&pool, now).await;
        let v = activity_impl(&db).await.unwrap();
        // sess-z (zero kills) filtered out; sess-a -> dominant mob, sess-b -> tag.
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["mobName"], json!("Atrox"));
        assert_eq!(mobs[0]["kills"], json!(5));
        assert_eq!(mobs[0]["hours"], json!(1.0)); // 3600s / 3600
        assert_eq!(mobs[0]["cycled"], json!(6.75));
        // pesPer100Ped = (skill 3.0 / cycled 6.75) * 100; lootRate = loot 50 / cycled.
        assert_eq!(mobs[0]["pesPer100Ped"], json!(44.44));
        assert_eq!(mobs[0]["lootRate"], json!(7.4074));
        let tags = v["tagComparisons"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0]["tagName"], json!("Thing"));
        assert_eq!(tags[0]["kills"], json!(3));
        assert_eq!(tags[0]["cycled"], json!(2.4));
        assert_eq!(tags[0]["pesPer100Ped"], json!(41.67));
        assert_eq!(tags[0]["lootRate"], json!(6.25));
        // weapon comparison keys kills off the session total (5 + 3 = 8) and
        // aggregates both sessions' hours / cycled / rates.
        let weapons = v["weaponComparisons"].as_array().unwrap();
        assert_eq!(weapons.len(), 1);
        assert_eq!(weapons[0]["weaponName"], json!("Opalo"));
        assert_eq!(weapons[0]["kills"], json!(8));
        assert_eq!(weapons[0]["hours"], json!(2.0));
        assert_eq!(weapons[0]["cycled"], json!(9.15));
        assert_eq!(weapons[0]["pesPer100Ped"], json!(43.72));
        assert_eq!(weapons[0]["lootRate"], json!(7.1038));
    }

    /// The activity filter drops a session failing ANY of the three guards
    /// (duration > 0, cycled > 0, kills > 0); `||` not `&&`. Three sessions,
    /// each dominated by its own mob, each failing exactly one guard except
    /// the keeper: only the keeper's mob survives.
    #[tokio::test]
    async fn activity_filter_drops_a_session_failing_any_single_guard() {
        let (_dir, db, pool) = open_env().await;
        // keeper: kills, duration, cost all positive.
        seed_filter_session(&pool, "keep", "Keeper", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // zero cost -> cycled 0 -> dropped by the cycled guard alone.
        seed_filter_session(&pool, "zcost", "Zerocost", 1000.0, 1000.0 + 3600.0, 0.0, 2).await;
        // zero duration (start == end) -> dropped by the duration guard alone.
        seed_filter_session(&pool, "zdur", "Zerodur", 1000.0, 1000.0, 5.0, 2).await;
        let v = activity_impl(&db).await.unwrap();
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1, "only the keeper survives the OR filter");
        assert_eq!(mobs[0]["mobName"], json!("Keeper"));
    }

    async fn seed_filter_session(
        pool: &SqlitePool,
        id: &str,
        mob: &str,
        start: f64,
        end: f64,
        armour: f64,
        kills: i64,
    ) {
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
             VALUES(?,?,?,?,0,0)",
        )
        .bind(id).bind(start).bind(end).bind(armour)
        .execute(pool).await.expect("seed");
        for i in 0..kills {
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(format!("{id}-k{i}")).bind(id).bind(mob).bind("Spec").bind("Young")
            .bind(start + i as f64).bind(0.0).bind(1.0)
            .execute(pool).await.expect("seed");
        }
    }

    /// Seed one session (cost via armour) and `kills` loot rows at `ts`, so a
    /// window's rate is loot_total / armour_cost.
    async fn seed_rate(pool: &SqlitePool, id: &str, ts: f64, cost: f64, kills: i64, loot: f64) {
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
             VALUES(?,?,?,?,0,0)",
        )
        .bind(id).bind(ts).bind(ts + 3600.0).bind(cost)
        .execute(pool).await.expect("seed");
        for i in 0..kills {
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,?,?,?,0,?)",
            )
            .bind(format!("{id}-k{i}"))
            .bind(id)
            .bind("M")
            .bind(ts + i as f64)
            .bind(loot)
            .execute(pool)
            .await
            .expect("seed");
        }
    }

    /// The trend compares the recent-30d rate against the prior-30d rate with
    /// a +/-2% band, guarded by both rates being positive.
    #[tokio::test]
    async fn overview_trend_bands() {
        let now = 1_800_000_000.0;
        let day = 86400.0;
        let trend = |v: Value| v["trend"].clone();

        // declining: recent rate 1.0 (10/10) below prior 2.0 (20/10) * 0.98.
        let (_dir, db, pool) = open_env().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("declining")
        );

        // improving: recent 2.0 above prior 1.0 * 1.02.
        let (_dir, db, pool) = open_env().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("improving")
        );

        // stable: recent equals prior, inside the band.
        let (_dir, db, pool) = open_env().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("stable")
        );

        // zero recent rate: the positivity guard short-circuits to stable
        // (a mutated guard would fall through into the banding and declare a
        // direction).
        let (_dir, db, pool) = open_env().await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(overview_impl(&db, now, "all").await.unwrap()),
            json!("stable")
        );

        // zero prior rate: the other half of the guard.
        let (_dir, db, pool) = open_env().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
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
        let (_dir, db, pool) = open_env().await;
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
             VALUES('nd',1000.0,4600.0,5.0,0,0)",
        )
        .execute(&pool).await.expect("seed");
        for (i, mob) in ["Alpha", "Bravo", "Charlie"].iter().enumerate() {
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,'nd',?,'Spec','Young',?,0,1.0)",
            )
            .bind(format!("nd-{i}")).bind(*mob).bind(1000.0 + i as f64)
            .execute(&pool).await.expect("seed");
        }
        let v = activity_impl(&db).await.unwrap();
        assert_eq!(v["mobComparisons"].as_array().unwrap().len(), 0);
        assert_eq!(v["tagComparisons"].as_array().unwrap().len(), 0);

        // Asymmetric: species present, maturity empty -> still a mob (the
        // presence test is OR, not AND), so it lands in mobComparisons.
        let (_dir, db, pool) = open_env().await;
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,armour_cost,heal_cost,dangling_cost) \
             VALUES('as',1000.0,4600.0,5.0,0,0)",
        )
        .execute(&pool).await.expect("seed");
        for i in 0..2 {
            sqlx::query(
                "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
                 VALUES(?,'as','Foo','Bar','',?,0,1.0)",
            )
            .bind(format!("as-{i}")).bind(1000.0 + i as f64)
            .execute(&pool).await.expect("seed");
        }
        let v = activity_impl(&db).await.unwrap();
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["mobName"], json!("Foo"));
        assert_eq!(v["tagComparisons"].as_array().unwrap().len(), 0);
    }

    /// A kill referencing a session that does not exist (representable with
    /// foreign keys off, as the app runs) is not counted: it belongs to no
    /// session, so it never enters any session's aggregate.
    #[tokio::test]
    async fn activity_ignores_a_kill_for_a_missing_session() {
        let (_dir, db, pool) = open_env().await;
        // A valid completed session with one dominant-mob kill.
        seed_filter_session(&pool, "ok", "Real", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // An orphan kill whose session_id matches no tracking_sessions row.
        sqlx::query(
            "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
             VALUES('orphan','ghost-session','Ghost','Spec','Young',1.0,0,9.0)",
        )
        .execute(&pool).await.expect("seed");
        // Only the real session's mob is compared; the orphan is ignored.
        let v = activity_impl(&db).await.unwrap();
        let mobs = v["mobComparisons"].as_array().unwrap();
        assert_eq!(mobs.len(), 1);
        assert_eq!(mobs[0]["mobName"], json!("Real"));
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
    fn float_field_coerces_integers_only() {
        assert_eq!(float_field(json!(0)), json!(0.0));
        assert_eq!(float_field(json!(3)), json!(3.0));
        assert_eq!(float_field(json!(1.5)), json!(1.5));
    }

    #[test]
    fn rounded_preserves_integers_and_banker_rounds_floats() {
        assert_eq!(rounded(&json!(0), 2), json!(0)); // int stays int
        assert_eq!(rounded(&json!(1.005), 2), json!(1.0)); // half-even
        assert_eq!(rounded(&json!(2.675), 2), json!(2.67));
    }

    #[test]
    fn number_sum_is_integral_only_when_both_are() {
        assert_eq!(number_sum(&json!(2), &json!(3)), json!(5));
        assert_eq!(number_sum(&json!(2), &json!(0.5)), json!(2.5));
    }

    // ── Hermetic write-handler tests (the mutation campaign's kills) ──

    /// Create then list round-trips for the ledger: the create echoes the
    /// input plus a generated id, and the list reads it back.
    #[tokio::test]
    async fn ledger_create_and_list_round_trip() {
        let (_dir, service) = write_service().await;
        let body = service
            .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
            .await
            .unwrap();
        assert_eq!(body["date"], json!("2026-05-01"));
        assert_eq!(body["type"], json!("expense"));
        assert_eq!(body["amount"], json!(12.5));
        assert_eq!(body["tag"], json!("ammo"));
        assert!(body["id"].as_str().is_some(), "create generates an id");

        let page = service.list_ledger(None, None).await.unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0]["description"], json!("Ammo"));
        assert_eq!(page.entries[0]["id"], body["id"]);
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
                seen.push(row["description"].as_str().unwrap().to_string());
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

    /// Create with the optional fields absent: notes is null and acquired_at
    /// defaults to the (frozen) clock's UTC date.
    #[tokio::test]
    async fn inventory_create_defaults_date_and_notes() {
        let (_dir, service) = write_service().await;
        let body = service
            .create_inventory_item("Imk2", 50.0, 5.0, None, None)
            .await
            .unwrap();
        // Response is camelCase even though the request is snake_case.
        assert_eq!(body["ttValue"], json!(50.0));
        assert_eq!(body["markupPaid"], json!(5.0));
        assert_eq!(body["notes"], Value::Null);
        assert_eq!(body["acquiredAt"], json!("2026-06-01"));

        // An explicit acquired_at / notes are honoured.
        let body = service
            .create_inventory_item("X", 1.0, 0.0, Some("spare"), Some("2026-01-02"))
            .await
            .unwrap();
        assert_eq!(body["notes"], json!("spare"));
        assert_eq!(body["acquiredAt"], json!("2026-01-02"));
    }

    /// PATCH field-selection: only PROVIDED (Some) fields update; a None
    /// field is left untouched, exactly as the reference's
    /// `if patch.x is not None`.
    #[tokio::test]
    async fn inventory_patch_updates_only_provided_fields() {
        let (_dir, service) = write_service().await;
        let created = service
            .create_inventory_item("Orig", 20.0, 3.0, Some("keep"), Some("2026-03-01"))
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();

        // Provide name + tt_value only: markup_paid and notes stay.
        let patched = service
            .update_inventory_item(&id, Some("Renamed"), Some(25.0), None, None)
            .await
            .unwrap()
            .expect("the item exists");
        assert_eq!(patched["name"], json!("Renamed"));
        assert_eq!(patched["ttValue"], json!(25.0));
        assert_eq!(patched["markupPaid"], json!(3.0), "untouched");
        assert_eq!(patched["notes"], json!("keep"), "untouched");

        // An all-None patch re-reads and returns the row unchanged.
        let same = service
            .update_inventory_item(&id, None, None, None, None)
            .await
            .unwrap()
            .expect("the item exists");
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
        let item = service
            .create_inventory_item("Sword", 10.0, 2.0, None, Some("2026-02-01"))
            .await
            .unwrap();
        let id = item["id"].as_str().unwrap().to_string();
        let body = service
            .sell_inventory_item(&id, 20.0, None, Some("2026-05-10"))
            .await
            .unwrap()
            .expect("the item exists");
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
        let item = service
            .create_inventory_item("Shield", 10.0, 2.0, None, Some("2026-02-01"))
            .await
            .unwrap();
        let id = item["id"].as_str().unwrap().to_string();
        let body = service
            .sell_inventory_item(&id, 5.0, Some("Dumped it"), None)
            .await
            .unwrap()
            .expect("the item exists");
        let entry = &body["ledgerEntry"];
        assert_eq!(entry["type"], json!("expense"));
        assert_eq!(entry["amount"], json!(7.0));
        assert_eq!(entry["description"], json!("Dumped it"));
        // Default sold_at is the frozen clock date.
        assert_eq!(entry["date"], json!("2026-06-01"));

        // ZERO-DELTA: sale == cost -> no ledger entry, item still removed.
        let (_dir, service) = write_service().await;
        let item = service
            .create_inventory_item("Even", 8.0, 2.0, None, Some("2026-02-01"))
            .await
            .unwrap();
        let id = item["id"].as_str().unwrap().to_string();
        let body = service
            .sell_inventory_item(&id, 10.0, None, None)
            .await
            .unwrap()
            .expect("the item exists");
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
        let created = service
            .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        // A successful delete reports true (the row existed); a second delete
        // reports false (nothing to remove).
        assert!(service.delete_ledger_entry(&id).await.unwrap());
        assert!(!service.delete_ledger_entry(&id).await.unwrap());
    }

    #[tokio::test]
    async fn preset_list_shapes_rows_then_delete_removes() {
        let (_dir, service) = write_service().await;
        let created = service
            .create_ledger_preset("Decay", "expense", "d", 0.5, "decay")
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        // The list shapes the row via preset_item (not an empty default).
        let rows = service.list_ledger_presets().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], json!("Decay"));
        assert_eq!(rows[0]["amount"], json!(0.5));
        assert_eq!(rows[0]["tag"], json!("decay"));
        assert!(service.delete_ledger_preset(&id).await.unwrap());
        assert!(!service.delete_ledger_preset(&id).await.unwrap());
    }

    #[tokio::test]
    async fn inventory_delete_removes_then_reports_missing() {
        let (_dir, service) = write_service().await;
        let created = service
            .create_inventory_item("Sword", 10.0, 2.0, None, None)
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap().to_string();
        assert!(service.delete_inventory_item(&id).await.unwrap());
        assert!(!service.delete_inventory_item(&id).await.unwrap());
    }
}
