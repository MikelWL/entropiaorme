//! Native analytics reads: the
//! `/api/analytics/overview` and `/api/analytics/activity` GETs.
//!
//! These handlers are router-resident SQL aggregation: the reference keeps
//! every query and the camelCase shaping in the router itself (no service
//! layer), reading only the single-owner connection and the injected clock.
//! The port mirrors that, running the same statements over `self.read()` /
//! `self.write()` and `self.clock`, and shaping the result to the `AnalyticsOverview` /
//! `AnalyticsActivity` response models byte-for-byte.
//!
//! The fidelity crux is pydantic's response-model coercion. A field typed
//! `float` coerces an engine-typed integer to its float form (`0` -> `0.0`);
//! a field typed `Any` (the `cycledBreakdown` map and the `ledgerGains` /
//! `ledgerLosses` timeline maps) passes the value through untouched, so an
//! empty `COALESCE(SUM(...), 0)` leaves the wire as the integer `0`. The
//! `sql_number` reader preserves the engine type (the quest-analytics
//! precedent), `rounded` applies Python's type-preserving `round`, and
//! `float_field` performs the model's int-to-float coercion only where the
//! model declares a float.

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{Response, StatusCode};
use eo_services::daily_rollup;
use eo_services::db::{Db, DbError};
use eo_services::tracker::naive_to_epoch;
use serde_json::{json, Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::hydration::{
    detail, error_response, internal_error, plain_json_response, HydrationState,
};

const ACTIVITY_DOMINANCE_THRESHOLD: f64 = 0.6;

// ── Engine-typed numeric primitives (the quest-analytics siblings in
//    eo-services::quests; kept local so this router stays self-contained,
//    matching the per-file formatter convention in hydration/character) ──

/// A SQLite numeric read preserving the engine type: a REAL decodes to a
/// float, an INTEGER (including the `COALESCE(SUM(...), 0)` empty case) to an
/// integer. `try_get::<f64>` rejects an integer-affinity value, so the
/// integer arm fires for the NULL-sum zeros.
fn sql_number(row: &SqliteRow, index: usize) -> Value {
    match row.try_get::<f64, _>(index) {
        Ok(value) => json!(value),
        Err(_) => json!(row.get::<i64, _>(index)),
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
fn as_float(row: &SqliteRow, index: usize) -> f64 {
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

async fn rollup_family_sums(
    pool: &SqlitePool,
    lo: Option<&str>,
    hi: &str,
) -> Result<FamilySums, sqlx::Error> {
    let mut sql = String::from(
        "SELECT SUM(loot_tt), SUM(weapon_cost), SUM(enhancer_cost), SUM(armour_cost), \
         SUM(heal_cost), SUM(dangling_cost), SUM(skill_tt), SUM(codex_pes), SUM(quest_pes) \
         FROM daily_rollups WHERE day <= ?",
    );
    if lo.is_some() {
        sql.push_str(" AND day >= ?");
    }
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(hi);
    if let Some(lo) = lo {
        query = query.bind(lo);
    }
    let row = query.fetch_one(pool).await?;
    let mut sums: FamilySums = [None; 9];
    for (index, slot) in sums.iter_mut().enumerate() {
        *slot = row.try_get(index)?;
    }
    Ok(sums)
}

/// One raw part's family sums over `[start, end)`, verbatim (NULL kept)
/// so the merge preserves engine typing.
async fn raw_family_sums(
    pool: &SqlitePool,
    range: (Option<f64>, Option<f64>),
) -> Result<FamilySums, sqlx::Error> {
    async fn fetch(
        pool: &SqlitePool,
        sql: String,
        params: &[f64],
        sums: usize,
    ) -> Result<Vec<Option<f64>>, sqlx::Error> {
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for value in params {
            query = query.bind(*value);
        }
        let row = query.fetch_one(pool).await?;
        (0..sums).map(|index| row.try_get(index)).collect()
    }

    let (start, end) = range;
    let mut sums: FamilySums = [None; 9];
    let (w, p) = where_epoch("timestamp", start, end);
    let kills = fetch(
        pool,
        format!("SELECT SUM(loot_total_ped), SUM(enhancer_cost) FROM kills WHERE {w}"),
        &p,
        2,
    )
    .await?;
    sums[0] = kills[0];
    sums[2] = kills[1];

    let (w, p) = where_epoch("k.timestamp", start, end);
    let weapon = fetch(
        pool,
        format!(
            "SELECT SUM(ts.cost_per_shot * ts.shots_fired) \
             FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id WHERE {w}"
        ),
        &p,
        1,
    )
    .await?;
    sums[1] = weapon[0];

    let (w, p) = where_epoch("started_at", start, end);
    let sessions = fetch(
        pool,
        format!(
            "SELECT SUM(armour_cost), SUM(heal_cost), SUM(dangling_cost) \
             FROM tracking_sessions WHERE {w}"
        ),
        &p,
        3,
    )
    .await?;
    sums[3] = sessions[0];
    sums[4] = sessions[1];
    sums[5] = sessions[2];

    let (w, p) = where_epoch("timestamp", start, end);
    sums[6] = fetch(
        pool,
        format!("SELECT SUM(ped_value) FROM skill_gains WHERE {w}"),
        &p,
        1,
    )
    .await?[0];
    let (w, p) = where_epoch("claimed_at", start, end);
    sums[7] = fetch(
        pool,
        format!("SELECT SUM(ped_value) FROM codex_claims WHERE {w}"),
        &p,
        1,
    )
    .await?[0];
    let (w, p) = where_epoch("claimed_at", start, end);
    sums[8] = fetch(
        pool,
        format!("SELECT SUM(ped_value) FROM quest_claims WHERE {w}"),
        &p,
        1,
    )
    .await?[0];
    Ok(sums)
}

/// A day/month-keyed aggregate (`SELECT <bucket>, COALESCE(SUM(...), 0) ...
/// GROUP BY <bucket>`) collected as `bucket -> engine-typed number`,
/// preserving the SQL row order.
async fn bucketed_epoch(
    pool: &SqlitePool,
    sql: String,
    params: &[f64],
) -> Result<Map<String, Value>, sqlx::Error> {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for value in params {
        query = query.bind(*value);
    }
    let rows = query.fetch_all(pool).await?;
    let mut out = Map::new();
    for row in &rows {
        out.insert(row.get::<String, _>(0), sql_number(row, 1));
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
async fn ledger_by_tag(
    pool: &SqlitePool,
    entry_type: &str,
    epoch_start: Option<f64>,
    epoch_end: Option<f64>,
    watermark: &str,
) -> Result<Map<String, Value>, sqlx::Error> {
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

    let (extra, params) = bounds("day");
    let sql = format!(
        "SELECT tag, SUM(amount) FROM daily_ledger_rollups \
         WHERE entry_type = ? AND day <= ?{extra} GROUP BY tag"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(entry_type)
        .bind(watermark);
    for value in &params {
        query = query.bind(value);
    }
    for row in &query.fetch_all(pool).await? {
        *totals.entry(row.get(0)).or_insert(0.0) += row.get::<f64, _>(1);
    }

    let (extra, params) = bounds("le.date");
    let sql = format!(
        "SELECT le.tag, SUM(le.amount) FROM ledger_entries le \
         WHERE le.type = ? AND le.date > ?{extra} GROUP BY le.tag"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(entry_type)
        .bind(watermark);
    for value in &params {
        query = query.bind(value);
    }
    for row in &query.fetch_all(pool).await? {
        *totals.entry(row.get(0)).or_insert(0.0) += row.get::<f64, _>(1);
    }

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
async fn compute_metrics(
    pool: &SqlitePool,
    watermark: &str,
    epoch_start: Option<f64>,
    epoch_end: Option<f64>,
) -> Result<Metrics, sqlx::Error> {
    let window = hybrid_window(epoch_start, epoch_end, watermark);
    let mut sums: FamilySums = [None; 9];
    if let Some((lo, hi)) = &window.rollup_days {
        merge_family_sums(
            &mut sums,
            rollup_family_sums(pool, lo.as_deref(), hi).await?,
        );
    }
    for range in &window.raw_ranges {
        merge_family_sums(&mut sums, raw_family_sums(pool, *range).await?);
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

    let ledger_gains = ledger_by_tag(pool, "markup", epoch_start, epoch_end, watermark).await?;
    let ledger_losses = ledger_by_tag(pool, "expense", epoch_start, epoch_end, watermark).await?;

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
async fn ledger_buckets(
    pool: &SqlitePool,
    kind: BucketKind,
    entry_type: &str,
    epoch_start: Option<f64>,
    watermark: &str,
) -> Result<std::collections::BTreeMap<String, Map<String, Value>>, sqlx::Error> {
    let mut sums: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>> =
        std::collections::BTreeMap::new();
    let start_iso = epoch_start.map(epoch_to_iso);

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
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(entry_type)
        .bind(watermark);
    if let Some(start) = &start_iso {
        query = query.bind(start);
    }
    for row in &query.fetch_all(pool).await? {
        *sums
            .entry(row.get(0))
            .or_default()
            .entry(row.get(1))
            .or_insert(0.0) += row.get::<f64, _>(2);
    }

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
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(entry_type)
        .bind(watermark);
    if let Some(start) = &start_iso {
        query = query.bind(start);
    }
    for row in &query.fetch_all(pool).await? {
        *sums
            .entry(row.get(0))
            .or_default()
            .entry(row.get(1))
            .or_insert(0.0) += row.get::<f64, _>(2);
    }

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
    // reader-held connection); every subsequent aggregate is a plain read
    // on the reader pool.
    let watermark = daily_rollup::heal_rollups(db.write(), now).await?;
    let pool = db.read();
    let epoch_start = period_epoch(period, now);

    let m = compute_metrics(pool, &watermark, epoch_start, None).await?;
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
    let day_30 = now - 30.0 * 86400.0;
    let day_60 = now - 60.0 * 86400.0;
    let rate_30d = rate_from_metrics(&compute_metrics(pool, &watermark, Some(day_30), None).await?);
    let rate_prior =
        rate_from_metrics(&compute_metrics(pool, &watermark, Some(day_60), Some(day_30)).await?);
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
    let timeline = breakdown_points(pool, &watermark, epoch_start, "date", BucketKind::Day).await?;
    // Monthly breakdown.
    let monthly =
        breakdown_points(pool, &watermark, epoch_start, "month", BucketKind::Month).await?;

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
async fn rollup_breakdown(
    pool: &SqlitePool,
    maps: &mut BreakdownMaps,
    kind: BucketKind,
    lo: Option<&str>,
    hi: &str,
) -> Result<(), sqlx::Error> {
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
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(hi);
    if let Some(lo) = lo {
        query = query.bind(lo);
    }
    for row in &query.fetch_all(pool).await? {
        let bucket = row.get::<String, _>(0);
        if row.get::<i64, _>(1) != 0 {
            maps.members.insert(bucket.clone());
        }
        let family = |index: usize| row.try_get::<Option<f64>, _>(index);
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
async fn raw_breakdown(
    pool: &SqlitePool,
    maps: &mut BreakdownMaps,
    kind: BucketKind,
    range: (Option<f64>, Option<f64>),
) -> Result<(), sqlx::Error> {
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
        let buckets = bucketed_epoch(pool, sql, params).await?;
        for (bucket, value) in buckets {
            maps.members.insert(bucket.clone());
            BreakdownMaps::merge(map, &bucket, value);
        }
    }
    Ok(())
}

/// Build the timeline / monthly breakdown: per-source bucketed sums merged
/// over the union of all buckets, then one point per bucket in sorted order.
/// Hybrid over the rollup watermark, exactly as [`compute_metrics`].
async fn breakdown_points(
    pool: &SqlitePool,
    watermark: &str,
    epoch_start: Option<f64>,
    bucket_label: &str,
    kind: BucketKind,
) -> Result<Value, sqlx::Error> {
    let window = hybrid_window(epoch_start, None, watermark);
    let mut maps = BreakdownMaps::default();
    if let Some((lo, hi)) = &window.rollup_days {
        rollup_breakdown(pool, &mut maps, kind, lo.as_deref(), hi).await?;
    }
    for range in &window.raw_ranges {
        raw_breakdown(pool, &mut maps, kind, *range).await?;
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

    let gains = ledger_buckets(pool, kind, "markup", epoch_start, watermark).await?;
    let losses = ledger_buckets(pool, kind, "expense", epoch_start, watermark).await?;

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
    // fresh install) sees current rows; the read itself runs on the reader.
    eo_services::session_summary::heal_summaries(db.write()).await?;
    let pool = db.read();
    let mut sessions = read_summary_activity_aggs(pool).await?;

    // Reconcile the sessions Activity counts but a summary never holds: an
    // ended session with kills and cost but no skill gains qualifies for
    // Activity yet fails the summary's gains requirement, so it has no summary
    // row. Rare (usually none); computed raw only for those ids, so the cost
    // scales with the divergence, not the whole history.
    let divergent = sqlx::query(
        "SELECT s.id, s.started_at, s.ended_at, COALESCE(s.armour_cost, 0), \
         COALESCE(s.heal_cost, 0), COALESCE(s.dangling_cost, 0) \
         FROM tracking_sessions s \
         LEFT JOIN session_summaries ss ON ss.session_id = s.id \
         WHERE s.ended_at IS NOT NULL AND ss.session_id IS NULL",
    )
    .fetch_all(pool)
    .await?;
    for row in &divergent {
        let id = row.get::<String, _>(0);
        let started: f64 = row.try_get::<f64, _>(1).unwrap_or(0.0);
        let ended: f64 = row.try_get::<f64, _>(2).unwrap_or(0.0);
        let agg = raw_session_agg(
            pool,
            &id,
            started,
            ended,
            as_float(row, 3),
            as_float(row, 4),
            as_float(row, 5),
        )
        .await?;
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
async fn read_summary_activity_aggs(
    pool: &SqlitePool,
) -> Result<std::collections::HashMap<String, SessionAgg>, DbError> {
    let rows = sqlx::query(
        "SELECT session_id, duration_hours, kills, loot_tt, cycled_ped, activity_skill_tt, \
         dominant_mob, dominant_tag, dominant_weapon, dominant_mob_kills, dominant_tag_kills \
         FROM session_summaries",
    )
    .fetch_all(pool)
    .await?;
    let mut out = std::collections::HashMap::with_capacity(rows.len());
    for row in &rows {
        let id = row.get::<String, _>(0);
        out.insert(
            id,
            SessionAgg {
                duration_hours: as_float(row, 1),
                kills: row.try_get::<i64, _>(2).unwrap_or(0),
                loot_tt: as_float(row, 3),
                cycled_ped: as_float(row, 4),
                skill_tt: as_float(row, 5),
                dominant_mob: row.get::<Option<String>, _>(6),
                dominant_tag: row.get::<Option<String>, _>(7),
                dominant_weapon: row.get::<Option<String>, _>(8),
                dominant_mob_kills: row.try_get::<i64, _>(9).unwrap_or(0),
                dominant_tag_kills: row.try_get::<i64, _>(10).unwrap_or(0),
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
async fn raw_session_agg(
    pool: &SqlitePool,
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

    let kill_row = sqlx::query(
        "SELECT COUNT(*), COALESCE(SUM(loot_total_ped), 0), COALESCE(SUM(enhancer_cost), 0) \
         FROM kills WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    agg.kills = kill_row.get::<i64, _>(0);
    agg.loot_tt = as_float(&kill_row, 1);
    agg.enhancer_cost = as_float(&kill_row, 2);

    let weapon_row = sqlx::query(
        "SELECT COALESCE(SUM(ts.cost_per_shot * ts.shots_fired), 0), \
         COALESCE(SUM(ts.shots_fired), 0) FROM kill_tool_stats ts \
         JOIN kills k ON k.id = ts.kill_id WHERE k.session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    agg.weapon_cost = as_float(&weapon_row, 0);
    agg.weapon_shots = as_float(&weapon_row, 1);

    let skill_row = sqlx::query(
        "SELECT COALESCE(SUM(ped_value), 0) FROM skill_gains \
         WHERE session_id = ? AND ped_value IS NOT NULL",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    agg.skill_tt = as_float(&skill_row, 0);

    let mob_rows = sqlx::query(
        "SELECT mob_name, COALESCE(mob_species, ''), COALESCE(mob_maturity, ''), COUNT(*) \
         FROM kills WHERE session_id = ? AND mob_name IS NOT NULL AND mob_name != 'Unknown' \
         GROUP BY mob_name, mob_species, mob_maturity ORDER BY COUNT(*) DESC, mob_name ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    if !mob_rows.is_empty() {
        let total_known: i64 = mob_rows
            .iter()
            .map(|r| r.try_get::<i64, _>(3).unwrap_or(0))
            .sum();
        if total_known > 0 {
            let top_name: String = mob_rows[0].get(0);
            let top_species: String = mob_rows[0].get(1);
            let top_maturity: String = mob_rows[0].get(2);
            let top_count: i64 = mob_rows[0].get(3);
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

    let tool_rows = sqlx::query(
        "SELECT ts.tool_name, COALESCE(SUM(ts.shots_fired), 0) \
         FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id \
         WHERE k.session_id = ? AND ts.tool_name IS NOT NULL AND ts.tool_name != 'Unknown' \
         GROUP BY ts.tool_name ORDER BY SUM(ts.shots_fired) DESC, ts.tool_name ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    if !tool_rows.is_empty() {
        let total_shots: f64 = tool_rows.iter().map(|r| as_float(r, 1)).sum();
        let top_name: String = tool_rows[0].get(0);
        let top_shots = as_float(&tool_rows[0], 1);
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

// ── The two handlers on the composition-root state ──

impl HydrationState {
    /// GET /api/analytics/overview?period=...
    ///
    /// Scales O(days), not O(kills): the aggregates read the daily
    /// rollup projection for completed days and touch the raw tables
    /// only for the partial edge days (see [`overview_impl`]).
    pub async fn analytics_overview(&self, period: &str) -> Response<Body> {
        let now = naive_to_epoch(self.clock.now());
        match overview_impl(&self.db, now, period).await {
            Ok(value) => plain_json_response(&value),
            Err(_) => internal_error(),
        }
    }

    /// GET /api/analytics/activity (no conditional-GET contract: the
    /// analytics surface is outside the ETag middleware's prefixes).
    pub async fn analytics_activity(&self, _if_none_match: Option<&str>) -> Response<Body> {
        match activity_impl(&self.db).await {
            Ok(value) => plain_json_response(&value),
            Err(_) => internal_error(),
        }
    }
}

// ── Ledger / presets / inventory writes (the CRUD surface) ──

const INVENTORY_SALE_TAG: &str = "inventory_sale";

/// `LedgerItem` / `LedgerPresetItem` share a shape; both select
/// (id, name-or-date, type, description, amount, tag).
fn ledger_item(row: &SqliteRow) -> Value {
    json!({
        "id": row.get::<String, _>(0),
        "date": row.get::<String, _>(1),
        "type": row.get::<String, _>(2),
        "description": row.get::<String, _>(3),
        "amount": float_field(sql_number(row, 4)),
        "tag": row.get::<String, _>(5),
    })
}

fn preset_item(row: &SqliteRow) -> Value {
    json!({
        "id": row.get::<String, _>(0),
        "name": row.get::<String, _>(1),
        "type": row.get::<String, _>(2),
        "description": row.get::<String, _>(3),
        "amount": float_field(sql_number(row, 4)),
        "tag": row.get::<String, _>(5),
    })
}

/// `_inventory_row_to_dict`: (id, name, tt_value, markup_paid, notes, acquired_at).
fn inventory_item(row: &SqliteRow) -> Value {
    json!({
        "id": row.get::<String, _>(0),
        "name": row.get::<String, _>(1),
        "ttValue": float_field(sql_number(row, 2)),
        "markupPaid": float_field(sql_number(row, 3)),
        "notes": row.get::<Option<String>, _>(4),
        "acquiredAt": row.get::<String, _>(5),
    })
}

impl HydrationState {
    /// `_utc_date_str(clock)`: the clock's instant as a UTC YYYY-MM-DD date.
    fn default_date(&self) -> String {
        epoch_to_iso(naive_to_epoch(self.clock.now()))
    }

    /// GET /api/analytics/ledger
    pub async fn list_ledger(&self) -> Response<Body> {
        match sqlx::query(
            "SELECT id, date, type, description, amount, tag FROM ledger_entries \
             ORDER BY date DESC, id DESC",
        )
        .fetch_all(self.read())
        .await
        {
            Ok(rows) => plain_json_response(&Value::Array(rows.iter().map(ledger_item).collect())),
            Err(_) => internal_error(),
        }
    }

    /// POST /api/analytics/ledger
    pub async fn create_ledger_entry(
        &self,
        date: &str,
        kind: &str,
        description: &str,
        amount: f64,
        tag: &str,
    ) -> Response<Body> {
        let id = Uuid::new_v4().to_string();
        // One transaction over the insert and the rollup refresh: a
        // backdated entry relands its day's rollup with the write.
        let write = async {
            let mut tx = self.write().begin().await?;
            sqlx::query(
                "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(date)
            .bind(kind)
            .bind(description)
            .bind(amount)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
            eo_services::daily_rollup::refresh_days(&mut tx, [date]).await?;
            tx.commit().await?;
            Ok::<(), eo_services::db::DbError>(())
        };
        match write.await {
            Ok(()) => plain_json_response(&json!({
                "id": id, "date": date, "type": kind,
                "description": description, "amount": amount, "tag": tag,
            })),
            Err(_) => internal_error(),
        }
    }

    /// DELETE /api/analytics/ledger/{entry_id}
    pub async fn delete_ledger_entry(&self, entry_id: &str) -> Response<Body> {
        // Capture the entry's day before deleting so its rollup relands
        // in the same transaction; a vanished entry keeps the 404.
        let write = async {
            let mut tx = self.write().begin().await?;
            let date: Option<String> =
                sqlx::query_scalar("SELECT date FROM ledger_entries WHERE id = ?")
                    .bind(entry_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some(date) = date else {
                return Ok::<bool, eo_services::db::DbError>(false);
            };
            sqlx::query("DELETE FROM ledger_entries WHERE id = ?")
                .bind(entry_id)
                .execute(&mut *tx)
                .await?;
            eo_services::daily_rollup::refresh_days(&mut tx, [date]).await?;
            tx.commit().await?;
            Ok(true)
        };
        match write.await {
            Ok(false) => error_response(StatusCode::NOT_FOUND, &detail("Entry not found")),
            Ok(true) => plain_json_response(&json!({"status": "deleted"})),
            Err(_) => internal_error(),
        }
    }

    /// GET /api/analytics/ledger/presets
    pub async fn list_ledger_presets(&self) -> Response<Body> {
        match sqlx::query(
            "SELECT id, name, type, description, amount, tag FROM ledger_presets \
             ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(self.read())
        .await
        {
            Ok(rows) => plain_json_response(&Value::Array(rows.iter().map(preset_item).collect())),
            Err(_) => internal_error(),
        }
    }

    /// POST /api/analytics/ledger/presets
    pub async fn create_ledger_preset(
        &self,
        name: &str,
        kind: &str,
        description: &str,
        amount: f64,
        tag: &str,
    ) -> Response<Body> {
        if kind != "expense" && kind != "markup" {
            return error_response(
                StatusCode::BAD_REQUEST,
                &detail("type must be 'expense' or 'markup'"),
            );
        }
        let id = Uuid::new_v4().to_string();
        match sqlx::query(
            "INSERT INTO ledger_presets (id, name, type, description, amount, tag) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(kind)
        .bind(description)
        .bind(amount)
        .bind(tag)
        .execute(self.write())
        .await
        {
            Ok(_) => plain_json_response(&json!({
                "id": id, "name": name, "type": kind,
                "description": description, "amount": amount, "tag": tag,
            })),
            Err(_) => internal_error(),
        }
    }

    /// DELETE /api/analytics/ledger/presets/{preset_id}
    pub async fn delete_ledger_preset(&self, preset_id: &str) -> Response<Body> {
        match sqlx::query("DELETE FROM ledger_presets WHERE id = ?")
            .bind(preset_id)
            .execute(self.write())
            .await
        {
            Ok(result) if result.rows_affected() == 0 => {
                error_response(StatusCode::NOT_FOUND, &detail("Preset not found"))
            }
            Ok(_) => plain_json_response(&json!({"status": "deleted"})),
            Err(_) => internal_error(),
        }
    }

    /// GET /api/analytics/inventory
    pub async fn list_inventory(&self) -> Response<Body> {
        match sqlx::query(
            "SELECT id, name, tt_value, markup_paid, notes, acquired_at FROM inventory_items \
             ORDER BY acquired_at DESC, id DESC",
        )
        .fetch_all(self.read())
        .await
        {
            Ok(rows) => {
                plain_json_response(&Value::Array(rows.iter().map(inventory_item).collect()))
            }
            Err(_) => internal_error(),
        }
    }

    /// The stored inventory row re-read and shaped (create / patch reply).
    async fn inventory_response(&self, item_id: &str) -> Response<Body> {
        match sqlx::query(
            "SELECT id, name, tt_value, markup_paid, notes, acquired_at \
             FROM inventory_items WHERE id = ?",
        )
        .bind(item_id)
        .fetch_optional(self.read())
        .await
        {
            Ok(Some(row)) => plain_json_response(&inventory_item(&row)),
            _ => internal_error(),
        }
    }

    /// POST /api/analytics/inventory
    pub async fn create_inventory_item(
        &self,
        name: &str,
        tt_value: f64,
        markup_paid: f64,
        notes: Option<&str>,
        acquired_at: Option<&str>,
    ) -> Response<Body> {
        let id = Uuid::new_v4().to_string();
        // `item.acquired_at or _utc_date_str(clock)`: the reference's `or`
        // treats an empty string as falsy, so "" defaults to the clock date.
        let date = acquired_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        if sqlx::query(
            "INSERT INTO inventory_items (id, name, tt_value, markup_paid, notes, acquired_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(tt_value)
        .bind(markup_paid)
        .bind(notes)
        .bind(&date)
        .execute(self.write())
        .await
        .is_err()
        {
            return internal_error();
        }
        self.inventory_response(&id).await
    }

    /// PATCH /api/analytics/inventory/{item_id}: only provided (non-null)
    /// fields update, bumping updated_at; an absent body of fields still
    /// re-reads and returns the row (the reference's shape).
    pub async fn update_inventory_item(
        &self,
        item_id: &str,
        name: Option<&str>,
        tt_value: Option<f64>,
        markup_paid: Option<f64>,
        notes: Option<&str>,
    ) -> Response<Body> {
        match sqlx::query("SELECT id FROM inventory_items WHERE id = ?")
            .bind(item_id)
            .fetch_optional(self.read())
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => {
                return error_response(StatusCode::NOT_FOUND, &detail("Inventory item not found"))
            }
            Err(_) => return internal_error(),
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
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            if let Some(value) = name {
                query = query.bind(value);
            }
            if let Some(value) = tt_value {
                query = query.bind(value);
            }
            if let Some(value) = markup_paid {
                query = query.bind(value);
            }
            if let Some(value) = notes {
                query = query.bind(value);
            }
            query = query.bind(item_id);
            if query.execute(self.write()).await.is_err() {
                return internal_error();
            }
        }
        self.inventory_response(item_id).await
    }

    /// DELETE /api/analytics/inventory/{item_id}
    pub async fn delete_inventory_item(&self, item_id: &str) -> Response<Body> {
        match sqlx::query("DELETE FROM inventory_items WHERE id = ?")
            .bind(item_id)
            .execute(self.write())
            .await
        {
            Ok(result) if result.rows_affected() == 0 => {
                error_response(StatusCode::NOT_FOUND, &detail("Inventory item not found"))
            }
            Ok(_) => plain_json_response(&json!({"status": "deleted"})),
            Err(_) => internal_error(),
        }
    }

    /// POST /api/analytics/inventory/{item_id}/sell: emit the realised delta
    /// to the ledger and remove the row, atomically; a zero-delta sale skips
    /// the ledger row and returns ledgerEntry null.
    pub async fn sell_inventory_item(
        &self,
        item_id: &str,
        sale_price: f64,
        description: Option<&str>,
        sold_at: Option<&str>,
    ) -> Response<Body> {
        let row = match sqlx::query(
            "SELECT id, name, tt_value, markup_paid, notes, acquired_at \
             FROM inventory_items WHERE id = ?",
        )
        .bind(item_id)
        .fetch_optional(self.read())
        .await
        {
            Ok(Some(row)) => row,
            Ok(None) => {
                return error_response(StatusCode::NOT_FOUND, &detail("Inventory item not found"))
            }
            Err(_) => return internal_error(),
        };

        let name = row.get::<String, _>(1);
        let tt_value = sql_number(&row, 2).as_f64().unwrap_or(0.0);
        let markup_paid = sql_number(&row, 3).as_f64().unwrap_or(0.0);
        let cost_basis = tt_value + markup_paid;
        let delta = sale_price - cost_basis;
        // `payload.sold_at or _utc_date_str(clock)`: empty string is falsy.
        let sold_at = sold_at
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.default_date());
        let sold_item = inventory_item(&row);

        let mut tx = match self.write().begin().await {
            Ok(tx) => tx,
            Err(_) => return internal_error(),
        };
        let ledger_entry = if delta != 0.0 {
            let entry_id = Uuid::new_v4().to_string();
            let entry_type = if delta > 0.0 { "markup" } else { "expense" };
            let amount = delta.abs();
            // `payload.description or "Inventory Sale: {name}"`: "" is falsy.
            let description = description
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Inventory Sale: {name}"));
            if sqlx::query(
                "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&entry_id)
            .bind(&sold_at)
            .bind(entry_type)
            .bind(&description)
            .bind(amount)
            .bind(INVENTORY_SALE_TAG)
            .execute(&mut *tx)
            .await
            .is_err()
            {
                return internal_error();
            }
            if eo_services::daily_rollup::refresh_days(&mut tx, [&sold_at])
                .await
                .is_err()
            {
                return internal_error();
            }
            json!({
                "id": entry_id, "date": sold_at, "type": entry_type,
                "description": description, "amount": amount, "tag": INVENTORY_SALE_TAG,
            })
        } else {
            Value::Null
        };
        if sqlx::query("DELETE FROM inventory_items WHERE id = ?")
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return internal_error();
        }
        if tx.commit().await.is_err() {
            return internal_error();
        }
        plain_json_response(&json!({"ledgerEntry": ledger_entry, "soldItem": sold_item}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eo_wire::normalizer::to_wire_json;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn memory_pool() -> SqlitePool {
        use std::str::FromStr;
        // Match the production connection surface (foreign keys off, as the app
        // opens the database) so the schema's REFERENCES clauses stay
        // declarative here too.
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("memory url")
            .foreign_keys(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("memory pool");
        // Build the real schema (the same migration chain the app runs), so the
        // reads that now depend on the full surface (session_summaries,
        // notable_events, the complete skill_gains columns) exercise the true
        // shape rather than a hand-trimmed subset.
        sqlx::migrate!("../eo-services/migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    /// A minimal `HydrationState` over an in-memory pool, its clock frozen so
    /// `default_date()` (`_utc_date_str(clock)`) is deterministic. The
    /// game-data store loads empty (no snapshot dir), which the write surface
    /// never touches.
    async fn write_state() -> crate::hydration::HydrationState {
        use eo_services::clock::MockClock;
        use eo_services::game_data_store::GameDataStore;
        use std::path::Path;
        use std::sync::Arc;
        let pool = memory_pool().await;
        let db = eo_services::db::Db::from_pool(pool);
        let naive =
            chrono::NaiveDateTime::parse_from_str("2026-06-01T12:00:00", "%Y-%m-%dT%H:%M:%S")
                .unwrap();
        crate::hydration::HydrationState::new(
            db,
            Arc::new(GameDataStore::new(Path::new("/nonexistent/snapshot")).unwrap()),
            Arc::new(MockClock::new(Some(naive), 0.0)),
            std::path::PathBuf::from("."),
        )
    }

    /// 2026-06-05T00:00:00Z: heals the rollup watermark past the
    /// backdated days these tests write to, so the write hooks are
    /// observable.
    async fn heal_to_june_fifth(pool: &SqlitePool) {
        eo_services::daily_rollup::heal_rollups(pool, 1_780_617_600.0)
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
        let state = write_state().await;
        heal_to_june_fifth(state.write()).await;

        // A backdated create lands its day's rollup with the insert.
        let (status, body) = body_of(
            state
                .create_ledger_entry("2026-06-02", "expense", "ammo restock", 12.5, "manual")
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            ledger_rollup(state.read(), "2026-06-02", "manual").await,
            Some(("expense".into(), 12.5))
        );

        // The delete relands it empty; a missing id keeps the 404.
        let id = body["id"].as_str().unwrap().to_string();
        let (status, _) = body_of(state.delete_ledger_entry(&id).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            ledger_rollup(state.read(), "2026-06-02", "manual").await,
            None
        );
        let (status, _) = body_of(state.delete_ledger_entry("missing").await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn inventory_sale_relands_the_sold_days_rollup() {
        let state = write_state().await;
        heal_to_june_fifth(state.write()).await;
        sqlx::query(
            "INSERT INTO inventory_items (id, name, tt_value, markup_paid, notes, acquired_at) \
             VALUES ('i1', 'Gun', 10.0, 2.0, NULL, '2026-05-01')",
        )
        .execute(state.write())
        .await
        .unwrap();

        // Sold at a backdated date for an 8.0 markup delta.
        let (status, _) = body_of(
            state
                .sell_inventory_item("i1", 20.0, None, Some("2026-06-02"))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            ledger_rollup(state.read(), "2026-06-02", INVENTORY_SALE_TAG).await,
            Some(("markup".into(), 8.0))
        );
    }

    /// Status + parsed JSON body of a handler response.
    async fn body_of(
        response: axum::http::Response<axum::body::Body>,
    ) -> (axum::http::StatusCode, Value) {
        use http_body_util::BodyExt;
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("collect")
            .to_bytes()
            .to_vec();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[tokio::test]
    async fn empty_overview_emits_the_engine_typed_zeros() {
        let pool = memory_pool().await;
        let value = overview_impl(&Db::from_pool(pool.clone()), 1_800_000_000.0, "all")
            .await
            .unwrap();
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
        let pool = memory_pool().await;
        let value = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
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
        let pool = memory_pool().await;
        seed_scenario(&pool, now).await;
        let v = overview_impl(&Db::from_pool(pool.clone()), now, "all")
            .await
            .unwrap();
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
        let v30 = overview_impl(&Db::from_pool(pool.clone()), now, "30d")
            .await
            .unwrap();
        assert_eq!(v30["returnsBreakdown"]["lootTt"], json!(50.0));
        assert_eq!(v30["returnsBreakdown"]["ledger"]["loot_sale"], json!(12.5));
        assert_eq!(v30["lossesBreakdown"]["ledger"], json!({}));
        assert_eq!(v30["timeline"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn seeded_activity_dominance_and_filters() {
        let now = 1_800_000_000.0;
        let pool = memory_pool().await;
        seed_scenario(&pool, now).await;
        let v = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
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
        let pool = memory_pool().await;
        // keeper: kills, duration, cost all positive.
        seed_filter_session(&pool, "keep", "Keeper", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // zero cost -> cycled 0 -> dropped by the cycled guard alone.
        seed_filter_session(&pool, "zcost", "Zerocost", 1000.0, 1000.0 + 3600.0, 0.0, 2).await;
        // zero duration (start == end) -> dropped by the duration guard alone.
        seed_filter_session(&pool, "zdur", "Zerodur", 1000.0, 1000.0, 5.0, 2).await;
        let v = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
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
        let pool = memory_pool().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(
                overview_impl(&Db::from_pool(pool.clone()), now, "all")
                    .await
                    .unwrap()
            ),
            json!("declining")
        );

        // improving: recent 2.0 above prior 1.0 * 1.02.
        let pool = memory_pool().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(
                overview_impl(&Db::from_pool(pool.clone()), now, "all")
                    .await
                    .unwrap()
            ),
            json!("improving")
        );

        // stable: recent equals prior, inside the band.
        let pool = memory_pool().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 10.0).await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 10.0).await;
        assert_eq!(
            trend(
                overview_impl(&Db::from_pool(pool.clone()), now, "all")
                    .await
                    .unwrap()
            ),
            json!("stable")
        );

        // zero recent rate: the positivity guard short-circuits to stable
        // (a mutated guard would fall through into the banding and declare a
        // direction).
        let pool = memory_pool().await;
        seed_rate(&pool, "p", now - 45.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(
                overview_impl(&Db::from_pool(pool.clone()), now, "all")
                    .await
                    .unwrap()
            ),
            json!("stable")
        );

        // zero prior rate: the other half of the guard.
        let pool = memory_pool().await;
        seed_rate(&pool, "r", now - 10.0 * day, 10.0, 1, 20.0).await;
        assert_eq!(
            trend(
                overview_impl(&Db::from_pool(pool.clone()), now, "all")
                    .await
                    .unwrap()
            ),
            json!("stable")
        );
    }

    /// Dominance needs the top group at or above 60% of known kills, and the
    /// species/maturity presence decides mob vs tag.
    #[tokio::test]
    async fn activity_dominance_threshold_and_tag_split() {
        // Non-dominant: three distinct mobs, one kill each (33% each, below
        // the 0.6 floor) -> no dominant element, no comparison rows.
        let pool = memory_pool().await;
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
        let v = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
        assert_eq!(v["mobComparisons"].as_array().unwrap().len(), 0);
        assert_eq!(v["tagComparisons"].as_array().unwrap().len(), 0);

        // Asymmetric: species present, maturity empty -> still a mob (the
        // presence test is OR, not AND), so it lands in mobComparisons.
        let pool = memory_pool().await;
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
        let v = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
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
        let pool = memory_pool().await;
        // A valid completed session with one dominant-mob kill.
        seed_filter_session(&pool, "ok", "Real", 1000.0, 1000.0 + 3600.0, 5.0, 2).await;
        // An orphan kill whose session_id matches no tracking_sessions row.
        sqlx::query(
            "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,enhancer_cost,loot_total_ped) \
             VALUES('orphan','ghost-session','Ghost','Spec','Young',1.0,0,9.0)",
        )
        .execute(&pool).await.expect("seed");
        // Only the real session's mob is compared; the orphan is ignored.
        let v = activity_impl(&Db::from_pool(pool.clone())).await.unwrap();
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
        let state = write_state().await;
        let (status, body) = body_of(
            state
                .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["date"], json!("2026-05-01"));
        assert_eq!(body["type"], json!("expense"));
        assert_eq!(body["amount"], json!(12.5));
        assert_eq!(body["tag"], json!("ammo"));
        assert!(body["id"].as_str().is_some(), "create generates an id");

        let (status, list) = body_of(state.list_ledger().await).await;
        assert_eq!(status, StatusCode::OK);
        let rows = list.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["description"], json!("Ammo"));
        assert_eq!(rows[0]["id"], body["id"]);
    }

    /// The preset type guard: only 'expense'/'markup' pass; anything else is
    /// a 400 with the reference's detail and writes nothing.
    #[tokio::test]
    async fn preset_create_validates_type() {
        let state = write_state().await;
        for kind in ["expense", "markup"] {
            let (status, _) =
                body_of(state.create_ledger_preset("P", kind, "d", 1.0, "t").await).await;
            assert_eq!(status, StatusCode::OK, "{kind} accepted");
        }
        let (status, body) = body_of(
            state
                .create_ledger_preset("Bad", "income", "d", 1.0, "t")
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["detail"], json!("type must be 'expense' or 'markup'"));
        // Only the two valid presets were written.
        let (_, list) = body_of(state.list_ledger_presets().await).await;
        assert_eq!(list.as_array().unwrap().len(), 2);
    }

    /// Create with the optional fields absent: notes is null and acquired_at
    /// defaults to the (frozen) clock's UTC date.
    #[tokio::test]
    async fn inventory_create_defaults_date_and_notes() {
        let state = write_state().await;
        let (status, body) = body_of(
            state
                .create_inventory_item("Imk2", 50.0, 5.0, None, None)
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        // Response is camelCase even though the request body is snake_case.
        assert_eq!(body["ttValue"], json!(50.0));
        assert_eq!(body["markupPaid"], json!(5.0));
        assert_eq!(body["notes"], Value::Null);
        assert_eq!(body["acquiredAt"], json!("2026-06-01"));

        // An explicit acquired_at / notes are honoured.
        let (_, body) = body_of(
            state
                .create_inventory_item("X", 1.0, 0.0, Some("spare"), Some("2026-01-02"))
                .await,
        )
        .await;
        assert_eq!(body["notes"], json!("spare"));
        assert_eq!(body["acquiredAt"], json!("2026-01-02"));
    }

    /// PATCH field-selection: only PROVIDED (Some) fields update; a None
    /// field is left untouched, exactly as the reference's
    /// `if patch.x is not None`.
    #[tokio::test]
    async fn inventory_patch_updates_only_provided_fields() {
        let state = write_state().await;
        let (_, created) = body_of(
            state
                .create_inventory_item("Orig", 20.0, 3.0, Some("keep"), Some("2026-03-01"))
                .await,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();

        // Provide name + tt_value only: markup_paid and notes stay.
        let (status, patched) = body_of(
            state
                .update_inventory_item(&id, Some("Renamed"), Some(25.0), None, None)
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(patched["name"], json!("Renamed"));
        assert_eq!(patched["ttValue"], json!(25.0));
        assert_eq!(patched["markupPaid"], json!(3.0), "untouched");
        assert_eq!(patched["notes"], json!("keep"), "untouched");

        // An all-None patch re-reads and returns the row unchanged.
        let (status, same) = body_of(
            state
                .update_inventory_item(&id, None, None, None, None)
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(same, patched);

        // Patch a missing id -> 404.
        let (status, body) = body_of(
            state
                .update_inventory_item("no-such", Some("Z"), None, None, None)
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["detail"], json!("Inventory item not found"));
    }

    /// Sell a created item, asserting the delta/type/description-default
    /// branch for profit / loss / zero-delta and the atomic item removal.
    #[tokio::test]
    async fn sell_emits_the_right_delta_branch() {
        // PROFIT: sale 20 over cost 12 -> markup 8.0; default description.
        let state = write_state().await;
        let (_, item) = body_of(
            state
                .create_inventory_item("Sword", 10.0, 2.0, None, Some("2026-02-01"))
                .await,
        )
        .await;
        let id = item["id"].as_str().unwrap().to_string();
        let (status, body) = body_of(
            state
                .sell_inventory_item(&id, 20.0, None, Some("2026-05-10"))
                .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
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
        let (_, inv) = body_of(state.list_inventory().await).await;
        assert_eq!(inv.as_array().unwrap().len(), 0);
        let (_, ledger) = body_of(state.list_ledger().await).await;
        assert_eq!(ledger.as_array().unwrap().len(), 1);

        // LOSS: sale 5 under cost 12 -> expense 7.0; explicit description.
        let state = write_state().await;
        let (_, item) = body_of(
            state
                .create_inventory_item("Shield", 10.0, 2.0, None, Some("2026-02-01"))
                .await,
        )
        .await;
        let id = item["id"].as_str().unwrap().to_string();
        let (_, body) = body_of(
            state
                .sell_inventory_item(&id, 5.0, Some("Dumped it"), None)
                .await,
        )
        .await;
        let entry = &body["ledgerEntry"];
        assert_eq!(entry["type"], json!("expense"));
        assert_eq!(entry["amount"], json!(7.0));
        assert_eq!(entry["description"], json!("Dumped it"));
        // Default sold_at is the frozen clock date.
        assert_eq!(entry["date"], json!("2026-06-01"));

        // ZERO-DELTA: sale == cost -> no ledger entry, item still removed.
        let state = write_state().await;
        let (_, item) = body_of(
            state
                .create_inventory_item("Even", 8.0, 2.0, None, Some("2026-02-01"))
                .await,
        )
        .await;
        let id = item["id"].as_str().unwrap().to_string();
        let (status, body) = body_of(state.sell_inventory_item(&id, 10.0, None, None).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ledgerEntry"], Value::Null);
        assert_eq!(body["soldItem"]["name"], json!("Even"));
        let (_, ledger) = body_of(state.list_ledger().await).await;
        assert_eq!(ledger.as_array().unwrap().len(), 0, "no noise row");
        let (_, inv) = body_of(state.list_inventory().await).await;
        assert_eq!(inv.as_array().unwrap().len(), 0, "item removed");

        // Sell a missing id -> 404.
        let state = write_state().await;
        let (status, body) =
            body_of(state.sell_inventory_item("no-such", 1.0, None, None).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["detail"], json!("Inventory item not found"));
    }

    #[tokio::test]
    async fn ledger_delete_removes_then_reports_missing() {
        let state = write_state().await;
        let (_, created) = body_of(
            state
                .create_ledger_entry("2026-05-01", "expense", "Ammo", 12.5, "ammo")
                .await,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();
        // A successful delete reports "deleted" (the rows_affected == 0 guard
        // is false for an existing row); a second delete hits the 404.
        let (status, body) = body_of(state.delete_ledger_entry(&id).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("deleted"));
        let (status, body) = body_of(state.delete_ledger_entry(&id).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["detail"], json!("Entry not found"));
    }

    #[tokio::test]
    async fn preset_list_shapes_rows_then_delete_removes() {
        let state = write_state().await;
        let (_, created) = body_of(
            state
                .create_ledger_preset("Decay", "expense", "d", 0.5, "decay")
                .await,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();
        // The list shapes the row via preset_item (not an empty default).
        let (status, list) = body_of(state.list_ledger_presets().await).await;
        assert_eq!(status, StatusCode::OK);
        let rows = list.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], json!("Decay"));
        assert_eq!(rows[0]["amount"], json!(0.5));
        assert_eq!(rows[0]["tag"], json!("decay"));
        let (status, body) = body_of(state.delete_ledger_preset(&id).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("deleted"));
        let (status, _) = body_of(state.delete_ledger_preset(&id).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn inventory_delete_removes_then_reports_missing() {
        let state = write_state().await;
        let (_, created) = body_of(
            state
                .create_inventory_item("Sword", 10.0, 2.0, None, None)
                .await,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();
        let (status, body) = body_of(state.delete_inventory_item(&id).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], json!("deleted"));
        let (status, _) = body_of(state.delete_inventory_item(&id).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
