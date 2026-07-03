//! Materialised per-day analytics rollups: the read model behind the
//! Overview and its breakdowns. Source of truth is the raw tracking
//! tables; rows write eagerly at the mutation points and heal lazily
//! (versioned) on read, following the `session_summaries` discipline.
//! The rollup tables sit outside the DB-state snapshot catalogue, so
//! parity surfaces through the analytics HTTP responses.
//!
//! ## The model
//!
//! One `daily_rollups` row per UTC day stores each aggregate family's
//! per-day SUM verbatim (NULL when the day had no contributing rows),
//! plus `has_rows`, which decides the day's membership in the
//! timeline/monthly point sets: NULL sums erase the difference between
//! "no rows" and "rows that summed to NULL", and both wire distinctions
//! matter. `daily_ledger_rollups` carries the per-day ledger sums by
//! entry type and tag, unrounded.
//!
//! The `daily_rollup_meta.rolled_through` watermark is the single split
//! boundary: every day from the earliest data day up to and including
//! it has a rollup row (empty days included, keeping the range
//! contiguous), and readers serve anything after it from the raw
//! tables. Healing advances the watermark to
//! yesterday, so the current day is never served from rollups and every
//! day is recomputed once after it completes; a hook missed on the
//! in-flight day can therefore never freeze a stale row. Writes dated
//! at or before the watermark (a backdated ledger entry, an unclaim, a
//! loot adjustment) mark their day dirty in the writing transaction and
//! recompute it eagerly; the dirty flag survives a crash between the
//! two, and the next heal repairs it.
//!
//! Ledger dates are user-entered TEXT and may not name a canonical
//! calendar day; the heal sweeps such stray keys into rollup rows of
//! their own (ledger sums only, epoch families NULL), so the read path
//! reproduces the raw bucketing byte for byte. A key that parses but is
//! not canonically formatted (`2026-6-5`) is deliberately treated as
//! stray rather than mapped to its calendar day: giving it that day's
//! epoch windows would double-count the epoch families against the
//! canonical row.

use chrono::NaiveDate;
use sqlx::sqlite::{SqliteConnection, SqlitePool};
use sqlx::Row;

use crate::db::DbError;

/// Bump when a rollup column's meaning changes: below-version rows heal
/// on the next read.
pub const ROLLUP_VERSION: i64 = 1;

/// The UTC day of an epoch second, rendered as SQLite's
/// `date(epoch, 'unixepoch')` renders it (`YYYY-MM-DD`).
pub fn epoch_day(epoch: f64) -> String {
    chrono::DateTime::from_timestamp(epoch.floor() as i64, 0)
        .expect("epoch within chrono range")
        .format("%Y-%m-%d")
        .to_string()
}

/// The canonical calendar day named by a rollup key, or None for a
/// stray key. Round-trips the format so a parseable-but-non-canonical
/// spelling (`2026-6-5`) stays stray instead of aliasing its calendar
/// day's epoch windows.
fn canonical_day(day: &str) -> Option<NaiveDate> {
    let parsed = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    (parsed.format("%Y-%m-%d").to_string() == day).then_some(parsed)
}

/// The `[00:00, next 00:00)` UTC epoch window of a canonical day.
fn day_bounds(date: NaiveDate) -> (f64, f64) {
    let start = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    let end = start + chrono::Duration::days(1);
    (
        start.and_utc().timestamp() as f64,
        end.and_utc().timestamp() as f64,
    )
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// One `SELECT COUNT(*), SUM(a) [, SUM(b), ...]` over an epoch window,
/// returning the row count and each sum verbatim (NULL preserved).
async fn window_sums(
    conn: &mut SqliteConnection,
    sql: &'static str,
    start: f64,
    end: f64,
    sums: usize,
) -> Result<(i64, Vec<Option<f64>>), DbError> {
    let row = sqlx::query(sql)
        .bind(start)
        .bind(end)
        .fetch_one(&mut *conn)
        .await?;
    let count: i64 = row.try_get(0)?;
    let mut values = Vec::with_capacity(sums);
    for index in 0..sums {
        values.push(row.try_get::<Option<f64>, _>(index + 1)?);
    }
    Ok((count, values))
}

/// Recompute one day's rollup row and ledger rollup rows from the raw
/// tables. The caller owns the surrounding commit semantics (the write
/// hooks run this inside their transaction; the heal wraps its own).
pub async fn recompute_day(conn: &mut SqliteConnection, day: &str) -> Result<(), DbError> {
    let mut has_rows = false;
    let mut families: [Option<f64>; 9] = [None; 9];

    if let Some(date) = canonical_day(day) {
        let (start, end) = day_bounds(date);

        let (kill_count, kill_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(loot_total_ped), SUM(enhancer_cost) \
             FROM kills WHERE timestamp >= ? AND timestamp < ?",
            start,
            end,
            2,
        )
        .await?;
        let (weapon_count, weapon_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(ts.cost_per_shot * ts.shots_fired) \
             FROM kill_tool_stats ts JOIN kills k ON k.id = ts.kill_id \
             WHERE k.timestamp >= ? AND k.timestamp < ?",
            start,
            end,
            1,
        )
        .await?;
        let (session_count, session_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(armour_cost), SUM(heal_cost), SUM(dangling_cost) \
             FROM tracking_sessions WHERE started_at >= ? AND started_at < ?",
            start,
            end,
            3,
        )
        .await?;
        let (skill_count, skill_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(ped_value) \
             FROM skill_gains WHERE timestamp >= ? AND timestamp < ?",
            start,
            end,
            1,
        )
        .await?;
        let (codex_count, codex_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(ped_value) \
             FROM codex_claims WHERE claimed_at >= ? AND claimed_at < ?",
            start,
            end,
            1,
        )
        .await?;
        let (quest_count, quest_sums) = window_sums(
            conn,
            "SELECT COUNT(*), SUM(ped_value) \
             FROM quest_claims WHERE claimed_at >= ? AND claimed_at < ?",
            start,
            end,
            1,
        )
        .await?;

        families = [
            kill_sums[0],   // loot_tt
            weapon_sums[0], // weapon_cost
            kill_sums[1],   // enhancer_cost
            session_sums[0],
            session_sums[1],
            session_sums[2],
            skill_sums[0],
            codex_sums[0],
            quest_sums[0],
        ];
        has_rows = kill_count > 0
            || weapon_count > 0
            || session_count > 0
            || skill_count > 0
            || codex_count > 0
            || quest_count > 0;
    }

    sqlx::query("DELETE FROM daily_ledger_rollups WHERE day = ?")
        .bind(day)
        .execute(&mut *conn)
        .await?;
    let ledger_rows = sqlx::query(
        "INSERT INTO daily_ledger_rollups (day, entry_type, tag, amount) \
         SELECT date, type, tag, SUM(amount) FROM ledger_entries \
         WHERE date = ? GROUP BY type, tag",
    )
    .bind(day)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    has_rows = has_rows || ledger_rows > 0;

    sqlx::query(
        "INSERT OR REPLACE INTO daily_rollups (\
         day, rollup_version, dirty, has_rows, loot_tt, weapon_cost, \
         enhancer_cost, armour_cost, heal_cost, dangling_cost, skill_tt, \
         codex_pes, quest_pes, computed_at) \
         VALUES (?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, unixepoch('now'))",
    )
    .bind(day)
    .bind(ROLLUP_VERSION)
    .bind(has_rows)
    .bind(families[0])
    .bind(families[1])
    .bind(families[2])
    .bind(families[3])
    .bind(families[4])
    .bind(families[5])
    .bind(families[6])
    .bind(families[7])
    .bind(families[8])
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Mark a day's rollup row dirty (minting a stub when absent), so the
/// next heal recomputes it even if the eager recompute never runs. Run
/// inside the transaction that writes the raw rows.
pub async fn mark_day_dirty(conn: &mut SqliteConnection, day: &str) -> Result<(), DbError> {
    sqlx::query(
        "INSERT INTO daily_rollups (day, rollup_version, dirty, has_rows) \
         VALUES (?, 0, 1, 0) \
         ON CONFLICT(day) DO UPDATE SET dirty = 1",
    )
    .bind(day)
    .execute(conn)
    .await?;
    Ok(())
}

/// The write-hook entry: for each distinct day at or before the
/// watermark, mark it dirty and recompute it eagerly. Days after the
/// watermark are served raw and need nothing; before the first heal
/// there is no watermark and the backfill covers everything. The caller
/// owns the surrounding commit semantics.
pub async fn refresh_days<I, S>(conn: &mut SqliteConnection, days: I) -> Result<(), DbError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(watermark) = rolled_through(&mut *conn).await? else {
        return Ok(());
    };
    let mut seen = std::collections::BTreeSet::new();
    for day in days {
        let day = day.as_ref();
        if day <= watermark.as_str() && seen.insert(day.to_string()) {
            mark_day_dirty(&mut *conn, day).await?;
            recompute_day(&mut *conn, day).await?;
        }
    }
    Ok(())
}

async fn rolled_through(conn: &mut SqliteConnection) -> Result<Option<String>, DbError> {
    let row = sqlx::query("SELECT rolled_through FROM daily_rollup_meta WHERE id = 1")
        .fetch_optional(conn)
        .await?;
    Ok(row.map(|r| r.get(0)))
}

/// The earliest calendar day carrying raw data, or None on an empty
/// database. Ledger dates that do not name a canonical day are ignored
/// here; the stray-key sweep in [`heal_rollups`] picks their rows up.
async fn earliest_data_day(conn: &mut SqliteConnection) -> Result<Option<NaiveDate>, DbError> {
    let epoch_mins = sqlx::query(
        "SELECT MIN(t) FROM (\
         SELECT MIN(timestamp) AS t FROM kills \
         UNION ALL SELECT MIN(started_at) FROM tracking_sessions \
         UNION ALL SELECT MIN(timestamp) FROM skill_gains \
         UNION ALL SELECT MIN(claimed_at) FROM codex_claims \
         UNION ALL SELECT MIN(claimed_at) FROM quest_claims)",
    )
    .fetch_one(&mut *conn)
    .await?;
    let mut earliest = epoch_mins
        .try_get::<Option<f64>, _>(0)?
        .and_then(|epoch| canonical_day(&epoch_day(epoch)));

    let ledger_days = sqlx::query("SELECT DISTINCT date FROM ledger_entries")
        .fetch_all(&mut *conn)
        .await?;
    for row in &ledger_days {
        let Some(date) = canonical_day(row.get(0)) else {
            continue;
        };
        if earliest.is_none_or(|current| date < current) {
            earliest = Some(date);
        }
    }
    Ok(earliest)
}

/// Bring the rollups current: advance the watermark to yesterday
/// (recomputing every day it crosses, empty days included), repair
/// dirty and below-version rows, and sweep stray ledger date keys into
/// rows of their own. Returns the watermark, the split boundary the
/// reader serves rollups up to. Runs as one transaction; idempotent.
pub async fn heal_rollups(pool: &SqlitePool, now: f64) -> Result<String, DbError> {
    let mut tx = pool.begin().await?;

    let today = canonical_day(&epoch_day(now)).expect("epoch_day is canonical");
    let yesterday = today - chrono::Duration::days(1);

    let watermark = match rolled_through(&mut tx).await? {
        Some(day) => day,
        None => {
            // First heal: start the walk just before the earliest data
            // day, or collapse it entirely on an empty database.
            let start = match earliest_data_day(&mut tx).await? {
                Some(earliest) => earliest - chrono::Duration::days(1),
                None => yesterday,
            };
            let day = iso(start.min(yesterday));
            sqlx::query("INSERT INTO daily_rollup_meta (id, rolled_through) VALUES (1, ?)")
                .bind(&day)
                .execute(&mut *tx)
                .await?;
            day
        }
    };

    // Walk the watermark forward to yesterday. A watermark already at or
    // past yesterday (a clock regression) is left alone: the reader
    // serves everything after it raw, so nothing double-counts.
    let mut watermark_date = canonical_day(&watermark).expect("watermark is canonical");
    while watermark_date < yesterday {
        watermark_date += chrono::Duration::days(1);
        recompute_day(&mut tx, &iso(watermark_date)).await?;
    }
    let watermark = iso(watermark_date);
    sqlx::query("UPDATE daily_rollup_meta SET rolled_through = ? WHERE id = 1")
        .bind(&watermark)
        .execute(&mut *tx)
        .await?;

    // Repair rows a write hook marked (or a version bump staled).
    let stale = sqlx::query("SELECT day FROM daily_rollups WHERE dirty = 1 OR rollup_version < ?")
        .bind(ROLLUP_VERSION)
        .fetch_all(&mut *tx)
        .await?;
    for row in &stale {
        recompute_day(&mut tx, row.get(0)).await?;
    }

    // Stray ledger date keys (non-canonical spellings) at or before the
    // watermark get rollup rows of their own; later ones stay raw.
    let strays = sqlx::query(
        "SELECT DISTINCT date FROM ledger_entries \
         WHERE date <= ? AND date NOT IN (SELECT day FROM daily_rollups)",
    )
    .bind(&watermark)
    .fetch_all(&mut *tx)
    .await?;
    for row in &strays {
        recompute_day(&mut tx, row.get(0)).await?;
    }

    tx.commit().await?;
    Ok(watermark)
}

/// Drop and regenerate every rollup row: the proof the projection is a
/// pure function of the raw tables.
pub async fn rebuild_rollups(pool: &SqlitePool, now: f64) -> Result<String, DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM daily_rollups")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM daily_ledger_rollups")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM daily_rollup_meta")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    heal_rollups(pool, now).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    /// Fixed clock: 2001-09-09T01:46:40Z. Today is 2001-09-09; the heal
    /// watermark lands on 2001-09-08.
    const NOW: f64 = 1_000_000_000.0;
    /// Midnight UTC starting each seeded day.
    const DAY_05: f64 = 999_648_000.0; // 2001-09-05
    const DAY_07: f64 = 999_820_800.0; // 2001-09-07
    const DAY_08: f64 = 999_907_200.0; // 2001-09-08
    const DAY_09: f64 = 999_993_600.0; // 2001-09-09 (today)

    async fn pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("entropia_orme.db"))
            .await
            .unwrap();
        let pool = db.pool().clone();
        (dir, pool)
    }

    async fn run(pool: &SqlitePool, sql: &str) {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
            .execute(pool)
            .await
            .unwrap();
    }

    /// Data across the fixed calendar: a full day (09-05), an empty gap
    /// day (09-06), a NULL-sum day (09-07: only attribute gains, whose
    /// ped_value is NULL), a plain day (09-08, yesterday), today's
    /// in-flight rows (09-09), a swept stray ledger key and an unswept
    /// lexically-greater one.
    async fn seed_calendar(pool: &SqlitePool) {
        run(
            pool,
            &format!(
                "INSERT INTO tracking_sessions \
                 (id, started_at, ended_at, is_active, armour_cost, heal_cost, dangling_cost) \
                 VALUES ('s1', {}, {}, 0, 0.07, 0.11, NULL)",
                DAY_05 + 3600.0,
                DAY_05 + 10_800.0
            ),
        )
        .await;
        run(
            pool,
            &format!(
                "INSERT INTO kills (id, session_id, mob_name, timestamp, enhancer_cost, loot_total_ped) \
                 VALUES ('k1', 's1', 'Atrox', {}, 0.02, 2.0), \
                        ('k2', 's1', 'Atrox', {}, 0.02, NULL), \
                        ('k3', 's1', 'Snable', {}, 0.05, 4.5), \
                        ('k4', 's1', 'Snable', {}, 0.01, 1.5)",
                DAY_05 + 4000.0,
                DAY_05 + 5000.0,
                DAY_08 + 4000.0,
                DAY_09 + 400.0
            ),
        )
        .await;
        run(
            pool,
            "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, cost_per_shot) \
             VALUES ('k1', 'Rifle', 30, 0.05)",
        )
        .await;
        run(
            pool,
            &format!(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
                 VALUES ('s1', {}, 'Rifle', 1.0, 0.5), \
                        ('s1', {}, 'Agility', 0.25, NULL), \
                        ('s1', {}, 'Strength', 0.5, NULL)",
                DAY_05 + 4100.0,
                DAY_07 + 100.0,
                DAY_07 + 200.0
            ),
        )
        .await;
        run(
            pool,
            &format!(
                "INSERT INTO codex_claims (species_name, rank, skill_name, ped_value, claimed_at) \
                 VALUES ('Atrox', 1, 'Rifle', 1.25, {})",
                DAY_05 + 4200.0
            ),
        )
        .await;
        run(
            pool,
            &format!(
                "INSERT INTO quest_claims (quest_name, ped_value, claimed_at) \
                 VALUES ('Iron Atrox', 2.5, {})",
                DAY_05 + 4300.0
            ),
        )
        .await;
        run(
            pool,
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) VALUES \
             ('l1', '2001-09-05', 'markup', 'sale', 3.0, 'manual'), \
             ('l2', '2001-09-05', 'markup', 'sale', 2.0, 'manual'), \
             ('l3', '2001-09-05', 'expense', 'repair', 1.0, 'repair'), \
             ('l4', '2001-08-99', 'expense', 'stray but below the watermark', 7.0, 'stray'), \
             ('l5', '2001-9-2', 'markup', 'stray above the watermark, stays raw', 9.0, 'stray')",
        )
        .await;
    }

    async fn rollup_row(pool: &SqlitePool, day: &str) -> Option<sqlx::sqlite::SqliteRow> {
        sqlx::query(
            "SELECT rollup_version, dirty, has_rows, loot_tt, weapon_cost, enhancer_cost, \
             armour_cost, heal_cost, dangling_cost, skill_tt, codex_pes, quest_pes \
             FROM daily_rollups WHERE day = ?",
        )
        .bind(day)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    fn family(row: &sqlx::sqlite::SqliteRow, index: usize) -> Option<f64> {
        row.try_get::<Option<f64>, _>(index).unwrap()
    }

    #[tokio::test]
    async fn epoch_day_matches_sqlite_date_rendering() {
        let (_dir, pool) = pool().await;
        for epoch in [DAY_05, DAY_09 - 0.1, NOW, NOW + 0.5, DAY_07 + 86_399.0] {
            let sqlite: String = sqlx::query_scalar("SELECT date(?, 'unixepoch')")
                .bind(epoch)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(epoch_day(epoch), sqlite, "epoch {epoch}");
        }
    }

    /// Run a recompute on a briefly-acquired connection: the test pool
    /// is small, so holding one across the assertion reads would starve
    /// them.
    async fn recompute(pool: &SqlitePool, day: &str) {
        let mut conn = pool.acquire().await.unwrap();
        recompute_day(&mut conn, day).await.unwrap();
    }

    #[tokio::test]
    async fn recompute_day_stores_verbatim_sums_and_membership() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;

        // The full day: every family present, NULL dangling preserved.
        recompute(&pool, "2001-09-05").await;
        let row = rollup_row(&pool, "2001-09-05").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(0).unwrap(), ROLLUP_VERSION);
        assert_eq!(row.try_get::<i64, _>(1).unwrap(), 0, "not dirty");
        assert_eq!(row.try_get::<i64, _>(2).unwrap(), 1, "has rows");
        assert_eq!(family(&row, 3), Some(2.0), "loot: SUM skips the NULL");
        assert_eq!(family(&row, 4), Some(1.5), "weapon: 30 shots at 0.05");
        assert_eq!(family(&row, 5), Some(0.04));
        assert_eq!(family(&row, 6), Some(0.07));
        assert_eq!(family(&row, 7), Some(0.11));
        assert_eq!(family(&row, 8), None, "dangling: NULL sum survives");
        assert_eq!(family(&row, 9), Some(0.5));
        assert_eq!(family(&row, 10), Some(1.25));
        assert_eq!(family(&row, 11), Some(2.5));
        let ledger: Vec<(String, String, f64)> = sqlx::query_as(
            "SELECT entry_type, tag, amount FROM daily_ledger_rollups \
             WHERE day = '2001-09-05' ORDER BY entry_type, tag",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            ledger,
            [
                ("expense".into(), "repair".into(), 1.0),
                ("markup".into(), "manual".into(), 5.0),
            ]
        );

        // The empty gap day: an all-NULL row with no membership.
        recompute(&pool, "2001-09-06").await;
        let row = rollup_row(&pool, "2001-09-06").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(2).unwrap(), 0, "no rows");
        for index in 3..=11 {
            assert_eq!(family(&row, index), None);
        }

        // The attribute-only day: rows existed, so the day is a member,
        // but the sum over all-NULL ped_value stays NULL.
        recompute(&pool, "2001-09-07").await;
        let row = rollup_row(&pool, "2001-09-07").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(2).unwrap(), 1);
        assert_eq!(family(&row, 9), None, "skill_tt: NULL-sum with rows");

        // A stray key: no epoch window, ledger sums only.
        recompute(&pool, "2001-08-99").await;
        let row = rollup_row(&pool, "2001-08-99").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(2).unwrap(), 1);
        for index in 3..=11 {
            assert_eq!(family(&row, index), None);
        }
        let stray_ledger: Vec<(String, String, f64)> = sqlx::query_as(
            "SELECT entry_type, tag, amount FROM daily_ledger_rollups WHERE day = '2001-08-99'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(stray_ledger, [("expense".into(), "stray".into(), 7.0)]);
    }

    #[tokio::test]
    async fn recompute_replaces_a_days_ledger_rows() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;
        recompute(&pool, "2001-09-05").await;

        run(&pool, "DELETE FROM ledger_entries WHERE id = 'l3'").await;
        recompute(&pool, "2001-09-05").await;
        let ledger: Vec<(String, String, f64)> = sqlx::query_as(
            "SELECT entry_type, tag, amount FROM daily_ledger_rollups WHERE day = '2001-09-05'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            ledger,
            [("markup".into(), "manual".into(), 5.0)],
            "the deleted expense's row is gone, not stale"
        );
    }

    #[tokio::test]
    async fn heal_backfills_to_yesterday_and_never_today() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;

        let watermark = heal_rollups(&pool, NOW).await.unwrap();
        assert_eq!(watermark, "2001-09-08");

        let days: Vec<String> = sqlx::query_scalar("SELECT day FROM daily_rollups ORDER BY day")
            .fetch_all(&pool)
            .await
            .unwrap();
        // The walk covers earliest..yesterday contiguously (the empty
        // 09-06 included); the below-watermark stray is swept; today and
        // the lexically-greater stray are not materialised.
        assert_eq!(
            days,
            [
                "2001-08-99",
                "2001-09-05",
                "2001-09-06",
                "2001-09-07",
                "2001-09-08"
            ]
        );
        let yesterday = rollup_row(&pool, "2001-09-08").await.unwrap();
        assert_eq!(family(&yesterday, 3), Some(4.5));

        // Idempotent: a second heal changes nothing.
        let watermark = heal_rollups(&pool, NOW).await.unwrap();
        assert_eq!(watermark, "2001-09-08");
        let row = rollup_row(&pool, "2001-09-05").await.unwrap();
        assert_eq!(family(&row, 3), Some(2.0));
    }

    #[tokio::test]
    async fn heal_repairs_dirty_and_below_version_rows() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;
        heal_rollups(&pool, NOW).await.unwrap();

        run(
            &pool,
            "UPDATE daily_rollups SET loot_tt = 99.0, dirty = 1 WHERE day = '2001-09-05'",
        )
        .await;
        run(
            &pool,
            "UPDATE daily_rollups SET loot_tt = 88.0, rollup_version = 0 WHERE day = '2001-09-08'",
        )
        .await;
        heal_rollups(&pool, NOW).await.unwrap();

        let row = rollup_row(&pool, "2001-09-05").await.unwrap();
        assert_eq!(family(&row, 3), Some(2.0));
        assert_eq!(row.try_get::<i64, _>(1).unwrap(), 0);
        let row = rollup_row(&pool, "2001-09-08").await.unwrap();
        assert_eq!(family(&row, 3), Some(4.5));
        assert_eq!(row.try_get::<i64, _>(0).unwrap(), ROLLUP_VERSION);
    }

    #[tokio::test]
    async fn a_dirty_stub_from_marking_heals_into_a_full_row() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;
        heal_rollups(&pool, NOW).await.unwrap();

        // A crash between the mark and the eager recompute leaves only
        // the stub; the next heal completes it.
        run(&pool, "DELETE FROM daily_rollups WHERE day = '2001-09-05'").await;
        {
            let mut conn = pool.acquire().await.unwrap();
            mark_day_dirty(&mut conn, "2001-09-05").await.unwrap();
        }
        let stub = rollup_row(&pool, "2001-09-05").await.unwrap();
        assert_eq!(stub.try_get::<i64, _>(1).unwrap(), 1, "dirty");
        assert_eq!(stub.try_get::<i64, _>(0).unwrap(), 0, "pre-version");

        heal_rollups(&pool, NOW).await.unwrap();
        let row = rollup_row(&pool, "2001-09-05").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(1).unwrap(), 0);
        assert_eq!(family(&row, 3), Some(2.0));
    }

    #[tokio::test]
    async fn refresh_days_respects_the_watermark() {
        let (_dir, pool) = pool().await;

        // Before any heal there is no watermark: a refresh is a no-op.
        seed_calendar(&pool).await;
        {
            let mut conn = pool.acquire().await.unwrap();
            refresh_days(&mut conn, ["2001-09-05"]).await.unwrap();
        }
        assert!(rollup_row(&pool, "2001-09-05").await.is_none());

        heal_rollups(&pool, NOW).await.unwrap();

        // A backdated ledger write, then the hook: the day recomputes
        // eagerly; today (beyond the watermark) is ignored.
        run(
            &pool,
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES ('l9', '2001-09-06', 'expense', 'backdated', 2.5, 'manual')",
        )
        .await;
        {
            let mut conn = pool.acquire().await.unwrap();
            refresh_days(&mut conn, ["2001-09-06", "2001-09-09"])
                .await
                .unwrap();
        }
        let row = rollup_row(&pool, "2001-09-06").await.unwrap();
        assert_eq!(row.try_get::<i64, _>(1).unwrap(), 0, "recomputed, clean");
        assert_eq!(row.try_get::<i64, _>(2).unwrap(), 1, "ledger row joined");
        assert!(rollup_row(&pool, "2001-09-09").await.is_none());
    }

    #[tokio::test]
    async fn rebuild_regenerates_identical_content() {
        let (_dir, pool) = pool().await;
        seed_calendar(&pool).await;
        heal_rollups(&pool, NOW).await.unwrap();

        type RollupRow = (String, i64, i64, i64, Option<f64>, Option<f64>);
        let snapshot = |pool: SqlitePool| async move {
            let rollups: Vec<RollupRow> = sqlx::query_as(
                "SELECT day, rollup_version, dirty, has_rows, loot_tt, skill_tt \
                 FROM daily_rollups ORDER BY day",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            let ledger: Vec<(String, String, String, f64)> = sqlx::query_as(
                "SELECT day, entry_type, tag, amount FROM daily_ledger_rollups \
                 ORDER BY day, entry_type, tag",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            (rollups, ledger)
        };
        let before = snapshot(pool.clone()).await;

        run(
            &pool,
            "UPDATE daily_rollups SET loot_tt = 77.0, has_rows = 0 WHERE day = '2001-09-05'",
        )
        .await;
        let watermark = rebuild_rollups(&pool, NOW).await.unwrap();
        assert_eq!(watermark, "2001-09-08");
        assert_eq!(snapshot(pool.clone()).await, before);
    }
}
