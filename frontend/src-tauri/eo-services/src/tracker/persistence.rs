//! The tracker's database writes: kill persistence, the session-end
//! ledger gains, and crash-orphan recovery.

use chrono::NaiveDateTime;
use eo_wire::normalizer::round_half_even;
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::db::{decoded_f64, DbError};
use crate::session_summary::write_session_summary;
use crate::tracking_models::Kill;

use super::actor::TrackerActor;
use super::time::{epoch_to_naive, naive_isoformat};

impl TrackerActor {
    /// Close sessions left open by a crash: end at the latest kill
    /// (or the start), write the ledger gains and the summary, and
    /// clear the active flag.
    pub(super) async fn recover_orphaned_sessions(&self) -> Result<(), DbError> {
        {
            let rows =
                sqlx::query("SELECT id, started_at FROM tracking_sessions WHERE is_active = 1")
                    .fetch_all(self.db.read())
                    .await?;
            for row in rows {
                let session_id: String = row.try_get(0)?;
                let started_at: f64 = row.try_get(1)?;
                let kill_row = sqlx::query("SELECT MAX(timestamp) FROM kills WHERE session_id = ?")
                    .bind(&session_id)
                    .fetch_one(self.db.read())
                    .await?;
                // The original's falsy fallback, not a None check: a
                // zero maximum also falls back to the session start.
                let ended_at = match kill_row.try_get::<Option<f64>, _>(0)? {
                    Some(latest) if latest != 0.0 => latest,
                    _ => started_at,
                };
                // Each orphan closes atomically: the same one-commit
                // grouping the stop path uses, so a failure mid-recovery
                // leaves that session untouched and still recoverable.
                let mut tx = self.db.write().begin().await?;
                sqlx::query(
                    "UPDATE tracking_sessions SET ended_at = ?, is_active = 0 WHERE id = ?",
                )
                .bind(ended_at)
                .bind(&session_id)
                .execute(&mut *tx)
                .await?;

                let end_dt = epoch_to_naive(ended_at);
                Self::create_enhancer_rebate_ledger_entry(&mut tx, &session_id, end_dt).await?;
                Self::create_shrapnel_ledger_entry(&mut tx, &session_id, end_dt).await?;
                write_session_summary(&mut tx, &session_id).await?;
                crate::daily_rollup::refresh_session_days(&mut tx, &session_id).await?;
                tx.commit().await?;
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
        let result: Result<(), sqlx::Error> = async {
            let mut tx = self.db.write().begin().await?;
            sqlx::query(
                "INSERT OR REPLACE INTO kills \
                 (id, session_id, mob_name, mob_species, mob_maturity, \
                  timestamp, shots_fired, damage_dealt, damage_taken, \
                  critical_hits, cost_ped, enhancer_cost, \
                  loot_total_ped, is_global, is_hof) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&kill.id)
            .bind(&kill.session_id)
            .bind(&kill.mob_name)
            .bind(&kill.mob_species)
            .bind(&kill.mob_maturity)
            .bind(kill.timestamp)
            .bind(kill.shots_fired)
            .bind(kill.damage_dealt)
            .bind(kill.damage_taken)
            .bind(kill.critical_hits)
            .bind(kill.cost_ped.value())
            .bind(kill.enhancer_cost.value())
            .bind(kill.loot_total_ped.value())
            .bind(i64::from(kill.is_global))
            .bind(i64::from(kill.is_hof))
            .execute(&mut *tx)
            .await?;

            for (_, stats) in &kill.tool_stats {
                sqlx::query(
                    "INSERT OR REPLACE INTO kill_tool_stats \
                     (kill_id, tool_name, shots_fired, damage_dealt, \
                      critical_hits, cost_per_shot) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&kill.id)
                .bind(&stats.tool_name)
                .bind(stats.shots_fired)
                .bind(stats.damage_dealt)
                .bind(stats.critical_hits)
                .bind(stats.cost_per_shot.value())
                .execute(&mut *tx)
                .await?;
            }

            for item in &kill.loot_items {
                sqlx::query(
                    "INSERT INTO kill_loot_items \
                     (kill_id, item_name, quantity, value_ped, is_enhancer_shrapnel) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&kill.id)
                .bind(&item.item_name)
                .bind(item.quantity)
                .bind(item.value_ped)
                .bind(i64::from(item.is_enhancer_shrapnel))
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            Ok(())
        }
        .await;
        // Contained like the original's handler exception.
        let _ = result;
    }

    /// Session-end margin on non-enhancer Shrapnel loot (1%, the
    /// trade-terminal conversion premium), recorded as a markup
    /// ledger gain.
    pub(super) async fn create_shrapnel_ledger_entry(
        conn: &mut SqliteConnection,
        session_id: &str,
        end_time: NaiveDateTime,
    ) -> Result<(), DbError> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(kli.value_ped), 0) \
             FROM kill_loot_items kli \
             JOIN kills k ON kli.kill_id = k.id \
             WHERE k.session_id = ? AND kli.item_name = 'Shrapnel' \
             AND COALESCE(kli.is_enhancer_shrapnel, 0) = 0 \
             AND kli.deactivated_at IS NULL",
        )
        .bind(session_id)
        .fetch_one(&mut *conn)
        .await?;
        let shrapnel_ped = decoded_f64(&row, 0);
        if shrapnel_ped <= 0.0 {
            return Ok(());
        }
        let margin = round_half_even(shrapnel_ped * 0.01, 4);
        let date = naive_isoformat(end_time);
        sqlx::query(
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&date)
        .bind("markup")
        .bind("Shrapnel Conversion")
        .bind(margin)
        .bind("convert")
        .execute(&mut *conn)
        .await?;
        // A live stop dates this "now" (past the rollup watermark), but
        // orphan recovery backdates it to the crashed session's end, so
        // the entry's day must reland with the write.
        crate::daily_rollup::refresh_days(&mut *conn, [date]).await?;
        Ok(())
    }

    /// Session-end rebate on enhancer-break Shrapnel (full TT value
    /// returned by breaks), recorded as a markup ledger gain.
    pub(super) async fn create_enhancer_rebate_ledger_entry(
        conn: &mut SqliteConnection,
        session_id: &str,
        end_time: NaiveDateTime,
    ) -> Result<(), DbError> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(kli.value_ped), 0) \
             FROM kill_loot_items kli \
             JOIN kills k ON kli.kill_id = k.id \
             WHERE k.session_id = ? AND COALESCE(kli.is_enhancer_shrapnel, 0) = 1 \
             AND kli.deactivated_at IS NULL",
        )
        .bind(session_id)
        .fetch_one(&mut *conn)
        .await?;
        let rebate = decoded_f64(&row, 0);
        if rebate <= 0.0 {
            return Ok(());
        }
        let date = naive_isoformat(end_time);
        sqlx::query(
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&date)
        .bind("markup")
        .bind("Enhancer Shrapnel Rebate")
        .bind(round_half_even(rebate, 4))
        .bind("enhancer")
        .execute(&mut *conn)
        .await?;
        // Same watermark reasoning as the shrapnel-conversion entry:
        // orphan recovery can backdate this day.
        crate::daily_rollup::refresh_days(&mut *conn, [date]).await?;
        Ok(())
    }
}
