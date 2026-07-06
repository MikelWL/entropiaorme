//! The tracking read/edit computation: the session list and detail
//! reads, the tag suggestions, the post-hoc session edits, and the
//! snapshot's trifecta attribution summary, over the shared database.
//!
//! The shaping here produces the wire `serde_json::Value` forms the
//! facade's DTOs pin byte-for-byte: the exclude-none projections, the
//! engine-typed numeric reads, and the string renderings are contract
//! lineage (pinned by the frozen goldens and the facade byte-pin
//! tests; ADR-0017/0019), so the `Value` edge is deliberate.

use std::collections::BTreeMap;

use chrono::DateTime;
use serde_json::{json, Map, Value};

use crate::character_calc::ATTRIBUTE_SKILLS;
use crate::config_service::{active_trifecta_preset, AppConfig};
use crate::db::{Db, DbError};
use crate::time::to_iso_utc;
use eo_wire::normalizer::round_half_even;

// ── Engine-typed numeric primitives ─────────────────────────────────

/// A SQLite numeric read preserving the engine type: a REAL decodes to a
/// float, an INTEGER (the `COALESCE(SUM(...), 0)` empty case) to an integer.
/// The stored value's affinity (`ValueRef`) drives the branch directly,
/// preserving the same behaviour as before: an integer value never
/// masquerades as a float.
pub fn sql_number(row: &rusqlite::Row, index: usize) -> Value {
    match row.get_ref_unwrap(index) {
        rusqlite::types::ValueRef::Real(value) => json!(value),
        value => json!(value.as_i64().expect("sql_number reads a numeric column")),
    }
}

/// `float(value)` over an engine-typed number.
pub fn as_f64(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

/// `round(value, places)`: banker's rounding, always producing a float.
pub fn round(value: f64, places: usize) -> f64 {
    round_half_even(value, places)
}

/// A model-declared `float` field: coerce an engine-typed integer to its
/// float form, so an integer zero leaves the wire as `0.0`.
pub fn float_field(value: Value) -> Value {
    match value.as_i64() {
        Some(integer) => json!(integer as f64),
        None => value,
    }
}

/// `datetime.fromtimestamp(ts, tz=UTC).isoformat()` (the session-list
/// start / end times). Emits `+00:00` and 6-digit microseconds only when
/// the fraction is non-zero; `None` maps to JSON null.
pub fn list_ts_to_iso(ts: Option<f64>) -> Value {
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
pub fn event_ts_to_iso(ts: Option<f64>) -> Value {
    match ts {
        Some(ts) => Value::String(to_iso_utc(ts)),
        None => Value::Null,
    }
}

/// Duration in whole seconds: stored span for an ended session, the clock's
/// running span for an active one, else zero.
pub fn duration_seconds(
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
pub fn notable_event_category(event_type: &str) -> &'static str {
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

pub async fn list_sessions_impl(db: &Db, now: f64) -> Result<Value, DbError> {
    // Heal so ended sessions carry current summaries (a write on the read
    // path, preserved), then read each recent session's row as one
    // synchronous unit on a reader-core connection.
    db.with_writer(|conn| crate::session_summary::heal_summaries(conn))
        .await?;
    db.with_reader(move |conn| list_sessions_read(conn, now))
        .await
}

/// The session-list read proper: the recent-session rows, their summaries,
/// and each row shaped from its summary (ended) or the raw tables. One
/// synchronous pass over a reader-core connection.
pub fn list_sessions_read(conn: &rusqlite::Connection, now: f64) -> Result<Value, DbError> {
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

pub fn parse_string_array(text: &str) -> Value {
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
pub fn list_row_from_raw(
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

pub fn scalar(conn: &rusqlite::Connection, sql: &str, sid: &str) -> Result<Value, DbError> {
    Ok(conn.query_row(sql, rusqlite::params![sid], |row| Ok(sql_number(row, 0)))?)
}

pub fn string_column(
    conn: &rusqlite::Connection,
    sql: &str,
    sid: &str,
) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![sid], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ── Session detail ──────────────────────────────────────────────────

pub async fn get_session_impl(
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
pub fn get_session_read(
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

pub fn loot_agg(
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

pub fn loot_breakdown_sorted(rows: &[(String, i64, f64)]) -> Vec<Value> {
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

pub fn session_skill_gains(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> Result<Value, DbError> {
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

pub fn stable_sort_desc_by_key(entries: &mut [(i64, Value)]) {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
}

pub fn stable_sort_desc_by_f64(entries: &mut [(f64, Value)]) {
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
}

/// Whether a tracking session with the given id exists (the
/// session-existence precondition the quest-link operations apply).
pub async fn session_exists(db: &Db, session_id: &str) -> Result<bool, DbError> {
    let sid = session_id.to_string();
    db.with_reader(move |conn| {
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
}

// ── Tag suggestions ─────────────────────────────────────────────────

pub async fn tag_suggestions_impl(db: &Db, q: &str, limit: i64) -> Result<Vec<String>, DbError> {
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
pub enum EditError {
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

pub async fn validate_session_exists(db: &Db, session_id: &str) -> Result<(), EditError> {
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

pub async fn build_mob_edit_response(
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

pub async fn build_loot_item_edit_response(
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

pub async fn rename_session_mob_impl(
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

pub async fn restore_session_mob_impl(
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

pub async fn bulk_flip_loot_item(
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
            crate::daily_rollup::refresh_session_days(&tx, &sid)?;
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

pub async fn set_armour_cost_impl(
    db: &Db,
    session_id: &str,
    cost: f64,
) -> Result<Value, EditError> {
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
            crate::daily_rollup::refresh_days(&tx, [crate::daily_rollup::epoch_day(started_at)])?;
            crate::session_summary::write_session_summary(&tx, &sid)?;
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
pub async fn delete_session_impl(db: &Db, session_id: &str) -> Result<(), EditError> {
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

            crate::daily_rollup::refresh_days(&tx, days)?;
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
pub fn validate_hotbar(config: &AppConfig) -> (bool, Option<String>) {
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
pub fn configured_manual_label(config: &AppConfig) -> (Value, Value) {
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
pub fn notable_event_label(event_type: &str) -> String {
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
pub fn notable_event_description(event_type: &str, mob_or_item: &str, value_ped: f64) -> String {
    let label = notable_event_label(event_type);
    if event_type.starts_with("quest_") {
        format!("{label}: {mob_or_item}")
    } else {
        format!("{label}: {mob_or_item} ({value_ped:.2} PED)")
    }
}

/// Python `str.capitalize` over an ASCII category.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The config update that clears the free-text session tag.
pub fn clear_tag() -> Map<String, Value> {
    let mut updates = Map::new();
    updates.insert("mob_tracking_tag".into(), json!(""));
    updates
}

/// The config update that clears the stored manual-mob selection.
pub fn clear_manual_mob() -> Map<String, Value> {
    let mut updates = Map::new();
    updates.insert("manual_mob_species".into(), json!(""));
    updates.insert("manual_mob_maturity".into(), json!(""));
    updates
}

/// The released-mob display value.
pub fn mob_display(species: &str, maturity: &str) -> Value {
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
pub fn project(value: &Value, order: &[&str]) -> Value {
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
pub fn python_str_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

/// `str(id) if id is not None else None` over a suggestion's nullable id.
pub fn str_id_or_null(value: &Value) -> Value {
    if value.is_null() {
        Value::Null
    } else {
        json!(python_str_of(value))
    }
}

/// `str(...) or None` as an owned `Option<String>` (for the typed DTOs).
pub fn opt_str(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

/// The quest-link suggestion wire shape (`get_session_quest_link_suggestion`).
pub fn format_quest_link_suggestion(session_id: &str, suggestion: &Value) -> Value {
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

/// `_trifecta_attribution_summary`: the active preset's bound
/// weapon/heal names plus the preset list, or null when nothing exists.
pub async fn trifecta_attribution_summary(db: &Db, config: &AppConfig) -> Result<Value, DbError> {
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
pub async fn equipment_name(db: &Db, id: Option<i64>, item_type: &str) -> Result<Value, DbError> {
    let Some(id) = id else {
        return Ok(Value::Null);
    };
    match db.equipment_item(id, item_type).await? {
        Some((_id, name, _properties)) => Ok(Value::String(name)),
        None => Ok(Value::Null),
    }
}
