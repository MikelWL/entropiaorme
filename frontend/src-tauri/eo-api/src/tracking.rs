//! The tracking family: the session-read surface (list, detail, tag /
//! manual-mob suggestions, the quest-link suggestion), the live producer
//! surface (start / stop / release-mob / manual-mob-lock / tag-lock / the
//! consolidated dashboard snapshot), the post-hoc session edits (rename /
//! restore mob, loot-item activate / deactivate, armour cost, quest-link
//! decision), and the one-shot repair-cost OCR read.
//!
//! Ported from the HTTP route handlers (`tracking_routes`,
//! `producer_routes`, `hydration`, `scan_routes`) onto typed DTOs over the
//! composed services. The computation is carried verbatim: the SQL, the
//! `serde_json::Value` shaping, the helpers, the constants. The transport's
//! response wrapping (`json_response` / `plain_json_response` /
//! `error_response` / `internal_error` / `detail`, the `EditError` enum) is
//! replaced with typed DTO returns plus [`ApiError`]; each read bridges its
//! computed `Value` into a declared DTO with `serde_json::from_value`, whose
//! serde field order is the wire order, so `to_string` reproduces the
//! HTTP-era body byte-for-byte.
//!
//! Ratified contract movements riding this migration (ADR-0019 lineage):
//!
//! * The **snapshot** and the **quest-link decision** were served
//!   `response_model_exclude_unset` (a field explicitly set to null stayed
//!   on the wire; an unset field was omitted). A single typed struct with
//!   `#[serde(skip_serializing_if = "Option::is_none")]` cannot tell a
//!   present-null from an absent key, so both collapse to "omitted": the
//!   projection narrows from **exclude-unset to exclude-none** (a
//!   present-null field is dropped rather than serialised `null`). The
//!   generated TypeScript type is all-optional and every consumer reads
//!   these fields defensively, so null and absent are equivalent to them.
//! * Structurally-impossible transport legs retire with no consumer: the
//!   `tainted` surrogate-500 (a Rust `String` argument is always valid
//!   UTF-8), the decoded-slash framework 404 (typed args carry the value
//!   directly), the 503 substrate-unavailable floor (every handle is
//!   present by construction), the ETag conditional-GET contract (a typed
//!   command answers a body, not a status + headers), and the body-taint /
//!   int-parse 422s (typed args are pre-validated).

use std::collections::BTreeMap;

use chrono::DateTime;
use eo_services::config_service::{active_trifecta_preset, load_config_readonly, AppConfig};
use eo_services::db::{Db, DbError};
use eo_services::mob_lookup_service::{python_whitespace, MobLookupService};
use eo_services::quests::QuestError;
use eo_services::time::{local_isoformat, naive_to_epoch, to_iso_utc};
use eo_services::tracker::HuntTracker;
use eo_services::trifecta_service::{validate_trifecta, TrifectaPreset};
use eo_wire::normalizer::round_half_even;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Api, ApiError};

// ── Constants ───────────────────────────────────────────────────────

/// The attribute skills the session-detail skill-gain aggregate excludes
/// (`character_calc.ATTRIBUTE_SKILLS`).
const ATTRIBUTE_SKILLS: [&str; 6] = [
    "Agility",
    "Health",
    "Intelligence",
    "Psyche",
    "Stamina",
    "Strength",
];

/// The `TrackingSnapshot` response-model field order (the polymorphic
/// dashboard hydration shape). The snake-case status trio sits among the
/// camelCase headline numbers exactly as the model declares them.
const SNAPSHOT_FIELDS: [&str; 35] = [
    "status",
    "hotbarListenerActive",
    "weaponAttribution",
    "repairOcrEnabled",
    "endOfSessionArmourReminderEnabled",
    "mobEntryMode",
    "currentMob",
    "mobSource",
    "currentTool",
    "trifectaAttribution",
    "recentEvents",
    "session_id",
    "started_at",
    "kill_count",
    "elapsed",
    "cost",
    "returns",
    "pes",
    "net",
    "returnRate",
    "damageDealtTotal",
    "weaponDamageDealt",
    "weaponCost",
    "shotsFiredTotal",
    "criticalHitsTotal",
    "maxDamage",
    "globalsCount",
    "hofsCount",
    "latestKillLoot",
    "multiplierLast",
    "multiplierAvg",
    "multiplierMax",
    "multiplierHistory",
    "cumulativeNetHistory",
    "warnings",
];

/// The repair-scan response-model field order (`exclude_unset`).
const REPAIR_FIELDS: [&str; 4] = ["cost_ped", "raw_text", "confidence", "error"];

// ── Engine-typed numeric primitives ─────────────────────────────────

/// A SQLite numeric read preserving the engine type: a REAL decodes to a
/// float, an INTEGER (the `COALESCE(SUM(...), 0)` empty case) to an integer.
/// The stored value's affinity (`ValueRef`) drives the branch directly,
/// preserving the same behaviour as before: an integer value never
/// masquerades as a float.
fn sql_number(row: &rusqlite::Row, index: usize) -> Value {
    match row.get_ref_unwrap(index) {
        rusqlite::types::ValueRef::Real(value) => json!(value),
        value => json!(value.as_i64().expect("sql_number reads a numeric column")),
    }
}

/// `float(value)` over an engine-typed number.
fn as_f64(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// `round(value, places)`: banker's rounding, always producing a float.
fn round(value: f64, places: usize) -> f64 {
    round_half_even(value, places)
}

/// A model-declared `float` field: coerce an engine-typed integer to its
/// float form, so an integer zero leaves the wire as `0.0`.
fn float_field(value: Value) -> Value {
    match value.as_i64() {
        Some(integer) => json!(integer as f64),
        None => value,
    }
}

/// `datetime.fromtimestamp(ts, tz=UTC).isoformat()` (the session-list
/// start / end times). Emits `+00:00` and 6-digit microseconds only when
/// the fraction is non-zero; `None` maps to JSON null.
fn list_ts_to_iso(ts: Option<f64>) -> Value {
    let Some(ts) = ts else {
        return Value::Null;
    };
    let frac = ts.fract();
    let whole = ts.trunc() as i64;
    let mut micros = round_half_even(frac * 1_000_000.0, 0) as i64;
    let mut secs = whole;
    if micros >= 1_000_000 {
        secs += 1;
        micros -= 1_000_000;
    } else if micros < 0 {
        secs -= 1;
        micros += 1_000_000;
    }
    let dt =
        DateTime::from_timestamp(secs, (micros as u32) * 1_000).expect("timestamp within range");
    let base = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
    if micros == 0 {
        json!(format!("{base}+00:00"))
    } else {
        json!(format!("{base}.{micros:06}+00:00"))
    }
}

/// `_ts_to_iso` (the snapshot's recent-event timestamps): a Unix timestamp
/// to the `+00:00`-suffixed ISO string the domain events stamp, or null.
fn event_ts_to_iso(ts: Option<f64>) -> Value {
    match ts {
        Some(ts) => Value::String(to_iso_utc(ts)),
        None => Value::Null,
    }
}

/// Duration in whole seconds: stored span for an ended session, the clock's
/// running span for an active one, else zero.
fn duration_seconds(
    started_at: Option<f64>,
    ended_at: Option<f64>,
    is_active: bool,
    now: f64,
) -> i64 {
    match (ended_at, started_at) {
        (Some(end), Some(start)) => (end - start) as i64,
        _ if is_active => match started_at {
            Some(start) => (now - start) as i64,
            None => 0,
        },
        _ => 0,
    }
}

/// `_notable_event_category`: the `type` field of a notable event.
fn notable_event_category(event_type: &str) -> &'static str {
    if event_type.starts_with("quest_") {
        "quest"
    } else if event_type.starts_with("hof_") {
        "hof"
    } else {
        "global"
    }
}

// ── Session list ────────────────────────────────────────────────────

/// The session-list columns read from `session_summaries`.
struct ListSummary {
    weapon_cost: f64,
    heal_cost: f64,
    enhancer_cost: f64,
    armour_cost: f64,
    dangling_cost: f64,
    loot_tt: f64,
    primary_mobs: Value,
    primary_weapons: Value,
    globals: i64,
    hofs: i64,
}

pub(crate) async fn list_sessions_impl(db: &Db, now: f64) -> Result<Value, DbError> {
    // Heal so ended sessions carry current summaries (a write on the read
    // path, preserved), then read each recent session's row as one
    // synchronous unit on a reader-core connection.
    db.with_writer(|conn| eo_services::session_summary::heal_summaries(conn))
        .await?;
    db.with_reader(move |conn| list_sessions_read(conn, now))
        .await
}

/// The session-list read proper: the recent-session rows, their summaries,
/// and each row shaped from its summary (ended) or the raw tables. One
/// synchronous pass over a reader-core connection.
fn list_sessions_read(conn: &rusqlite::Connection, now: f64) -> Result<Value, DbError> {
    let meta: Vec<(String, Option<f64>, Option<f64>, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT id, started_at, ended_at, is_active \
             FROM tracking_sessions ORDER BY started_at DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let ids: Vec<String> = meta.iter().map(|(id, ..)| id.clone()).collect();
    let summaries = fetch_list_summaries(conn, &ids)?;

    let mut sessions = Vec::with_capacity(meta.len());
    for (sid, started_at, ended_at, is_active) in &meta {
        let session = match summaries.get(sid) {
            Some(summary) if !*is_active => {
                list_row_from_summary(sid, *started_at, *ended_at, *is_active, now, summary)
            }
            _ => list_row_from_raw(conn, sid, *started_at, *ended_at, *is_active, now)?,
        };
        sessions.push(session);
    }
    Ok(Value::Array(sessions))
}

fn fetch_list_summaries(
    conn: &rusqlite::Connection,
    ids: &[String],
) -> Result<std::collections::HashMap<String, ListSummary>, DbError> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT session_id, weapon_cost, heal_cost, enhancer_cost, armour_cost, dangling_cost, \
         loot_tt, primary_mobs_json, primary_weapons_json, globals, hofs \
         FROM session_summaries WHERE session_id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(ids))?;
    let mut out = std::collections::HashMap::with_capacity(ids.len());
    while let Some(row) = rows.next()? {
        let sid = row.get::<_, String>(0)?;
        out.insert(
            sid,
            ListSummary {
                weapon_cost: as_f64(&sql_number(row, 1)),
                heal_cost: as_f64(&sql_number(row, 2)),
                enhancer_cost: as_f64(&sql_number(row, 3)),
                armour_cost: as_f64(&sql_number(row, 4)),
                dangling_cost: as_f64(&sql_number(row, 5)),
                loot_tt: as_f64(&sql_number(row, 6)),
                primary_mobs: parse_string_array(&row.get::<_, String>(7)?),
                primary_weapons: parse_string_array(&row.get::<_, String>(8)?),
                globals: row.get::<_, i64>(9)?,
                hofs: row.get::<_, i64>(10)?,
            },
        );
    }
    Ok(out)
}

fn parse_string_array(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::Array(Vec::new()))
}

fn list_row_from_summary(
    session_id: &str,
    started_at: Option<f64>,
    ended_at: Option<f64>,
    is_active: bool,
    now: f64,
    summary: &ListSummary,
) -> Value {
    let duration = duration_seconds(started_at, ended_at, is_active, now);
    let cost = summary.weapon_cost
        + summary.heal_cost
        + summary.enhancer_cost
        + summary.armour_cost
        + summary.dangling_cost;
    let returns = summary.loot_tt;
    let net = returns - cost;
    let return_rate = if cost > 0.0 { returns / cost } else { 0.0 };
    json!({
        "id": session_id,
        "startTime": list_ts_to_iso(started_at),
        "endTime": list_ts_to_iso(ended_at),
        "duration": duration,
        "primaryMobs": summary.primary_mobs,
        "primaryWeapons": summary.primary_weapons,
        "cost": round(cost, 2),
        "returns": round(returns, 2),
        "net": round(net, 2),
        "returnRate": round(return_rate, 4),
        "globals": summary.globals,
        "hofs": summary.hofs,
    })
}

#[allow(clippy::too_many_arguments)]
fn list_row_from_raw(
    conn: &rusqlite::Connection,
    session_id: &str,
    started_at: Option<f64>,
    ended_at: Option<f64>,
    is_active: bool,
    now: f64,
) -> Result<Value, DbError> {
    let duration = duration_seconds(started_at, ended_at, is_active, now);

    let weapon_cost = scalar(
        conn,
        "SELECT COALESCE(SUM(ts.cost_per_shot * ts.shots_fired), 0) \
         FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id WHERE k.session_id = ?",
        session_id,
    )?;
    let enhancer_cost = scalar(
        conn,
        "SELECT COALESCE(SUM(k.enhancer_cost), 0) FROM kills k WHERE k.session_id = ?",
        session_id,
    )?;
    let (armour_cost, heal_cost, dangling_cost) = conn.query_row(
        "SELECT COALESCE(armour_cost, 0), COALESCE(heal_cost, 0), COALESCE(dangling_cost, 0) \
         FROM tracking_sessions WHERE id = ?",
        rusqlite::params![session_id],
        |row| {
            Ok((
                as_f64(&sql_number(row, 0)),
                as_f64(&sql_number(row, 1)),
                as_f64(&sql_number(row, 2)),
            ))
        },
    )?;
    let weapon_cost = as_f64(&weapon_cost);
    let enhancer_cost = as_f64(&enhancer_cost);
    let cost = weapon_cost + heal_cost + enhancer_cost + armour_cost + dangling_cost;

    let returns = as_f64(&scalar(
        conn,
        "SELECT COALESCE(SUM(loot_total_ped), 0) FROM kills WHERE session_id = ?",
        session_id,
    )?);

    let primary_mobs = string_column(
        conn,
        "SELECT mob_name FROM kills \
         WHERE session_id = ? AND mob_name IS NOT NULL AND mob_name != 'Unknown' \
         GROUP BY mob_name ORDER BY COUNT(*) DESC LIMIT 3",
        session_id,
    )?;
    let primary_weapons = string_column(
        conn,
        "SELECT ts.tool_name FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id \
         WHERE k.session_id = ? AND ts.tool_name IS NOT NULL AND ts.tool_name != 'Unknown' \
         GROUP BY ts.tool_name ORDER BY SUM(ts.shots_fired) DESC LIMIT 3",
        session_id,
    )?;

    let net = returns - cost;
    let return_rate = if cost > 0.0 { returns / cost } else { 0.0 };

    let (globals, hofs) = conn.query_row(
        "SELECT \
           COALESCE(SUM(CASE WHEN event_type LIKE 'global_%' THEN 1 ELSE 0 END), 0), \
           COALESCE(SUM(CASE WHEN event_type LIKE 'hof_%' THEN 1 ELSE 0 END), 0) \
         FROM notable_events WHERE session_id = ?",
        rusqlite::params![session_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;

    Ok(json!({
        "id": session_id,
        "startTime": list_ts_to_iso(started_at),
        "endTime": list_ts_to_iso(ended_at),
        "duration": duration,
        "primaryMobs": primary_mobs,
        "primaryWeapons": primary_weapons,
        "cost": round(cost, 2),
        "returns": round(returns, 2),
        "net": round(net, 2),
        "returnRate": round(return_rate, 4),
        "globals": globals,
        "hofs": hofs,
    }))
}

fn scalar(conn: &rusqlite::Connection, sql: &str, sid: &str) -> Result<Value, DbError> {
    Ok(conn.query_row(sql, rusqlite::params![sid], |row| Ok(sql_number(row, 0)))?)
}

fn string_column(
    conn: &rusqlite::Connection,
    sql: &str,
    sid: &str,
) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![sid], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ── Session detail ──────────────────────────────────────────────────

pub(crate) async fn get_session_impl(
    db: &Db,
    session_id: &str,
    now: f64,
) -> Result<Option<Value>, DbError> {
    let sid = session_id.to_string();
    db.with_reader(move |conn| get_session_read(conn, &sid, now))
        .await
}

/// One session's full detail, read in one synchronous pass over a reader-core
/// connection; an absent session is `None`.
fn get_session_read(
    conn: &rusqlite::Connection,
    session_id: &str,
    now: f64,
) -> Result<Option<Value>, DbError> {
    use rusqlite::OptionalExtension as _;

    let session_meta = conn
        .query_row(
            "SELECT id, started_at, ended_at, is_active, mob_tracking_mode \
             FROM tracking_sessions WHERE id = ?",
            rusqlite::params![session_id],
            |row| {
                Ok((
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((started_at, ended_at, is_active, mob_mode)) = session_meta else {
        return Ok(None);
    };

    let mob_entry_mode = mob_mode
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "mob".to_string());

    let duration = duration_seconds(started_at, ended_at, is_active, now);

    let (armour_cost, session_heal_cost, dangling_cost) = conn.query_row(
        "SELECT COALESCE(armour_cost, 0), COALESCE(heal_cost, 0), COALESCE(dangling_cost, 0) \
         FROM tracking_sessions WHERE id = ?",
        rusqlite::params![session_id],
        |row| {
            Ok((
                as_f64(&sql_number(row, 0)),
                as_f64(&sql_number(row, 1)),
                as_f64(&sql_number(row, 2)),
            ))
        },
    )?;

    let (kills, total_returns, total_enhancer_cost) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(loot_total_ped), 0), COALESCE(SUM(enhancer_cost), 0) \
         FROM kills WHERE session_id = ?",
        rusqlite::params![session_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                as_f64(&sql_number(row, 1)),
                as_f64(&sql_number(row, 2)),
            ))
        },
    )?;

    let merged_tools: Vec<(String, i64, f64, i64, f64)> = {
        let mut stmt = conn.prepare(
            "SELECT t.tool_name, SUM(t.shots_fired), SUM(t.damage_dealt), SUM(t.critical_hits), \
             SUM(COALESCE(t.cost_per_shot, 0) * COALESCE(t.shots_fired, 0)) \
             FROM kill_tool_stats t JOIN kills k ON k.id = t.kill_id \
             WHERE k.session_id = ? GROUP BY t.tool_name",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1).unwrap_or(0),
                as_f64(&sql_number(row, 2)),
                row.get::<_, i64>(3).unwrap_or(0),
                as_f64(&sql_number(row, 4)),
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let weapon_cost: f64 = merged_tools
        .iter()
        .map(|(_, _, _, _, cost_attr)| cost_attr)
        .sum();

    let merged_loot = loot_agg(conn, session_id, "l.deactivated_at IS NULL")?;
    let merged_deactivated_loot = loot_agg(conn, session_id, "l.deactivated_at IS NOT NULL")?;

    let mob_breakdown: Vec<Value> = {
        let mut stmt = conn.prepare(
            "SELECT mob_name, original_mob_name, COUNT(*) FROM kills \
             WHERE session_id = ? AND mob_name IS NOT NULL \
             GROUP BY mob_name, original_mob_name ORDER BY COUNT(*) DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            Ok(json!({
                "currentName": row.get::<_, String>(0)?,
                "originalName": row.get::<_, Option<String>>(1)?,
                "killCount": row.get::<_, i64>(2)?,
            }))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let total_cost =
        weapon_cost + session_heal_cost + total_enhancer_cost + armour_cost + dangling_cost;

    let detail_skill_tt = as_f64(&scalar(
        conn,
        "SELECT COALESCE(SUM(ped_value), 0) FROM skill_gains WHERE session_id = ?",
        session_id,
    )?);

    let net = total_returns - total_cost;
    let return_rate = if total_cost > 0.0 {
        total_returns / total_cost
    } else {
        0.0
    };

    let loot_breakdown = loot_breakdown_sorted(&merged_loot);
    let deactivated_loot_breakdown = loot_breakdown_sorted(&merged_deactivated_loot);

    let mut tool_stats: Vec<(i64, Value)> = merged_tools
        .iter()
        .map(|(name, shots, dmg, crits, cost_attr)| {
            (
                *shots,
                json!({
                    "weaponName": name,
                    "shotsFired": shots,
                    "damageDealt": float_field(json!(dmg)),
                    "crits": crits,
                    "costAttributed": round(*cost_attr, 2),
                }),
            )
        })
        .collect();
    stable_sort_desc_by_key(&mut tool_stats);
    let tool_stats: Vec<Value> = tool_stats.into_iter().map(|(_, v)| v).collect();

    let notable_events: Vec<Value> = {
        let mut stmt = conn.prepare(
            "SELECT event_type, mob_or_item, value_ped FROM notable_events \
             WHERE session_id = ? ORDER BY timestamp",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id], |row| {
            let event_type = row.get::<_, String>(0)?;
            let mob_or_item = row.get::<_, Option<String>>(1)?;
            let value = sql_number(row, 2);
            Ok(json!({
                "type": notable_event_category(&event_type),
                "eventType": event_type,
                "target": mob_or_item,
                "item": mob_or_item,
                "value": float_field(value),
            }))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let skill_gains = session_skill_gains(conn, session_id)?;

    Ok(Some(json!({
        "sessionId": session_id,
        "summary": {
            "cost": round(total_cost, 2),
            "returns": round(total_returns, 2),
            "pes": round(detail_skill_tt, 2),
            "net": round(net, 2),
            "returnRate": round(return_rate, 4),
            "kills": kills,
            "duration": duration,
            "costBreakdown": {
                "weaponCost": round(weapon_cost, 2),
                "healCost": round(session_heal_cost, 2),
                "enhancerCost": round(total_enhancer_cost, 2),
                "armourCost": round(armour_cost, 2),
            },
        },
        "mobEntryMode": mob_entry_mode,
        "notableEvents": notable_events,
        "lootBreakdown": loot_breakdown,
        "deactivatedLootBreakdown": deactivated_loot_breakdown,
        "mobBreakdown": mob_breakdown,
        "effectiveLoot": round(total_returns, 2),
        "toolStats": tool_stats,
        "skillGains": skill_gains,
    })))
}

fn loot_agg(
    conn: &rusqlite::Connection,
    session_id: &str,
    deactivated_clause: &str,
) -> Result<Vec<(String, i64, f64)>, DbError> {
    let sql = format!(
        "SELECT l.item_name, SUM(l.quantity), SUM(l.value_ped) \
         FROM kill_loot_items l JOIN kills k ON k.id = l.kill_id \
         WHERE k.session_id = ? AND COALESCE(l.is_enhancer_shrapnel, 0) = 0 AND {deactivated_clause} \
         GROUP BY l.item_name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1).unwrap_or(0),
            as_f64(&sql_number(row, 2)),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn loot_breakdown_sorted(rows: &[(String, i64, f64)]) -> Vec<Value> {
    let mut entries: Vec<(f64, Value)> = rows
        .iter()
        .map(|(name, qty, val)| {
            let tt = round(*val, 2);
            (
                tt,
                json!({
                    "name": name,
                    "quantity": qty,
                    "ttValue": float_field(json!(tt)),
                }),
            )
        })
        .collect();
    stable_sort_desc_by_f64(&mut entries);
    entries.into_iter().map(|(_, v)| v).collect()
}

fn session_skill_gains(conn: &rusqlite::Connection, session_id: &str) -> Result<Value, DbError> {
    let attr_placeholders = vec!["?"; ATTRIBUTE_SKILLS.len()].join(",");
    let sql = format!(
        "SELECT sg.skill_name, SUM(sg.amount) as total_amount, \
         COALESCE(SUM(sg.ped_value), 0) as total_ped \
         FROM skill_gains sg WHERE sg.session_id = ? \
         AND sg.skill_name NOT IN ({attr_placeholders}) \
         GROUP BY sg.skill_name ORDER BY total_ped DESC"
    );
    // Each row's skill name (index 0) and engine-typed total_ped (index 2);
    // total_amount (index 1) is unread, kept in the projection for SQL parity.
    let rows: Vec<(String, Value)> = {
        let mut params: Vec<&str> = Vec::with_capacity(1 + ATTRIBUTE_SKILLS.len());
        params.push(session_id);
        params.extend(ATTRIBUTE_SKILLS);
        let mut stmt = conn.prepare(&sql)?;
        let mapped = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            Ok((row.get::<_, String>(0)?, sql_number(row, 2)))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if rows.is_empty() {
        return Ok(json!([]));
    }

    let skill_names: Vec<String> = rows.iter().map(|(name, _)| name.clone()).collect();
    let placeholders = vec!["?"; skill_names.len()].join(",");
    let cal_sql = format!(
        "SELECT skill_name, level FROM skill_calibrations WHERE id IN ( \
         SELECT MAX(id) FROM skill_calibrations WHERE skill_name IN ({placeholders}) \
         GROUP BY skill_name)"
    );
    let mut levels: BTreeMap<String, f64> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(&cal_sql)?;
        let mut cal_rows = stmt.query(rusqlite::params_from_iter(&skill_names))?;
        while let Some(row) = cal_rows.next()? {
            levels.insert(row.get::<_, String>(0)?, as_f64(&sql_number(row, 1)));
        }
    }

    let gains: Vec<Value> = rows
        .iter()
        .map(|(name, total_ped)| {
            let level = match levels.get(name) {
                Some(level) => float_field(json!(round(*level, 1))),
                None => json!(0.0),
            };
            let tt = as_f64(total_ped);
            json!({
                "skillName": name,
                "level": level,
                "ttValueGained": float_field(json!(round(tt, 4))),
            })
        })
        .collect();
    Ok(Value::Array(gains))
}

fn stable_sort_desc_by_key(entries: &mut [(i64, Value)]) {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
}

fn stable_sort_desc_by_f64(entries: &mut [(f64, Value)]) {
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
}

// ── Tag suggestions ─────────────────────────────────────────────────

async fn tag_suggestions_impl(db: &Db, q: &str, limit: i64) -> Result<Vec<String>, DbError> {
    let query = q.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let bounded = limit.clamp(1, 20);
    let like = format!("%{}%", query.to_lowercase());
    db.with_reader(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT mob_name, COUNT(*) as uses FROM kills \
             WHERE mob_name IS NOT NULL AND mob_name != 'Unknown' \
             AND COALESCE(mob_species, '') = '' AND COALESCE(mob_maturity, '') = '' \
             AND lower(mob_name) LIKE ? \
             GROUP BY mob_name ORDER BY uses DESC, mob_name ASC LIMIT ?",
        )?;
        let rows = stmt.query_map(rusqlite::params![like, bounded], |row| {
            row.get::<_, String>(0)
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
    .await
}

// ── Session edits ───────────────────────────────────────────────────
//
// Post-hoc edits to ENDED sessions, byte-faithful to the original. The
// four mob/loot edits share the active-session guard (404 absent, 409
// still active); `armour-cost` deliberately omits it. The transport's
// `EditError` maps onto [`ApiError`]: an `Internal` (a `DbError` at the
// boundary) is logged server-side and collapses to the fixed reply.

#[derive(Debug)]
enum EditError {
    NotFound(String),
    Conflict(String),
    BadRequest(String),
    Internal,
}

impl From<DbError> for EditError {
    fn from(_: DbError) -> Self {
        EditError::Internal
    }
}

fn edit_error(context: &'static str) -> impl Fn(EditError) -> ApiError {
    move |error| match error {
        EditError::NotFound(message) => ApiError::not_found(message),
        EditError::Conflict(message) => ApiError::conflict(message),
        EditError::BadRequest(message) => ApiError::bad_request(message),
        EditError::Internal => ApiError::invalid_state(format!("{context} failed")),
    }
}

async fn validate_session_exists(db: &Db, session_id: &str) -> Result<(), EditError> {
    let sid = session_id.to_string();
    let is_active = db
        .with_reader(move |conn| {
            use rusqlite::OptionalExtension as _;
            Ok(conn
                .query_row(
                    "SELECT id, is_active FROM tracking_sessions WHERE id = ?",
                    rusqlite::params![sid],
                    |row| row.get::<_, i64>(1),
                )
                .optional()?)
        })
        .await?;
    let Some(is_active) = is_active else {
        return Err(EditError::NotFound("Session not found".to_string()));
    };
    if is_active != 0 {
        return Err(EditError::Conflict(
            "Session mob edits are only available after the session has ended".to_string(),
        ));
    }
    Ok(())
}

async fn build_mob_edit_response(
    db: &Db,
    session_id: &str,
    mob_name: &str,
) -> Result<Value, EditError> {
    let sid = session_id.to_string();
    let mob = mob_name.to_string();
    let kill_count = db
        .with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM kills WHERE session_id = ? AND mob_name = ?",
                rusqlite::params![sid, mob],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await?;
    Ok(json!({
        "sessionId": session_id,
        "mobName": mob_name,
        "killCount": kill_count,
    }))
}

async fn build_loot_item_edit_response(
    db: &Db,
    session_id: &str,
    item_name: &str,
    affected_rows: i64,
    total_value_delta: f64,
) -> Result<Value, EditError> {
    let sid = session_id.to_string();
    let session_returns = db
        .with_reader(move |conn| {
            Ok(as_f64(&conn.query_row(
                "SELECT COALESCE(SUM(loot_total_ped), 0) FROM kills WHERE session_id = ?",
                rusqlite::params![sid],
                |row| Ok(sql_number(row, 0)),
            )?))
        })
        .await?;
    Ok(json!({
        "sessionId": session_id,
        "itemName": item_name,
        "affectedRows": affected_rows,
        "totalValueDelta": round(total_value_delta, 4),
        "sessionTotalReturns": round(session_returns, 2),
    }))
}

async fn rename_session_mob_impl(
    db: &Db,
    session_id: &str,
    from_mob: &str,
    to_mob: &str,
) -> Result<Value, EditError> {
    validate_session_exists(db, session_id).await?;
    let from_mob = from_mob.trim();
    let to_mob = to_mob.trim();
    if from_mob.is_empty() || to_mob.is_empty() {
        return Err(EditError::BadRequest(
            "Mob names cannot be blank".to_string(),
        ));
    }
    if from_mob == to_mob {
        return Err(EditError::Conflict(
            "rename target matches the current value (no-op)".to_string(),
        ));
    }

    // The domain outcome of the writer-core transaction: the "nothing matched"
    // branch the original raised as a conflict, or the completed rename.
    enum RenameOutcome {
        NoMatch,
        Renamed,
    }

    let sid = session_id.to_string();
    let from = from_mob.to_string();
    let to = to_mob.to_string();
    let outcome = db
        .with_writer(move |conn| {
            let tx = conn.transaction()?;
            let preserve = tx.execute(
                "UPDATE kills \
                 SET original_mob_name = COALESCE(original_mob_name, mob_name) \
                 WHERE session_id = ? AND mob_name = ?",
                rusqlite::params![sid, from],
            )?;
            if preserve == 0 {
                return Ok(RenameOutcome::NoMatch);
            }
            tx.execute(
                "UPDATE kills \
                 SET mob_name = ?, \
                     original_mob_name = CASE \
                         WHEN original_mob_name = ? THEN NULL \
                         ELSE original_mob_name \
                     END \
                 WHERE session_id = ? AND mob_name = ?",
                rusqlite::params![to, to, sid, from],
            )?;
            tx.execute(
                "DELETE FROM session_summaries WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.commit()?;
            Ok(RenameOutcome::Renamed)
        })
        .await?;

    match outcome {
        RenameOutcome::NoMatch => Err(EditError::Conflict(format!(
            "No kills in this session match mob_name='{from_mob}'"
        ))),
        RenameOutcome::Renamed => build_mob_edit_response(db, session_id, to_mob).await,
    }
}

async fn restore_session_mob_impl(
    db: &Db,
    session_id: &str,
    current_mob: &str,
) -> Result<Value, EditError> {
    validate_session_exists(db, session_id).await?;
    let current_mob = current_mob.trim();
    if current_mob.is_empty() {
        return Err(EditError::BadRequest(
            "Mob name cannot be blank".to_string(),
        ));
    }

    // The domain outcome of the writer-core transaction: the two conflict
    // branches the original raised as edit errors, or the restored prior name.
    enum RestoreOutcome {
        NoMatch,
        Ambiguous(usize),
        Restored(String),
    }

    let sid = session_id.to_string();
    let current = current_mob.to_string();
    let outcome = db
        .with_writer(move |conn| {
            let tx = conn.transaction()?;
            let restored_rows: Vec<String> = {
                let mut stmt = tx.prepare(
                    "UPDATE kills \
                     SET mob_name = original_mob_name, original_mob_name = NULL \
                     WHERE session_id = ? AND mob_name = ? AND original_mob_name IS NOT NULL \
                     RETURNING mob_name",
                )?;
                let rows = stmt.query_map(rusqlite::params![sid, current], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            if restored_rows.is_empty() {
                return Ok(RestoreOutcome::NoMatch);
            }

            let mut distinct: Vec<String> = Vec::new();
            for original in &restored_rows {
                if !distinct.contains(original) {
                    distinct.push(original.clone());
                }
            }
            if distinct.len() > 1 {
                return Ok(RestoreOutcome::Ambiguous(distinct.len()));
            }
            let restored_to = distinct.into_iter().next().expect("one distinct original");

            tx.execute(
                "DELETE FROM session_summaries WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.commit()?;
            Ok(RestoreOutcome::Restored(restored_to))
        })
        .await?;

    match outcome {
        RestoreOutcome::NoMatch => Err(EditError::Conflict(format!(
            "No restorable kills in this session for mob_name='{current_mob}' \
             (either no rename has happened or the preservation column is empty)"
        ))),
        RestoreOutcome::Ambiguous(count) => Err(EditError::Conflict(format!(
            "Ambiguous restore for mob_name='{current_mob}': {count} distinct prior names merged into it."
        ))),
        RestoreOutcome::Restored(restored_to) => {
            build_mob_edit_response(db, session_id, &restored_to).await
        }
    }
}

async fn bulk_flip_loot_item(
    db: &Db,
    session_id: &str,
    item_name: &str,
    to_state: &str,
) -> Result<Value, EditError> {
    validate_session_exists(db, session_id).await?;
    let item_name = item_name.trim();
    if item_name.is_empty() {
        return Err(EditError::BadRequest(
            "Item name cannot be blank".to_string(),
        ));
    }

    let (opposite_clause, new_flag_sql, delta_sign) = match to_state {
        "deactivated" => ("l.deactivated_at IS NULL", "unixepoch('now')", -1.0_f64),
        "active" => ("l.deactivated_at IS NOT NULL", "NULL", 1.0_f64),
        other => panic!("unsupported to_state: {other:?}"),
    };

    // The domain outcome of the writer-core transaction: the two "nothing
    // flipped" branches the original raised as edit errors, or the flip's
    // affected-row count and net delta for the response.
    enum FlipOutcome {
        NoLoot,
        AllAlready,
        Flipped { affected: i64, total_delta: f64 },
    }

    let flip_sql = format!(
        "UPDATE kill_loot_items \
         SET deactivated_at = {new_flag_sql} \
         WHERE id IN ( \
             SELECT l.id \
             FROM kill_loot_items l \
             JOIN kills k ON k.id = l.kill_id \
             WHERE k.session_id = ? AND l.item_name = ? AND {opposite_clause} \
         ) \
         RETURNING kill_id, value_ped"
    );
    let sid = session_id.to_string();
    let item = item_name.to_string();
    let outcome = db
        .with_writer(move |conn| {
            use rusqlite::OptionalExtension as _;
            let tx = conn.transaction()?;
            let flipped: Vec<(String, f64)> = {
                let mut stmt = tx.prepare(&flip_sql)?;
                let rows = stmt.query_map(rusqlite::params![sid, item], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            if flipped.is_empty() {
                let any_row: Option<i64> = tx
                    .query_row(
                        "SELECT 1 FROM kill_loot_items l \
                         JOIN kills k ON k.id = l.kill_id \
                         WHERE k.session_id = ? AND l.item_name = ? \
                         LIMIT 1",
                        rusqlite::params![sid, item],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?;
                if any_row.is_none() {
                    return Ok(FlipOutcome::NoLoot);
                }
                return Ok(FlipOutcome::AllAlready);
            }

            let mut order: Vec<String> = Vec::new();
            let mut per_kill: BTreeMap<String, f64> = BTreeMap::new();
            let mut total_delta = 0.0;
            for (kill_id, value) in &flipped {
                if !per_kill.contains_key(kill_id) {
                    order.push(kill_id.clone());
                }
                *per_kill.entry(kill_id.clone()).or_insert(0.0) += value;
                total_delta += value;
            }
            for kill_id in &order {
                let kill_delta = per_kill[kill_id];
                tx.execute(
                    "UPDATE kills SET loot_total_ped = loot_total_ped + ? WHERE id = ?",
                    rusqlite::params![delta_sign * kill_delta, kill_id],
                )?;
            }
            tx.execute(
                "DELETE FROM session_summaries WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            eo_services::daily_rollup::refresh_session_days(&tx, &sid)?;
            tx.commit()?;

            Ok(FlipOutcome::Flipped {
                affected: flipped.len() as i64,
                total_delta: delta_sign * total_delta,
            })
        })
        .await?;

    match outcome {
        FlipOutcome::NoLoot => Err(EditError::NotFound(format!(
            "No loot named '{item_name}' in this session"
        ))),
        FlipOutcome::AllAlready => Err(EditError::Conflict(format!(
            "All '{item_name}' rows in this session are already {to_state}"
        ))),
        FlipOutcome::Flipped {
            affected,
            total_delta,
        } => build_loot_item_edit_response(db, session_id, item_name, affected, total_delta).await,
    }
}

async fn set_armour_cost_impl(db: &Db, session_id: &str, cost: f64) -> Result<Value, EditError> {
    // The existence check, the update, and its projection hooks run together on
    // the writer core connection: the read sees the not-yet-committed guard row
    // and the write commits atomically with the rollup and summary refresh.
    let sid = session_id.to_string();
    let found = db
        .with_writer(move |conn| {
            use rusqlite::OptionalExtension as _;
            let started_at: Option<f64> = conn
                .query_row(
                    "SELECT started_at FROM tracking_sessions WHERE id = ?",
                    rusqlite::params![sid],
                    |row| row.get::<_, f64>(0),
                )
                .optional()?;
            let Some(started_at) = started_at else {
                return Ok(false);
            };
            let tx = conn.transaction()?;
            tx.execute(
                "UPDATE tracking_sessions SET armour_cost = COALESCE(armour_cost, 0) + ? \
                 WHERE id = ?",
                rusqlite::params![cost, sid],
            )?;
            eo_services::daily_rollup::refresh_days(
                &tx,
                [eo_services::daily_rollup::epoch_day(started_at)],
            )?;
            eo_services::session_summary::write_session_summary(&tx, &sid)?;
            tx.commit()?;
            Ok(true)
        })
        .await?;
    if !found {
        return Err(EditError::NotFound("Session not found".to_string()));
    }
    Ok(json!({
        "sessionId": session_id,
        "armourCost": round(cost, 2),
    }))
}

/// Delete a tracking session and every row that references it. An active
/// session cannot be deleted (its writes are still in flight); a missing one
/// is a not-found. The cascade runs in one transaction, child-scoped rows
/// first, and repairs the daily rollups for exactly the calendar days the
/// session touched (captured before the deletes empty those tables).
async fn delete_session_impl(db: &Db, session_id: &str) -> Result<(), EditError> {
    // The domain outcome of the writer-core transaction: the guard branches the
    // original raised as edit errors, or the completed cascade.
    enum DeleteOutcome {
        NotFound,
        Active,
        Deleted,
    }

    let sid = session_id.to_string();
    let outcome = db
        .with_writer(move |conn| {
            use rusqlite::OptionalExtension as _;
            let tx = conn.transaction()?;

            // Read the existence-and-active guard inside the transaction, so the
            // check and the cascade are one atomic unit (a separate acquisition
            // for the check could interleave with another write between the read
            // and the delete).
            let is_active: Option<i64> = tx
                .query_row(
                    "SELECT is_active FROM tracking_sessions WHERE id = ?",
                    rusqlite::params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(is_active) = is_active else {
                return Ok(DeleteOutcome::NotFound);
            };
            if is_active != 0 {
                return Ok(DeleteOutcome::Active);
            }

            // Capture the days this session touches before its rows are deleted,
            // so the rollup repair below recomputes exactly those days.
            let days: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT DISTINCT date(timestamp, 'unixepoch') FROM kills WHERE session_id = ? \
                     UNION SELECT DISTINCT date(timestamp, 'unixepoch') FROM skill_gains \
                           WHERE session_id = ? \
                     UNION SELECT date(started_at, 'unixepoch') FROM tracking_sessions WHERE id = ? \
                     UNION SELECT date(ended_at, 'unixepoch') FROM tracking_sessions \
                           WHERE id = ? AND ended_at IS NOT NULL",
                )?;
                let rows = stmt.query_map(
                    rusqlite::params![sid, sid, sid, sid],
                    |row| row.get::<_, String>(0),
                )?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            // Child-scoped rows first (the kill-scoped tables have no ON DELETE
            // cascade), then the kills, the session-scoped rows, and the session.
            tx.execute(
                "DELETE FROM kill_tool_stats \
                 WHERE kill_id IN (SELECT id FROM kills WHERE session_id = ?)",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM kill_loot_items \
                 WHERE kill_id IN (SELECT id FROM kills WHERE session_id = ?)",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM kills WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM skill_gains WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM notable_events WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM session_summaries WHERE session_id = ?",
                rusqlite::params![sid],
            )?;
            tx.execute(
                "DELETE FROM tracking_sessions WHERE id = ?",
                rusqlite::params![sid],
            )?;

            eo_services::daily_rollup::refresh_days(&tx, days)?;
            tx.commit()?;
            Ok(DeleteOutcome::Deleted)
        })
        .await?;
    match outcome {
        DeleteOutcome::NotFound => Err(EditError::NotFound("Session not found".to_string())),
        DeleteOutcome::Active => Err(EditError::Conflict(
            "Cannot delete an active session".to_string(),
        )),
        DeleteOutcome::Deleted => Ok(()),
    }
}

// ── Producer helpers ────────────────────────────────────────────────

/// `_validate_hotbar`: hotbar attribution is workable while at least one
/// slot is bound (a non-null library id).
fn validate_hotbar(config: &AppConfig) -> (bool, Option<String>) {
    let any_bound = config
        .hotbar
        .values()
        .any(|library_id| !library_id.is_null());
    if any_bound {
        (true, None)
    } else {
        (
            false,
            Some(
                "Bind at least one hotbar slot in the Equipment page before tracking.".to_string(),
            ),
        )
    }
}

/// `_configured_manual_label`: the idle-state mob label and its source.
fn configured_manual_label(config: &AppConfig) -> (Value, Value) {
    if config.mob_tracking_mode == "tag" {
        let tag = config.mob_tracking_tag.trim();
        if tag.is_empty() {
            return (Value::Null, Value::Null);
        }
        return (
            Value::String(tag.to_string()),
            Value::String("tag".to_string()),
        );
    }
    let species = config.manual_mob_species.trim();
    let maturity = config.manual_mob_maturity.trim();
    if species.is_empty() {
        return (Value::Null, Value::Null);
    }
    let display = if maturity.is_empty() {
        species.to_string()
    } else {
        format!("{maturity} {species}")
    };
    (Value::String(display), Value::String("manual".to_string()))
}

/// `_notable_event_label`: the curated label for known event types, else
/// the category title-cased.
fn notable_event_label(event_type: &str) -> String {
    match event_type {
        "global_kill" => "Global Kill".to_string(),
        "global_item" => "Global Item".to_string(),
        "hof_kill" => "HoF Kill".to_string(),
        "hof_item" => "HoF Item".to_string(),
        "quest_started" => "Quest Started".to_string(),
        "quest_completed" => "Quest Completed".to_string(),
        _ => {
            let category = notable_event_category(event_type);
            if category == "hof" {
                "HoF".to_string()
            } else {
                capitalize(category)
            }
        }
    }
}

/// `_notable_event_description`: the label with the mob or item, and the
/// value in PED for everything but the quest events.
fn notable_event_description(event_type: &str, mob_or_item: &str, value_ped: f64) -> String {
    let label = notable_event_label(event_type);
    if event_type.starts_with("quest_") {
        format!("{label}: {mob_or_item}")
    } else {
        format!("{label}: {mob_or_item} ({value_ped:.2} PED)")
    }
}

/// Python `str.capitalize` over an ASCII category.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The config update that clears the free-text session tag.
fn clear_tag() -> Map<String, Value> {
    let mut updates = Map::new();
    updates.insert("mob_tracking_tag".into(), json!(""));
    updates
}

/// The config update that clears the stored manual-mob selection.
fn clear_manual_mob() -> Map<String, Value> {
    let mut updates = Map::new();
    updates.insert("manual_mob_species".into(), json!(""));
    updates.insert("manual_mob_maturity".into(), json!(""));
    updates
}

/// The released-mob display value.
fn mob_display(species: &str, maturity: &str) -> Value {
    if species.is_empty() {
        Value::Null
    } else if maturity.is_empty() {
        Value::String(species.to_string())
    } else {
        Value::String(format!("{maturity} {species}"))
    }
}

/// Project a service value into a response model's field order, emitting
/// only the keys present (Pydantic's `exclude_unset`).
fn project(value: &Value, order: &[&str]) -> Value {
    let mut out = Map::new();
    if let Some(object) = value.as_object() {
        for &field in order {
            if let Some(found) = object.get(field) {
                out.insert(field.to_string(), found.clone());
            }
        }
    }
    Value::Object(out)
}

// ── Quest-link formatters ───────────────────────────────────────────

/// `str(value)` as the router applies it to ids.
fn python_str_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

/// `str(id) if id is not None else None` over a suggestion's nullable id.
fn str_id_or_null(value: &Value) -> Value {
    if value.is_null() {
        Value::Null
    } else {
        json!(python_str_of(value))
    }
}

/// `str(...) or None` as an owned `Option<String>` (for the typed DTOs).
fn opt_str(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

/// The quest-link suggestion wire shape (`get_session_quest_link_suggestion`).
fn format_quest_link_suggestion(session_id: &str, suggestion: &Value) -> Value {
    json!({
        "sessionId": session_id,
        "suggestionType": suggestion["suggestion_type"],
        "reason": suggestion["reason"],
        "questId": str_id_or_null(&suggestion["quest_id"]),
        "questName": suggestion["quest_name"],
        "playlistId": str_id_or_null(&suggestion["playlist_id"]),
        "playlistName": suggestion["playlist_name"],
    })
}

// ── Response DTOs ───────────────────────────────────────────────────

/// One row of the session list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackingSession {
    pub id: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub duration: i64,
    pub primary_mobs: Vec<String>,
    pub primary_weapons: Vec<String>,
    pub cost: f64,
    pub returns: f64,
    pub net: f64,
    pub return_rate: f64,
    pub globals: i64,
    pub hofs: i64,
}

/// The session-detail cost split.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    pub weapon_cost: f64,
    pub heal_cost: f64,
    pub enhancer_cost: f64,
    pub armour_cost: f64,
}

/// The session-detail headline summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub cost: f64,
    pub returns: f64,
    pub pes: f64,
    pub net: f64,
    pub return_rate: f64,
    pub kills: i64,
    pub duration: i64,
    pub cost_breakdown: CostBreakdown,
}

/// One notable event in a session detail.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotableEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub event_type: String,
    pub target: Option<String>,
    pub item: Option<String>,
    pub value: f64,
}

/// One aggregated loot line.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LootEntry {
    pub name: String,
    pub quantity: i64,
    pub tt_value: f64,
}

/// One per-mob breakdown row.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MobBreakdownRow {
    pub current_name: String,
    pub original_name: Option<String>,
    pub kill_count: i64,
}

/// One per-tool aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub weapon_name: String,
    pub shots_fired: i64,
    pub damage_dealt: f64,
    pub crits: i64,
    pub cost_attributed: f64,
}

/// One per-skill gain (attributes excluded).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillGain {
    pub skill_name: String,
    pub level: f64,
    pub tt_value_gained: f64,
}

/// The full session detail.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session_id: String,
    pub summary: SessionSummary,
    pub mob_entry_mode: String,
    pub notable_events: Vec<NotableEvent>,
    pub loot_breakdown: Vec<LootEntry>,
    pub deactivated_loot_breakdown: Vec<LootEntry>,
    pub mob_breakdown: Vec<MobBreakdownRow>,
    pub effective_loot: f64,
    pub tool_stats: Vec<ToolStat>,
    pub skill_gains: Vec<SkillGain>,
}

/// One manual-mob autocomplete suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManualMobSuggestion {
    pub display: String,
    pub species: String,
    pub maturity: String,
}

/// The quest-link suggestion (all seven fields always present).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuestLinkSuggestion {
    pub session_id: String,
    pub suggestion_type: Option<String>,
    pub reason: Option<String>,
    pub quest_id: Option<String>,
    pub quest_name: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_name: Option<String>,
}

/// One preset reference inside the trifecta attribution summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrifectaPresetRef {
    pub id: String,
    pub name: String,
}

/// The trifecta attribution summary (present when trifecta mode is active
/// and a preset or binding exists). Its members are always emitted (a
/// null binding stays on the wire), so none skip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrifectaAttribution {
    pub active_preset_id: Option<String>,
    pub preset_name: Option<String>,
    pub presets: Vec<TrifectaPresetRef>,
    pub small_weapon: Option<String>,
    pub big_weapon: Option<String>,
    pub heal_tool: Option<String>,
}

/// One recent event in the active-session snapshot feed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecentEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub value: f64,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub timestamp: Option<String>,
    pub id: String,
}

/// One active-session warning.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub value: f64,
}

/// The consolidated dashboard hydration snapshot: the polymorphic idle /
/// active shape, in the model's declaration order. Every field is optional
/// and skipped when absent; under the ratified exclude-unset -> exclude-none
/// movement a present-null field is dropped rather than serialised null.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackingSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotbar_listener_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_attribution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_ocr_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_session_armour_reminder_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mob_entry_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_mob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mob_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trifecta_attribution: Option<TrifectaAttribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_events: Option<Vec<RecentEvent>>,
    #[serde(rename = "session_id", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "started_at", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "kill_count", skip_serializing_if = "Option::is_none")]
    pub kill_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage_dealt_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_damage_dealt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shots_fired_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_hits_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_damage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub globals_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hofs_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_kill_loot: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_avg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_history: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cumulative_net_history: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<Warning>>,
}

/// The start lifecycle acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartResult {
    pub session_id: String,
    pub started_at: String,
    pub status: String,
}

/// The stop lifecycle acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StopResult {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub kill_count: i64,
}

/// The release-mob acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseResult {
    pub released: Option<String>,
}

/// The manual-mob-lock acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualMobLockResult {
    pub mob_name: String,
    pub species: String,
    pub maturity: String,
}

/// The tag-lock acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagLockResult {
    pub tag: String,
}

/// The mob rename / restore result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MobEditResult {
    pub session_id: String,
    pub mob_name: String,
    pub kill_count: i64,
}

/// The loot-item activate / deactivate result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LootItemEditResult {
    pub session_id: String,
    pub item_name: String,
    pub affected_rows: i64,
    pub total_value_delta: f64,
    pub session_total_returns: f64,
}

/// The armour-cost result (echoes the submitted value, not the new total).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArmourCostResult {
    pub session_id: String,
    pub armour_cost: f64,
}

/// The quest-link decision: `accept` carries the full link object, `decline`
/// only `sessionId` / `status`. The accept-only fields skip when absent
/// (exclude-unset -> exclude-none movement: a present-null link field is
/// dropped rather than serialised null).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestLinkDecision {
    pub session_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_name: Option<String>,
}

/// The one-shot repair-cost read (`exclude_unset`): the cost / raw text /
/// confidence on success, plus `error` on a logical refusal.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepairScanResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_ped: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// The recent sessions (newest first), each shaped from its summary or
    /// raw tables. Heals ended-session summaries first (a write on the read
    /// path, preserved from the reference).
    pub async fn tracking_sessions(&self) -> Result<Vec<TrackingSession>, ApiError> {
        let now = naive_to_epoch(self.clock.now());
        let value = list_sessions_impl(&self.db, now)
            .await
            .map_err(ApiError::internal("tracking sessions"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking sessions shaping"))
    }

    /// One session's full detail; an absent session is a not-found.
    pub async fn tracking_session_detail(
        &self,
        session_id: String,
    ) -> Result<SessionDetail, ApiError> {
        let now = naive_to_epoch(self.clock.now());
        match get_session_impl(&self.db, &session_id, now)
            .await
            .map_err(ApiError::internal("tracking session detail"))?
        {
            Some(value) => serde_json::from_value(value)
                .map_err(ApiError::internal("tracking session detail shaping")),
            None => Err(ApiError::not_found("Session not found")),
        }
    }

    /// Free-text tag autocomplete over species-less kills.
    pub async fn tracking_tag_suggestions(
        &self,
        q: String,
        limit: Option<i64>,
    ) -> Result<Vec<String>, ApiError> {
        tag_suggestions_impl(&self.db, &q, limit.unwrap_or(10))
            .await
            .map_err(ApiError::internal("tracking tag suggestions"))
    }

    /// Catalogue mob-name autocomplete. The tag-mode 409 fires first (before
    /// the empty-query short-circuit), consulting the per-session captured
    /// mode when tracking, else the live config.
    pub async fn tracking_manual_mob_suggestions(
        &self,
        q: String,
        limit: Option<i64>,
    ) -> Result<Vec<ManualMobSuggestion>, ApiError> {
        let tag_mode = if self.tracker.is_tracking() {
            self.tracker.is_session_tag_mode()
        } else {
            load_config_readonly(&self.data_dir)
                .map_err(ApiError::internal("manual mob suggestions config"))?
                .mob_tracking_mode
                == "tag"
        };
        if tag_mode {
            return Err(ApiError::conflict("Tag mode disables manual mob selection"));
        }

        let query = q.trim_matches(python_whitespace);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let bounded = limit.unwrap_or(10).clamp(1, 20) as usize;
        let lookup = MobLookupService::new(&self.game_data);
        lookup
            .search_mob_names(query, bounded)
            .into_iter()
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(ApiError::internal("manual mob suggestions shaping"))
            })
            .collect()
    }

    /// The consolidated dashboard hydration snapshot.
    pub async fn tracking_snapshot(&self) -> Result<TrackingSnapshot, ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("tracking snapshot config"))?;
        let value =
            build_snapshot_value(&self.db, &self.tracker, &config, self.hotbar.is_running())
                .await?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking snapshot shaping"))
    }

    /// The curated quest-link suggestion for a completed session; an absent
    /// session is a not-found.
    pub async fn tracking_quest_link_suggestion(
        &self,
        session_id: String,
    ) -> Result<SessionQuestLinkSuggestion, ApiError> {
        if !self.tracking_session_exists(&session_id).await? {
            return Err(ApiError::not_found("Session not found"));
        }
        let suggestion = self
            .quests
            .get_session_link_suggestion(&session_id)
            .await
            .map_err(ApiError::internal("quest-link suggestion"))?;
        let value = format_quest_link_suggestion(&session_id, &suggestion);
        serde_json::from_value(value).map_err(ApiError::internal("quest-link suggestion shaping"))
    }

    /// Begin a tracking session. 409 if one is already active (before the
    /// attribution gate), 400 if the attribution requirement is unmet.
    pub async fn tracking_start(&self) -> Result<StartResult, ApiError> {
        if self.tracker.is_tracking() {
            return Err(ApiError::conflict("Session already active"));
        }
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("tracking start config"))?;
        let (ready, message) = if config.hotbar_hooks_enabled {
            validate_hotbar(&config)
        } else {
            let preset = active_trifecta_preset(&config).map(|p| TrifectaPreset {
                small_weapon_id: p.small_weapon_id,
                big_weapon_id: p.big_weapon_id,
                heal_id: p.heal_id,
            });
            let (ready, reason) = validate_trifecta(&self.db, preset.as_ref())
                .await
                .map_err(ApiError::internal("tracking start validate"))?;
            (
                ready,
                reason.or_else(|| {
                    Some(
                        "Configure the trifecta in the Equipment page before tracking.".to_string(),
                    )
                }),
            )
        };
        if !ready {
            let detail_message = message.unwrap_or_else(|| {
                "Configure the trifecta in the Equipment page before tracking.".to_string()
            });
            return Err(ApiError::bad_request(detail_message));
        }
        let session = self
            .tracker
            .start_session()
            .await
            .map_err(ApiError::internal("tracking start"))?;
        Ok(StartResult {
            session_id: session.id,
            started_at: local_isoformat(session.start_time),
            status: "active".to_string(),
        })
    }

    /// End the active tracking session. 409 if none is active.
    pub async fn tracking_stop(&self) -> Result<StopResult, ApiError> {
        if !self.tracker.is_tracking() {
            return Err(ApiError::conflict("No active session"));
        }
        match self
            .tracker
            .stop_session()
            .await
            .map_err(ApiError::internal("tracking stop"))?
        {
            Some(session) => Ok(StopResult {
                session_id: session.id.clone(),
                started_at: local_isoformat(session.start_time),
                ended_at: session.end_time.map(local_isoformat),
                kill_count: session.kills.len() as i64,
            }),
            // Defensive: `is_tracking` was true above, so a None is a broken
            // invariant. The transport's 500 message does not survive (the
            // boundary reply is fixed); logged server-side.
            None => Err(ApiError::invalid_state("tracking stop returned no session")),
        }
    }

    /// Clear the locked mob or tag, echoing what was released.
    pub async fn tracking_release_mob(&self) -> Result<ReleaseResult, ApiError> {
        // The (non-`Send`) config guard is scoped around each read/write
        // so it is never held across the tracker's await points; the
        // release-then-write order within each branch is unchanged.
        let lock_config = || {
            self.config_service
                .lock()
                .map_err(|_| ApiError::invalid_state("release mob: poisoned config lock"))
        };
        let released = if self.tracker.is_tracking() && self.tracker.is_session_tag_mode() {
            let released = self.tracker.release_current_mob().await;
            lock_config()?
                .update(&clear_tag())
                .map_err(ApiError::internal("release mob"))?;
            released.map(Value::from).unwrap_or(Value::Null)
        } else if !self.tracker.is_tracking() {
            let mut guard = lock_config()?;
            if guard.get().mob_tracking_mode == "tag" {
                let trimmed = guard.get().mob_tracking_tag.trim().to_string();
                let released = if trimmed.is_empty() {
                    Value::Null
                } else {
                    Value::String(trimmed)
                };
                guard
                    .update(&clear_tag())
                    .map_err(ApiError::internal("release mob"))?;
                released
            } else {
                let species = guard.get().manual_mob_species.trim().to_string();
                let maturity = guard.get().manual_mob_maturity.trim().to_string();
                let released = mob_display(&species, &maturity);
                guard
                    .update(&clear_manual_mob())
                    .map_err(ApiError::internal("release mob"))?;
                released
            }
        } else {
            let released = self.tracker.release_current_mob().await;
            lock_config()?
                .update(&clear_manual_mob())
                .map_err(ApiError::internal("release mob"))?;
            released.map(Value::from).unwrap_or(Value::Null)
        };
        Ok(ReleaseResult {
            released: opt_str(&released),
        })
    }

    /// Lock a catalogue mob for manual kill stamping. 409 in tag mode, 400
    /// when the mob is absent from the catalogue.
    pub async fn tracking_manual_mob_lock(
        &self,
        species: String,
        maturity: Option<String>,
    ) -> Result<ManualMobLockResult, ApiError> {
        let maturity = maturity.unwrap_or_default();
        let species = species.trim();
        let maturity = maturity.trim();
        let display = if maturity.is_empty() {
            species.to_string()
        } else {
            format!("{maturity} {species}")
        };
        // Validate and write inside a block so the (non-`Send`) config
        // guard is gone before the tracker await below.
        {
            let Ok(mut guard) = self.config_service.lock() else {
                return Err(ApiError::invalid_state(
                    "manual mob lock: poisoned config lock",
                ));
            };
            let idle_tag_mode =
                !self.tracker.is_tracking() && guard.get().mob_tracking_mode == "tag";
            if (self.tracker.is_tracking() && self.tracker.is_session_tag_mode()) || idle_tag_mode {
                return Err(ApiError::conflict("Tag mode disables manual mob selection"));
            }
            if !MobLookupService::new(&self.game_data).has_mob_name(species, maturity) {
                return Err(ApiError::bad_request("Mob is not present in the catalogue"));
            }
            let mut updates = Map::new();
            updates.insert("manual_mob_species".into(), json!(species));
            updates.insert("manual_mob_maturity".into(), json!(maturity));
            guard
                .update(&updates)
                .map_err(ApiError::internal("manual mob lock"))?;
        }
        if self.tracker.is_tracking()
            && self
                .tracker
                .set_manual_mob(&display, species, maturity)
                .await
                .is_err()
        {
            // The gate cleared an active non-tag session; the only reachable
            // error is the live config having flipped to tag mode since the
            // session started. Mirror the reference's post-write 500.
            return Err(ApiError::invalid_state(
                "manual mob lock: session flipped to tag mode",
            ));
        }
        Ok(ManualMobLockResult {
            mob_name: display,
            species: species.to_string(),
            maturity: maturity.to_string(),
        })
    }

    /// Set the active free-text tag. 409 when not in tag mode, 400 on an
    /// empty tag.
    pub async fn tracking_tag_lock(&self, tag: String) -> Result<TagLockResult, ApiError> {
        let tag = tag.trim();
        // Validate and write inside a block so the (non-`Send`) config
        // guard is gone before the tracker await below.
        {
            let Ok(mut guard) = self.config_service.lock() else {
                return Err(ApiError::invalid_state("tag lock: poisoned config lock"));
            };
            if self.tracker.is_tracking() {
                if !self.tracker.is_session_tag_mode() {
                    return Err(ApiError::conflict("Active session is not in tag mode"));
                }
            } else if guard.get().mob_tracking_mode != "tag" {
                return Err(ApiError::conflict("Tag mode is not enabled"));
            }
            if tag.is_empty() {
                return Err(ApiError::bad_request("Tag cannot be empty"));
            }
            let mut updates = Map::new();
            updates.insert("mob_tracking_tag".into(), json!(tag));
            guard
                .update(&updates)
                .map_err(ApiError::internal("tag lock"))?;
        }
        if self.tracker.is_tracking() {
            let _ = self.tracker.set_manual_tag(tag).await;
        }
        Ok(TagLockResult {
            tag: tag.to_string(),
        })
    }

    /// Rename a mob across an ended session.
    pub async fn tracking_rename_mob(
        &self,
        session_id: String,
        from_mob_name: String,
        to_mob_name: String,
    ) -> Result<MobEditResult, ApiError> {
        let value = rename_session_mob_impl(&self.db, &session_id, &from_mob_name, &to_mob_name)
            .await
            .map_err(edit_error("tracking rename mob"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking rename mob shaping"))
    }

    /// Restore a renamed mob to its preserved original.
    pub async fn tracking_restore_mob(
        &self,
        session_id: String,
        current_mob_name: String,
    ) -> Result<MobEditResult, ApiError> {
        let value = restore_session_mob_impl(&self.db, &session_id, &current_mob_name)
            .await
            .map_err(edit_error("tracking restore mob"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking restore mob shaping"))
    }

    /// Re-activate a deactivated loot line.
    pub async fn tracking_loot_item_activate(
        &self,
        session_id: String,
        item_name: String,
    ) -> Result<LootItemEditResult, ApiError> {
        let value = bulk_flip_loot_item(&self.db, &session_id, &item_name, "active")
            .await
            .map_err(edit_error("tracking loot item activate"))?;
        serde_json::from_value(value)
            .map_err(ApiError::internal("tracking loot item activate shaping"))
    }

    /// Deactivate a loot line.
    pub async fn tracking_loot_item_deactivate(
        &self,
        session_id: String,
        item_name: String,
    ) -> Result<LootItemEditResult, ApiError> {
        let value = bulk_flip_loot_item(&self.db, &session_id, &item_name, "deactivated")
            .await
            .map_err(edit_error("tracking loot item deactivate"))?;
        serde_json::from_value(value)
            .map_err(ApiError::internal("tracking loot item deactivate shaping"))
    }

    /// Add an armour cost to a session (no active-session guard; 404 only
    /// when absent). Echoes the submitted value.
    pub async fn tracking_armour_cost(
        &self,
        session_id: String,
        cost: f64,
    ) -> Result<ArmourCostResult, ApiError> {
        let value = set_armour_cost_impl(&self.db, &session_id, cost)
            .await
            .map_err(edit_error("tracking armour cost"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking armour cost shaping"))
    }

    /// Accept or decline the curated quest-link suggestion. 404 for an absent
    /// session, 400 for an unknown action; accept with no linkable suggestion
    /// is a 409.
    pub async fn tracking_quest_link(
        &self,
        session_id: String,
        action: String,
    ) -> Result<QuestLinkDecision, ApiError> {
        if !self.tracking_session_exists(&session_id).await? {
            return Err(ApiError::not_found("Session not found"));
        }
        let action = action.trim().to_lowercase();
        if action == "accept" {
            return match self
                .quests
                .accept_session_link_suggestion(&session_id)
                .await
            {
                Ok(suggestion) => Ok(QuestLinkDecision {
                    session_id: session_id.clone(),
                    status: "linked".to_string(),
                    link_type: opt_str(&suggestion["suggestion_type"]),
                    quest_id: opt_str(&str_id_or_null(&suggestion["quest_id"])),
                    quest_name: opt_str(&suggestion["quest_name"]),
                    playlist_id: opt_str(&str_id_or_null(&suggestion["playlist_id"])),
                    playlist_name: opt_str(&suggestion["playlist_name"]),
                }),
                Err(QuestError::Invalid(message)) => Err(ApiError::conflict(message)),
                Err(_) => Err(ApiError::invalid_state("quest-link accept")),
            };
        }
        if action == "decline" {
            self.quests
                .decline_session_link(&session_id)
                .await
                .map_err(ApiError::internal("quest-link decline"))?;
            return Ok(QuestLinkDecision {
                session_id,
                status: "declined".to_string(),
                link_type: None,
                quest_id: None,
                quest_name: None,
                playlist_id: None,
                playlist_name: None,
            });
        }
        Err(ApiError::bad_request(
            "Action must be 'accept' or 'decline'",
        ))
    }

    /// The one-shot repair-cost OCR read, gated on `repair_ocr_enabled`
    /// (400 when disabled). The `session_id` is unused (the reference
    /// ignores it too); it stays in the signature for the route mapping.
    pub fn tracking_repair_scan(&self, session_id: String) -> Result<RepairScanResult, ApiError> {
        let _ = session_id;
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("repair scan config"))?;
        if !config.repair_ocr_enabled {
            return Err(ApiError::bad_request("Repair OCR is disabled"));
        }
        let value = project(&self.repair_ocr.scan_repair_cost(), &REPAIR_FIELDS);
        serde_json::from_value(value).map_err(ApiError::internal("repair scan shaping"))
    }

    /// Delete a session and all of its data (an active session cannot be
    /// deleted; a missing one is a not-found). The rollups are repaired for
    /// the days it touched.
    pub async fn tracking_session_delete(&self, session_id: String) -> Result<(), ApiError> {
        delete_session_impl(&self.db, &session_id)
            .await
            .map_err(edit_error("tracking session delete"))
    }

    // ── Private snapshot assembly ────────────────────────────────────

    /// The session-existence precondition the quest-link routes apply.
    async fn tracking_session_exists(&self, session_id: &str) -> Result<bool, ApiError> {
        let sid = session_id.to_string();
        self.db
            .with_reader(move |conn| {
                use rusqlite::OptionalExtension as _;
                Ok(conn
                    .query_row(
                        "SELECT id FROM tracking_sessions WHERE id = ?",
                        rusqlite::params![sid],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .is_some())
            })
            .await
            .map_err(ApiError::internal("session existence"))
    }
}

// ── Snapshot assembly (shared by the live and demo snapshots) ────────

/// Assemble the projected snapshot value from the tracker readout, the
/// resolved config, and the hotbar listener's running state. A free
/// function (rather than an `Api` method) so both the live snapshot and
/// the guide-mode demo snapshot, which runs over its own parallel tracker
/// and database, share one assembly.
pub(crate) async fn build_snapshot_value(
    db: &Db,
    tracker: &HuntTracker,
    config: &AppConfig,
    hotbar_active: bool,
) -> Result<Value, ApiError> {
    let weapon_attribution = if config.hotbar_hooks_enabled {
        "hotbar"
    } else {
        "trifecta"
    };
    let trifecta_attribution = if weapon_attribution == "trifecta" {
        trifecta_attribution_summary(db, config)
            .await
            .map_err(ApiError::internal("snapshot trifecta summary"))?
    } else {
        Value::Null
    };
    let readout = tracker
        .snapshot()
        .await
        .map_err(ApiError::internal("snapshot readout"))?;
    let current_tool = match &readout.current_tool {
        Some(tool) => Value::String(tool.clone()),
        None => Value::Null,
    };

    let value = match &readout.active {
        None => {
            let (current_mob, mob_source) = configured_manual_label(config);
            json!({
                "status": "idle",
                "hotbarListenerActive": hotbar_active,
                "weaponAttribution": weapon_attribution,
                "repairOcrEnabled": config.repair_ocr_enabled,
                "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                "currentTool": current_tool,
                "trifectaAttribution": trifecta_attribution,
                "mobEntryMode": config.mob_tracking_mode,
                "currentMob": current_mob,
                "mobSource": mob_source,
                "recentEvents": [],
            })
        }
        Some(active) => {
            let recent_events: Vec<Value> = active
                    .notable_event_rows
                    .iter()
                    .enumerate()
                    .map(|(index, (event_type, mob_or_item, value_ped, ts))| {
                        json!({
                            "type": notable_event_category(event_type),
                            "description": notable_event_description(event_type, mob_or_item, *value_ped),
                            "value": *value_ped,
                            "eventType": event_type.clone(),
                            "timestamp": event_ts_to_iso(*ts),
                            "id": format!("ne-{index}"),
                        })
                    })
                    .collect();
            let warnings: Vec<Value> = active
                .warnings
                .iter()
                .map(|message| json!({"type": "warning", "description": message, "value": 0.0}))
                .collect();
            json!({
                "status": "active",
                "session_id": active.session_id.clone(),
                "started_at": active.started_at.clone(),
                "kill_count": active.kill_count,
                "elapsed": active.elapsed,
                "cost": active.cost,
                "returns": active.returns,
                "pes": active.pes,
                "net": active.net,
                "returnRate": active.return_rate,
                "damageDealtTotal": active.damage_dealt_total,
                "weaponDamageDealt": active.weapon_damage_dealt,
                "weaponCost": active.weapon_cost,
                "shotsFiredTotal": active.shots_fired_total,
                "criticalHitsTotal": active.critical_hits_total,
                "maxDamage": active.max_damage,
                "globalsCount": active.globals_count,
                "hofsCount": active.hofs_count,
                "latestKillLoot": active.latest_kill_loot,
                "multiplierLast": active.multiplier_last,
                "multiplierAvg": active.multiplier_avg,
                "multiplierMax": active.multiplier_max,
                "multiplierHistory": active.multiplier_history.clone(),
                "cumulativeNetHistory": active.cumulative_net_history.clone(),
                "hotbarListenerActive": hotbar_active,
                "weaponAttribution": weapon_attribution,
                "repairOcrEnabled": config.repair_ocr_enabled,
                "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                "currentTool": current_tool,
                "trifectaAttribution": trifecta_attribution,
                "mobEntryMode": active.mob_entry_mode.clone(),
                "currentMob": active.current_mob.clone(),
                "mobSource": active.mob_source.clone(),
                "recentEvents": recent_events,
                "warnings": warnings,
            })
        }
    };
    Ok(project(&value, &SNAPSHOT_FIELDS))
}

/// `_trifecta_attribution_summary`: the active preset's bound
/// weapon/heal names plus the preset list, or null when nothing exists.
async fn trifecta_attribution_summary(db: &Db, config: &AppConfig) -> Result<Value, DbError> {
    let active = active_trifecta_preset(config);
    let small = active.and_then(|preset| preset.small_weapon_id);
    let big = active.and_then(|preset| preset.big_weapon_id);
    let heal = active.and_then(|preset| preset.heal_id);
    let presets: Vec<Value> = config
        .trifecta_presets
        .iter()
        .map(|preset| json!({"id": preset.id, "name": preset.name}))
        .collect();
    if presets.is_empty() && small.is_none() && big.is_none() && heal.is_none() {
        return Ok(Value::Null);
    }
    let mut summary = Map::new();
    summary.insert(
        "activePresetId".into(),
        match &config.active_trifecta_preset_id {
            Some(id) => Value::String(id.clone()),
            None => Value::Null,
        },
    );
    summary.insert(
        "presetName".into(),
        match active {
            Some(preset) => Value::String(preset.name.clone()),
            None => Value::Null,
        },
    );
    summary.insert("presets".into(), Value::Array(presets));
    summary.insert(
        "smallWeapon".into(),
        equipment_name(db, small, "weapon").await?,
    );
    summary.insert("bigWeapon".into(), equipment_name(db, big, "weapon").await?);
    summary.insert(
        "healTool".into(),
        equipment_name(db, heal, "healing").await?,
    );
    Ok(Value::Object(summary))
}

/// The equipment-library name for a bound id and type, or null.
async fn equipment_name(db: &Db, id: Option<i64>, item_type: &str) -> Result<Value, DbError> {
    let Some(id) = id else {
        return Ok(Value::Null);
    };
    match db.equipment_item(id, item_type).await? {
        Some((_id, name, _properties)) => Ok(Value::String(name)),
        None => Ok(Value::Null),
    }
}
