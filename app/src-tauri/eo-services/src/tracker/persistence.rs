//! The tracker's database writes: kill persistence, the session-end
//! ledger gains, and crash-orphan recovery.

use chrono::{DateTime, Utc};
use eo_wire::normalizer::round_half_even;

use crate::db::DbError;
use crate::session_summary::write_session_summary;
use crate::tracking_models::Kill;

use super::actor::TrackerActor;
use super::time::{epoch_to_instant, local_isoformat};

impl TrackerActor {
    /// Close sessions left open by a crash: end at the latest kill
    /// (or the start), write the ledger gains and the summary, and
    /// clear the active flag.
    pub(super) async fn recover_orphaned_sessions(&self) -> Result<(), DbError> {
        {
            let rows: Vec<(String, f64)> = self
                .db
                .with_reader(|conn| {
                    let mut stmt = conn.prepare(
                        "SELECT id, started_at FROM tracking_sessions WHERE is_active = 1",
                    )?;
                    let mapped = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?;
                    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
                })
                .await?;
            for (session_id, started_at) in rows {
                let sid_read = session_id.clone();
                let latest = self
                    .db
                    .with_reader(move |conn| {
                        Ok(conn.query_row(
                            "SELECT MAX(timestamp) FROM kills WHERE session_id = ?",
                            rusqlite::params![sid_read],
                            |row| row.get::<_, Option<f64>>(0),
                        )?)
                    })
                    .await?;
                // The original's falsy fallback, not a None check: a
                // zero maximum also falls back to the session start.
                let ended_at = match latest {
                    Some(latest) if latest != 0.0 => latest,
                    _ => started_at,
                };
                // Each orphan closes atomically: the same one-commit
                // grouping the stop path uses, so a failure mid-recovery
                // leaves that session untouched and still recoverable.
                let sid = session_id.clone();
                self.db
                    .with_writer(move |conn| {
                        let tx = conn.transaction()?;
                        tx.execute(
                            "UPDATE tracking_sessions SET ended_at = ?, is_active = 0 WHERE id = ?",
                            rusqlite::params![ended_at, sid],
                        )?;

                        let end_dt = epoch_to_instant(ended_at);
                        Self::create_enhancer_rebate_ledger_entry(&tx, &sid, end_dt)?;
                        Self::create_shrapnel_ledger_entry(&tx, &sid, end_dt)?;
                        write_session_summary(&tx, &sid)?;
                        crate::daily_rollup::refresh_session_days(&tx, &sid)?;
                        tx.commit()?;
                        Ok(())
                    })
                    .await?;
            }
            Ok(())
        }
    }
    /// Write a finalised kill to the database: the kill row, the
    /// per-tool stats (`INSERT OR REPLACE` keyed on the tool name, so
    /// among same-name phases the last written wins, as the
    /// original's insertion-ordered iteration does), and the loot
    /// items, under one commit.
    pub(super) async fn persist_kill(&self, kill: &Kill) {
        let kill = kill.clone();
        let result = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT OR REPLACE INTO kills \
                     (id, session_id, mob_name, mob_species, mob_maturity, \
                      timestamp, shots_fired, damage_dealt, damage_taken, \
                      critical_hits, cost_ped, enhancer_cost, \
                      loot_total_ped, is_global, is_hof) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        kill.id,
                        kill.session_id,
                        kill.mob_name,
                        kill.mob_species,
                        kill.mob_maturity,
                        kill.timestamp,
                        kill.shots_fired,
                        kill.damage_dealt,
                        kill.damage_taken,
                        kill.critical_hits,
                        kill.cost_ped.value(),
                        kill.enhancer_cost.value(),
                        kill.loot_total_ped.value(),
                        i64::from(kill.is_global),
                        i64::from(kill.is_hof),
                    ],
                )?;

                for (_, stats) in &kill.tool_stats {
                    tx.execute(
                        "INSERT OR REPLACE INTO kill_tool_stats \
                         (kill_id, tool_name, shots_fired, damage_dealt, \
                          critical_hits, cost_per_shot) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                        rusqlite::params![
                            kill.id,
                            stats.tool_name,
                            stats.shots_fired,
                            stats.damage_dealt,
                            stats.critical_hits,
                            stats.cost_per_shot.value(),
                        ],
                    )?;
                }

                for item in &kill.loot_items {
                    tx.execute(
                        "INSERT INTO kill_loot_items \
                         (kill_id, item_name, quantity, value_ped, is_enhancer_shrapnel) \
                         VALUES (?, ?, ?, ?, ?)",
                        rusqlite::params![
                            kill.id,
                            item.item_name,
                            item.quantity,
                            item.value_ped,
                            i64::from(item.is_enhancer_shrapnel),
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await;
        // Contained like the original's handler exception.
        let _ = result;
    }

    /// Session-end margin on non-enhancer Shrapnel loot (1%, the
    /// trade-terminal conversion premium), recorded as a markup
    /// ledger gain.
    pub(super) fn create_shrapnel_ledger_entry(
        conn: &rusqlite::Connection,
        session_id: &str,
        end_time: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let shrapnel_ped: f64 = conn.query_row(
            "SELECT COALESCE(SUM(kli.value_ped), 0) \
             FROM kill_loot_items kli \
             JOIN kills k ON kli.kill_id = k.id \
             WHERE k.session_id = ? AND kli.item_name = 'Shrapnel' \
             AND COALESCE(kli.is_enhancer_shrapnel, 0) = 0 \
             AND kli.deactivated_at IS NULL",
            rusqlite::params![session_id],
            |row| row.get::<_, f64>(0),
        )?;
        if shrapnel_ped <= 0.0 {
            return Ok(());
        }
        let margin = round_half_even(shrapnel_ped * 0.01, 4);
        let date = local_isoformat(end_time);
        conn.execute(
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                date,
                "markup",
                "Shrapnel Conversion",
                margin,
                "convert",
            ],
        )?;
        // A live stop dates this "now" (past the rollup watermark), but
        // orphan recovery backdates it to the crashed session's end, so
        // the entry's day must reland with the write.
        crate::daily_rollup::refresh_days(conn, [date])?;
        Ok(())
    }

    /// Session-end rebate on enhancer-break Shrapnel (full TT value
    /// returned by breaks), recorded as a markup ledger gain.
    pub(super) fn create_enhancer_rebate_ledger_entry(
        conn: &rusqlite::Connection,
        session_id: &str,
        end_time: DateTime<Utc>,
    ) -> Result<(), DbError> {
        let rebate: f64 = conn.query_row(
            "SELECT COALESCE(SUM(kli.value_ped), 0) \
             FROM kill_loot_items kli \
             JOIN kills k ON kli.kill_id = k.id \
             WHERE k.session_id = ? AND COALESCE(kli.is_enhancer_shrapnel, 0) = 1 \
             AND kli.deactivated_at IS NULL",
            rusqlite::params![session_id],
            |row| row.get::<_, f64>(0),
        )?;
        if rebate <= 0.0 {
            return Ok(());
        }
        let date = local_isoformat(end_time);
        conn.execute(
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                date,
                "markup",
                "Enhancer Shrapnel Rebate",
                round_half_even(rebate, 4),
                "enhancer",
            ],
        )?;
        // Same watermark reasoning as the shrapnel-conversion entry:
        // orphan recovery can backdate this day.
        crate::daily_rollup::refresh_days(conn, [date])?;
        Ok(())
    }
}
