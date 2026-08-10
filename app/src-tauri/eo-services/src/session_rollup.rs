//! Materialised per-session activity rollups: the read model behind the
//! Hunting analytics and stock surfaces, the session-grain sibling of
//! [`crate::daily_rollup`]. Source of truth is the raw tracking tables;
//! rows write eagerly at the mutation points and heal lazily on read.
//!
//! ## The model
//!
//! An ended session's events aggregate to four cell sets no activity
//! consumer folds finer than: kill cells by `(context, species, maturity)`
//! (`session_kill_rollups`), active loot cells by `(species, shrapnel,
//! item)` (`session_loot_rollups`, species pre-folded to the empty string
//! for shrapnel rows exactly as the position reads fold it), loot-
//! composition cells by context (`session_context_loot_rollups`), and
//! skill-gain cells by context (`session_pes_rollups`). The raw tables
//! grow with total play history; the cells stay proportional to sessions,
//! species, and items, so a reader folding cells does O(cells) work
//! however long the history gets.
//!
//! `session_rollup_meta` is the settlement boundary, the counterpart of
//! the daily watermark: a session marked at [`ROLLUP_VERSION`] serves from
//! its cells, and every other session (the live one, a freshly edited one,
//! a stale version) serves raw, scoped to its own session id. Readers are
//! therefore correct regardless of heal timing; healing only keeps the
//! raw-served set small. The mutation seams (session stop, orphan
//! recovery, the loot edit flip, session delete) recompute or drop the
//! session's cells inside their own transaction, the same discipline as
//! [`crate::daily_rollup::refresh_session_days`]; a crash between write
//! and commit leaves the marker absent, and the next heal repairs it.

use crate::db::DbError;

/// Bump when a cell's meaning changes: below-version sessions are served
/// raw and heal on the next read.
pub const ROLLUP_VERSION: i64 = 2;

/// Drop one session's cells and marker. The session reads raw from this
/// commit on; the delete path wants exactly that, and [`recompute_session`]
/// starts here so a recompute can never leave mixed generations.
pub fn drop_session(conn: &rusqlite::Connection, session_id: &str) -> Result<(), DbError> {
    // Marker first: if anything below fails mid-way, an unmarked session
    // is merely raw-served, never wrong.
    conn.execute(
        "DELETE FROM session_rollup_meta WHERE session_id = ?",
        rusqlite::params![session_id],
    )?;
    for table in [
        "session_kill_rollups",
        "session_loot_rollups",
        "session_context_loot_rollups",
        "session_pes_rollups",
    ] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE session_id = ?"),
            rusqlite::params![session_id],
        )?;
    }
    Ok(())
}

/// Recompute one session's cells in place. An unended session settles
/// nothing (it stays raw-served); an ended one gets fresh cells and the
/// marker, in that order. The caller owns the surrounding commit.
pub fn recompute_session(conn: &rusqlite::Connection, session_id: &str) -> Result<(), DbError> {
    use rusqlite::OptionalExtension as _;

    drop_session(conn, session_id)?;
    let ended = conn
        .query_row(
            "SELECT ended_at IS NOT NULL FROM tracking_sessions WHERE id = ?",
            rusqlite::params![session_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false);
    if !ended {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO session_kill_rollups \
             (session_id, context_id, mob_species, mob_maturity, kills, cycled_ped, loot_tt) \
         SELECT session_id, context_id, \
                COALESCE(mob_species, ''), COALESCE(mob_maturity, ''), \
                COUNT(*), \
                COALESCE(SUM(cost_ped + enhancer_cost), 0), \
                COALESCE(SUM(loot_total_ped), 0) \
         FROM kills WHERE session_id = ?1 \
         GROUP BY 1, 2, 3, 4",
        rusqlite::params![session_id],
    )?;
    // Shrapnel rows fold their species away here, not at read time: every
    // consumer either folds it (positions) or excludes the rows entirely
    // (composition), so the cell grain carries the flag and no species.
    conn.execute(
        "INSERT INTO session_loot_rollups \
             (session_id, mob_species, is_enhancer_shrapnel, item_name, quantity, value_ped) \
         SELECT k.session_id, \
                CASE WHEN li.is_enhancer_shrapnel = 0 THEN COALESCE(k.mob_species, '') \
                     ELSE '' END, \
                li.is_enhancer_shrapnel, li.item_name, \
                SUM(li.quantity), COALESCE(SUM(li.value_ped), 0) \
         FROM kill_loot_items li \
         JOIN kills k ON k.id = li.kill_id \
         WHERE k.session_id = ?1 AND li.deactivated_at IS NULL \
         GROUP BY 1, 2, 3, 4",
        rusqlite::params![session_id],
    )?;
    conn.execute(
        "INSERT INTO session_context_loot_rollups \
             (session_id, context_id, item_name, quantity, value_ped) \
         SELECT k.session_id, k.context_id, li.item_name, \
                SUM(li.quantity), COALESCE(SUM(li.value_ped), 0) \
         FROM kill_loot_items li \
         JOIN kills k ON k.id = li.kill_id \
         WHERE k.session_id = ?1 AND li.deactivated_at IS NULL \
           AND li.is_enhancer_shrapnel = 0 \
         GROUP BY 1, 2, 3",
        rusqlite::params![session_id],
    )?;
    conn.execute(
        "INSERT INTO session_pes_rollups (session_id, context_id, pes) \
         SELECT session_id, context_id, COALESCE(SUM(ped_value), 0) \
         FROM skill_gains \
         WHERE session_id = ?1 AND ped_value IS NOT NULL \
         GROUP BY 1, 2",
        rusqlite::params![session_id],
    )?;
    conn.execute(
        "INSERT INTO session_rollup_meta (session_id, rollup_version) VALUES (?, ?)",
        rusqlite::params![session_id, ROLLUP_VERSION],
    )?;
    Ok(())
}

/// Drop every cell and marker and settle the whole history again: the
/// from-scratch rebuild the maintenance surface uses to prove the
/// incremental maintenance never drifts from a clean projection
/// (`maintenance::rebuild_and_verify`, the ADR-0018 rebuildability
/// guarantee).
pub fn rebuild(conn: &mut rusqlite::Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "DELETE FROM session_rollup_meta; \
         DELETE FROM session_kill_rollups; \
         DELETE FROM session_loot_rollups; \
         DELETE FROM session_context_loot_rollups; \
         DELETE FROM session_pes_rollups;",
    )?;
    heal(conn)
}

/// The sessions currently served raw: unmarked or below-version. The live
/// session is always here; after a heal it is the only member. Plain
/// `rusqlite` errors so raw read helpers can call it without an error
/// bridge.
pub fn unsettled_sessions(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.id FROM tracking_sessions t \
         LEFT JOIN session_rollup_meta m \
                ON m.session_id = t.id AND m.rollup_version >= ?1 \
         WHERE m.session_id IS NULL",
    )?;
    let rows = stmt.query_map(rusqlite::params![ROLLUP_VERSION], |row| row.get(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
}

/// Bring the rollups current: settle every ended session still served
/// raw. The first call after the migration backfills the whole history;
/// steady-state calls find nothing to do beyond the marker scan. Callers
/// run this on the write connection before an activity read, exactly as
/// the Overview heals the daily rollups.
///
/// Settlement is set-based (one grouped pass per raw table over the whole
/// pending set) rather than a [`recompute_session`] loop: for the
/// backfill case that is the difference between one sequential pass over
/// each table and a random-access walk per session, and for the
/// steady-state case (one or two pending sessions) the named join order
/// makes it the same indexed work.
pub fn heal(conn: &mut rusqlite::Connection) -> Result<(), DbError> {
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tracking_sessions t \
         LEFT JOIN session_rollup_meta m \
                ON m.session_id = t.id AND m.rollup_version >= ?1 \
         WHERE m.session_id IS NULL AND t.ended_at IS NOT NULL",
        rusqlite::params![ROLLUP_VERSION],
        |row| row.get(0),
    )?;
    let orphaned: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_rollup_meta \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
        |row| row.get(0),
    )?;
    if pending == 0 && orphaned == 0 {
        return Ok(());
    }
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS pending_settlement (id TEXT PRIMARY KEY); \
         DELETE FROM pending_settlement;",
    )?;
    tx.execute(
        "INSERT INTO pending_settlement (id) \
         SELECT t.id FROM tracking_sessions t \
         LEFT JOIN session_rollup_meta m \
                ON m.session_id = t.id AND m.rollup_version >= ?1 \
         WHERE m.session_id IS NULL AND t.ended_at IS NOT NULL",
        rusqlite::params![ROLLUP_VERSION],
    )?;
    // Markers first, then any stale-version cells, so a failure part-way
    // leaves the pending sessions merely unmarked and raw-served.
    for table in [
        "session_rollup_meta",
        "session_kill_rollups",
        "session_loot_rollups",
        "session_context_loot_rollups",
        "session_pes_rollups",
    ] {
        tx.execute(
            &format!(
                "DELETE FROM {table} \
                 WHERE session_id IN (SELECT id FROM pending_settlement)"
            ),
            [],
        )?;
    }
    // CROSS JOIN names the join order (pending ids through the session
    // and kill indexes); a planner-chosen order could re-scan the whole
    // raw table for a single pending session.
    tx.execute(
        "INSERT INTO session_kill_rollups \
             (session_id, context_id, mob_species, mob_maturity, kills, cycled_ped, loot_tt) \
         SELECT p.id, k.context_id, \
                COALESCE(k.mob_species, ''), COALESCE(k.mob_maturity, ''), \
                COUNT(*), \
                COALESCE(SUM(k.cost_ped + k.enhancer_cost), 0), \
                COALESCE(SUM(k.loot_total_ped), 0) \
         FROM pending_settlement p CROSS JOIN kills k \
         WHERE k.session_id = p.id \
         GROUP BY 1, 2, 3, 4",
        [],
    )?;
    tx.execute(
        "INSERT INTO session_loot_rollups \
             (session_id, mob_species, is_enhancer_shrapnel, item_name, quantity, value_ped) \
         SELECT p.id, \
                CASE WHEN li.is_enhancer_shrapnel = 0 THEN COALESCE(k.mob_species, '') \
                     ELSE '' END, \
                li.is_enhancer_shrapnel, li.item_name, \
                SUM(li.quantity), COALESCE(SUM(li.value_ped), 0) \
         FROM pending_settlement p CROSS JOIN kills k CROSS JOIN kill_loot_items li \
         WHERE k.session_id = p.id AND li.kill_id = k.id AND li.deactivated_at IS NULL \
         GROUP BY 1, 2, 3, 4",
        [],
    )?;
    tx.execute(
        "INSERT INTO session_context_loot_rollups \
             (session_id, context_id, item_name, quantity, value_ped) \
         SELECT p.id, k.context_id, li.item_name, \
                SUM(li.quantity), COALESCE(SUM(li.value_ped), 0) \
         FROM pending_settlement p CROSS JOIN kills k CROSS JOIN kill_loot_items li \
         WHERE k.session_id = p.id AND li.kill_id = k.id \
           AND li.deactivated_at IS NULL AND li.is_enhancer_shrapnel = 0 \
         GROUP BY 1, 2, 3",
        [],
    )?;
    tx.execute(
        "INSERT INTO session_pes_rollups (session_id, context_id, pes) \
         SELECT p.id, sg.context_id, COALESCE(SUM(sg.ped_value), 0) \
         FROM pending_settlement p CROSS JOIN skill_gains sg \
         WHERE sg.session_id = p.id AND sg.ped_value IS NOT NULL \
         GROUP BY 1, 2",
        [],
    )?;
    tx.execute(
        "INSERT INTO session_rollup_meta (session_id, rollup_version) \
         SELECT id, ?1 FROM pending_settlement",
        rusqlite::params![ROLLUP_VERSION],
    )?;
    tx.execute("DELETE FROM pending_settlement", [])?;
    // Cells for sessions the raw tables no longer know (an external bulk
    // copy, a partial restore) would ghost into the folds; sweep them with
    // the same transaction that settles the backlog.
    tx.execute(
        "DELETE FROM session_kill_rollups \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
    )?;
    tx.execute(
        "DELETE FROM session_loot_rollups \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
    )?;
    tx.execute(
        "DELETE FROM session_context_loot_rollups \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
    )?;
    tx.execute(
        "DELETE FROM session_pes_rollups \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
    )?;
    tx.execute(
        "DELETE FROM session_rollup_meta \
         WHERE session_id NOT IN (SELECT id FROM tracking_sessions)",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    async fn open_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("test.db")).await.expect("open");
        (dir, db)
    }

    fn seed_session(conn: &rusqlite::Connection, id: &str, ended: bool) {
        conn.execute(
            "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active) \
             VALUES (?, 1000.0, ?, 0)",
            rusqlite::params![id, ended.then_some(5000.0)],
        )
        .expect("session");
    }

    fn seed_kill(conn: &rusqlite::Connection, id: &str, session: &str, species: &str) {
        conn.execute(
            "INSERT INTO kills (id, session_id, timestamp, mob_name, mob_species, \
                                mob_maturity, cost_ped, enhancer_cost, loot_total_ped) \
             VALUES (?, ?, 2000.0, ?, ?, 'Old', 1.5, 0.5, 3.25)",
            rusqlite::params![id, session, species, species],
        )
        .expect("kill");
        conn.execute(
            "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, \
                                          is_enhancer_shrapnel) \
             VALUES (?, 'Animal Hide', 2, 1.75, 0), (?, 'Shrapnel', 100, 0.01, 1)",
            rusqlite::params![id, id],
        )
        .expect("loot");
        conn.execute(
            "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
             VALUES (?, 2000.0, 'Rifle', 0.5, 0.0421)",
            rusqlite::params![session],
        )
        .expect("gain");
    }

    #[tokio::test]
    async fn ended_sessions_settle_and_live_ones_stay_raw() {
        let (_dir, db) = open_db().await;
        db.with_writer(|conn| {
            seed_session(conn, "s-ended", true);
            seed_session(conn, "s-live", false);
            seed_kill(conn, "k1", "s-ended", "Atrox");
            seed_kill(conn, "k2", "s-live", "Daikiba");
            heal(conn)?;

            let unsettled = unsettled_sessions(conn)?;
            assert_eq!(unsettled, vec!["s-live".to_string()]);

            let (kills, cycled, loot): (i64, f64, f64) = conn.query_row(
                "SELECT kills, cycled_ped, loot_tt FROM session_kill_rollups \
                 WHERE session_id = 's-ended'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            assert_eq!(kills, 1);
            assert!((cycled - 2.0).abs() < 1e-9);
            assert!((loot - 3.25).abs() < 1e-9);

            // Shrapnel folds its species away and keeps its flag.
            let shrapnel_species: String = conn.query_row(
                "SELECT mob_species FROM session_loot_rollups \
                 WHERE session_id = 's-ended' AND is_enhancer_shrapnel = 1",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(shrapnel_species, "");

            let live_cells: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_kill_rollups WHERE session_id = 's-live'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(live_cells, 0);
            Ok(())
        })
        .await
        .expect("writer");
    }

    #[tokio::test]
    async fn recompute_reflects_edits_and_deactivations() {
        let (_dir, db) = open_db().await;
        db.with_writer(|conn| {
            seed_session(conn, "s-1", true);
            seed_kill(conn, "k1", "s-1", "Atrox");
            heal(conn)?;

            conn.execute(
                "UPDATE kill_loot_items SET deactivated_at = 9000.0 \
                 WHERE kill_id = 'k1' AND item_name = 'Animal Hide'",
                [],
            )?;
            recompute_session(conn, "s-1")?;

            let hide_cells: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_loot_rollups \
                 WHERE session_id = 's-1' AND item_name = 'Animal Hide'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(hide_cells, 0);
            let marked: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_rollup_meta WHERE session_id = 's-1'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(marked, 1);
            Ok(())
        })
        .await
        .expect("writer");
    }

    #[tokio::test]
    async fn heal_sweeps_cells_of_vanished_sessions() {
        let (_dir, db) = open_db().await;
        db.with_writer(|conn| {
            seed_session(conn, "s-gone", true);
            seed_kill(conn, "k1", "s-gone", "Atrox");
            heal(conn)?;
            // Simulate an external bulk copy that dropped the session but
            // left its cells behind.
            conn.execute("DELETE FROM kills WHERE session_id = 's-gone'", [])?;
            conn.execute("DELETE FROM tracking_sessions WHERE id = 's-gone'", [])?;
            heal(conn)?;

            let ghost: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_kill_rollups WHERE session_id = 's-gone'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(ghost, 0);
            let marker: i64 = conn.query_row(
                "SELECT COUNT(*) FROM session_rollup_meta WHERE session_id = 's-gone'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(marker, 0);
            Ok(())
        })
        .await
        .expect("writer");
    }

    #[tokio::test]
    async fn version_bump_re_serves_raw_until_healed() {
        let (_dir, db) = open_db().await;
        db.with_writer(|conn| {
            seed_session(conn, "s-1", true);
            seed_kill(conn, "k1", "s-1", "Atrox");
            heal(conn)?;
            conn.execute("UPDATE session_rollup_meta SET rollup_version = 0", [])?;
            let unsettled = unsettled_sessions(conn)?;
            assert_eq!(unsettled, vec!["s-1".to_string()]);
            heal(conn)?;
            assert!(unsettled_sessions(conn)?.is_empty());
            Ok(())
        })
        .await
        .expect("writer");
    }
}
