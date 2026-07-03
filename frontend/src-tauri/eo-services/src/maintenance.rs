//! On-demand read-model maintenance: rebuild every projection from the raw
//! tracking tables and prove the result matches the incrementally-maintained
//! rows.
//!
//! The Overview's `daily_rollups` / `daily_ledger_rollups` and the Activity
//! and session-list `session_summaries` are CQRS-style read models: the raw
//! tracking tables are the source of truth, and each projection is a pure
//! function of them, maintained eagerly on write and healed lazily on read.
//! This module makes that guarantee runnable: heal the projections current,
//! snapshot them, drop and rebuild them from the raw tables alone, then
//! assert the rebuilt rows are byte-identical to the incrementally-maintained
//! ones. A mismatch is a projection-staleness bug (a missed write hook),
//! named and surfaced instead of silently served.
//!
//! Each projection row carries a `computed_at` bookkeeping timestamp that is
//! wall-clock at recompute and so differs between an incremental write and a
//! rebuild; it is excluded from the comparison. Every column that carries
//! meaning is included.

use serde_json::Value;
use sqlx::SqlitePool;

use crate::db::{row_to_json, Db, DbError};
use crate::{daily_rollup, session_summary};

/// A projection table and the deterministic, `computed_at`-excluding
/// snapshot query used to compare its incremental and rebuilt states.
struct Projection {
    table: &'static str,
    snapshot_sql: &'static str,
}

/// The three read models, each rebuildable from the raw tracking tables.
const PROJECTIONS: [Projection; 3] = [
    Projection {
        table: "session_summaries",
        snapshot_sql: "SELECT session_id, summary_version, started_at, ended_at, \
             duration_hours, kills, loot_tt, weapon_cost, enhancer_cost, armour_cost, \
             heal_cost, dangling_cost, cycled_ped, regular_skill_ped_json, \
             attribute_levels_json, regular_skill_tt, attribute_levels_total, \
             dominant_mob, dominant_tag, dominant_weapon, dominant_mob_kills, \
             dominant_tag_kills, activity_skill_tt, primary_mobs_json, \
             primary_weapons_json, globals, hofs \
             FROM session_summaries ORDER BY session_id",
    },
    Projection {
        table: "daily_rollups",
        snapshot_sql: "SELECT day, rollup_version, dirty, has_rows, loot_tt, weapon_cost, \
             enhancer_cost, armour_cost, heal_cost, dangling_cost, skill_tt, codex_pes, \
             quest_pes FROM daily_rollups ORDER BY day",
    },
    Projection {
        table: "daily_ledger_rollups",
        snapshot_sql: "SELECT day, entry_type, tag, amount FROM daily_ledger_rollups \
             ORDER BY day, entry_type, tag",
    },
];

/// One projection table's verdict from a rebuild-and-verify run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableVerdict {
    /// The projection table's name.
    pub table: &'static str,
    /// Whether the rebuilt rows were byte-identical to the
    /// incrementally-maintained ones.
    pub matched: bool,
    /// The row count after the rebuild (equal to the incremental count when
    /// `matched`).
    pub row_count: usize,
}

/// The result of rebuilding every read model and comparing it against the
/// incrementally-maintained rows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RebuildReport {
    /// One verdict per projection, in a stable order.
    pub tables: Vec<TableVerdict>,
}

impl RebuildReport {
    /// True when every projection rebuilt byte-identically: the CQRS
    /// rebuildability guarantee holds.
    pub fn all_matched(&self) -> bool {
        self.tables.iter().all(|verdict| verdict.matched)
    }
}

/// Snapshot one projection's rows in its deterministic order, each row
/// canonically serialised (typed by its stored value) for comparison.
async fn snapshot(pool: &SqlitePool, sql: &'static str) -> Result<Vec<Value>, DbError> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    rows.iter().map(row_to_json).collect()
}

/// Heal the read models current, then drop and rebuild every one of them
/// from the raw tracking tables, asserting the rebuilt rows are byte-
/// identical to the incrementally-maintained ones. This is the proof that
/// the projections are a pure function of the raw tracking tables (the CQRS
/// rebuildability guarantee recorded in ADR-0018) and the standing
/// mitigation for projection staleness: a mismatch names the table whose
/// incremental maintenance drifted from a rebuild.
///
/// Runs entirely on the writer: the rebuild is a write, and each snapshot
/// reads back its own committed writes on the same serialised connection,
/// so there is no cross-connection visibility window. Off the hot path.
pub async fn rebuild_and_verify(db: &Db, now: f64) -> Result<RebuildReport, DbError> {
    let pool = db.write();

    // Bring the incremental projections fully current, so the comparison is
    // against an up-to-date maintained state rather than a mid-heal one.
    daily_rollup::heal_rollups(pool, now).await?;
    session_summary::heal_summaries(pool).await?;

    let mut incremental = Vec::with_capacity(PROJECTIONS.len());
    for projection in &PROJECTIONS {
        incremental.push(snapshot(pool, projection.snapshot_sql).await?);
    }

    // Rebuild every read model from the raw tables alone.
    daily_rollup::rebuild_rollups(pool, now).await?;
    session_summary::rebuild_summaries(pool).await?;

    let mut tables = Vec::with_capacity(PROJECTIONS.len());
    for (projection, maintained) in PROJECTIONS.iter().zip(incremental) {
        let rebuilt = snapshot(pool, projection.snapshot_sql).await?;
        tables.push(TableVerdict {
            table: projection.table,
            matched: rebuilt == maintained,
            row_count: rebuilt.len(),
        });
    }
    Ok(RebuildReport { tables })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    /// A fixed clock well past any seeded day, so the heal watermark covers
    /// the seeded calendar.
    const NOW: f64 = 1_000_000_000.0;

    async fn db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("entropia_orme.db"))
            .await
            .unwrap();
        (dir, db)
    }

    async fn run(db: &Db, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
            .execute(db.write())
            .await
            .unwrap();
    }

    /// Seed a session with kills, skill gains and ledger entries, so all
    /// three projections are non-empty.
    async fn seed(db: &Db) {
        run(
            db,
            "INSERT INTO tracking_sessions \
             (id, started_at, ended_at, is_active, armour_cost, heal_cost, dangling_cost) \
             VALUES ('s1', 999648000.0, 999658800.0, 0, 0.07, 0.11, 0.0)",
        )
        .await;
        run(
            db,
            "INSERT INTO kills (id, session_id, mob_name, timestamp, enhancer_cost, loot_total_ped) \
             VALUES ('k1', 's1', 'Atrox', 999652000.0, 0.02, 2.0), \
                    ('k2', 's1', 'Atrox', 999653000.0, 0.02, 4.5)",
        )
        .await;
        run(
            db,
            "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, cost_per_shot) \
             VALUES ('k1', 'Rifle', 30, 0.05)",
        )
        .await;
        run(
            db,
            "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
             VALUES ('s1', 999652100.0, 'Rifle', 1.0, 0.5)",
        )
        .await;
        run(
            db,
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) VALUES \
             ('l1', '2001-09-05', 'markup', 'sale', 3.0, 'manual'), \
             ('l2', '2001-09-05', 'expense', 'repair', 1.0, 'repair')",
        )
        .await;
    }

    #[tokio::test]
    async fn rebuild_and_verify_matches_the_incrementally_maintained_models() {
        let (_dir, db) = db().await;
        seed(&db).await;

        let report = rebuild_and_verify(&db, NOW).await.unwrap();
        assert!(
            report.all_matched(),
            "every projection is a pure function of the raw tables: {report:?}"
        );
        // All three projections are covered and non-trivial.
        assert_eq!(report.tables.len(), 3);
        assert!(report
            .tables
            .iter()
            .any(|t| t.table == "session_summaries" && t.row_count == 1));
        assert!(report
            .tables
            .iter()
            .any(|t| t.table == "daily_rollups" && t.row_count > 0));
        assert!(report
            .tables
            .iter()
            .any(|t| t.table == "daily_ledger_rollups" && t.row_count == 2));
    }

    #[tokio::test]
    async fn a_corrupted_projection_row_is_caught_as_a_mismatch() {
        let (_dir, db) = db().await;
        seed(&db).await;
        // Heal so the incremental rows exist, then poison one: this is the
        // projection-staleness class the verifier exists to catch.
        daily_rollup::heal_rollups(db.write(), NOW).await.unwrap();
        run(
            &db,
            "UPDATE daily_rollups SET loot_tt = 999.0 WHERE day = '2001-09-05'",
        )
        .await;

        let report = rebuild_and_verify(&db, NOW).await.unwrap();
        // The rebuild wipes the poisoned value, so the snapshot taken before
        // the rebuild (the poisoned one) differs from the rebuilt truth.
        assert!(
            !report.all_matched(),
            "a drifted daily_rollups row must fail the equality proof"
        );
        let rollups = report
            .tables
            .iter()
            .find(|t| t.table == "daily_rollups")
            .unwrap();
        assert!(!rollups.matched);
    }
}
