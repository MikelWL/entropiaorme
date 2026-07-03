//! The persistence base: one SQLite database behind a narrow handle.
//!
//! Design decisions, mirroring the original Python implementation and the porting
//! references:
//!
//! - **Writer/reader split**: one dedicated writer connection serialises
//!   every write in-process, and a small reader pool serves reads
//!   concurrently against the WAL, so a live write stream no longer
//!   stalls dashboard reads. Callers pick the pool by intent
//!   ([`Db::read`] for `SELECT`, [`Db::write`] for mutations); no other
//!   module reaches a raw pool. (The original single-owner pool-of-one,
//!   a faithful transcription of the backend's shared connection, was
//!   the benchmark-justified renovation point once real databases
//!   outgrew it; the split is response-invariant, re-validated against
//!   the DB-state goldens.)
//! - **Session configuration**: WAL journal, NORMAL synchronous, a
//!   5-second busy timeout, and a 64 MB page cache per connection.
//! - **Schema baseline**: the migration chain starts at the schema the
//!   backend creates on a fresh install (version 33), statement text
//!   verbatim, so a freshly-migrated native database is
//!   `sqlite_master`-identical to a freshly-created backend one.
//! - **Adoption over re-creation**: opening an existing database that
//!   the backend has already migrated to version 33 marks the baseline
//!   as applied without running any DDL. Databases on older schema
//!   versions are refused: the backend process owns their upgrade for as
//!   long as it ships, and the pre-baseline upgrade chain moves natively
//!   only when that ownership ends.
//!
//! No driver type escapes this module's API: callers see [`Db`],
//! [`DbError`], and plain data.

//!
//! Queries here are runtime-prepared (`sqlx::query`), not compile-time
//! checked macros: the snapshot catalogue composes its SQL from
//! constants, so an offline statement cache has nothing to hold. If a
//! compile-time-checked query (`sqlx::query!`) ever lands in this
//! workspace, wire `cargo sqlx prepare` and the committed `.sqlx`
//! cache into CI in the same change.

use std::path::Path;
use std::time::Duration;

use serde_json::{Map, Value};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::Row;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// The schema version the baseline migration reproduces.
const BASELINE_SCHEMA_VERSION: i64 = 33;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// The on-disk schema predates the supported baseline; the backend
    /// process upgrades it on its own launch.
    #[error("database schema version {found} predates the supported baseline {supported}")]
    UnsupportedSchemaVersion { found: i64, supported: i64 },
    /// Any driver failure.
    #[error(transparent)]
    Driver(#[from] sqlx::Error),
    /// A migration failure.
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    /// A stored value that does not decode into its domain shape.
    #[error("{context}: {source}")]
    Decode {
        context: &'static str,
        #[source]
        source: serde_json::Error,
    },
    /// A snapshot read met a column type outside the emitter's catalogue.
    #[error("unsupported value type {type_name} in column {column}")]
    UnsupportedValueType { type_name: String, column: String },
}

/// The composition-root open outcome over an application database (see
/// [`Db::open_adopted`]): a pre-existing database that cannot be
/// adopted quarantines (native arm stands down, file untouched), while
/// a failure with no prior file is an ordinary environment error.
#[derive(Debug, thiserror::Error)]
pub enum AdoptError {
    #[error(
        "existing database at {} cannot be adopted ({source}); native services stand \
         down and the file is left untouched for diagnosis",
        path.display()
    )]
    Quarantined {
        path: std::path::PathBuf,
        source: DbError,
    },
    #[error(transparent)]
    Fresh(DbError),
}

impl AdoptError {
    /// True when the decline is "the existing database is below the adoptable
    /// baseline and below the rung the native upgrade bridges"
    /// ([`DbError::UnsupportedSchemaVersion`]). The native first-launch upgrade
    /// ([`upgrade_to_baseline`]) carries the one in-the-wild rung (v32 -> v33),
    /// so a v32 database adopts cleanly and never surfaces here; only older
    /// schemas, which no database in the wild occupies, reach this. The
    /// composition root treats it as a terminal decline, exactly as it treats
    /// every other decline (a corrupt file, a driver fault).
    pub fn is_below_baseline(&self) -> bool {
        matches!(
            self,
            AdoptError::Quarantined {
                source: DbError::UnsupportedSchemaVersion { .. },
                ..
            }
        )
    }
}

/// Decode a numeric aggregate that SQLite may hand back as INTEGER
/// (a `SUM`/`COALESCE` expression result keeps the integer type even
/// over REAL-affinity columns). Only a value that decodes as no
/// number at all (NULL, text) falls back to zero; a structural
/// failure (a missing column) is a programming error and panics
/// rather than silently zeroing an analytic.
pub(crate) fn decoded_f64(row: &sqlx::sqlite::SqliteRow, index: usize) -> f64 {
    use sqlx::Row as _;
    row.try_get::<f64, _>(index)
        .or_else(|_| row.try_get::<i64, _>(index).map(|value| value as f64))
        .unwrap_or_else(|error| match error {
            sqlx::Error::ColumnDecode { .. } => 0.0,
            other => panic!("decoded_f64 column {index}: {other}"),
        })
}

/// The number of reader connections. SQLite in WAL mode serves many
/// concurrent readers against one writer; a handful is ample for a
/// desktop app's dashboard, and keeps the page-cache footprint bounded.
const READER_POOL_SIZE: u32 = 4;

/// The page cache each connection may grow to, in KiB (the leading `-`
/// is SQLite's "kibibytes, not pages" sign): 64 MB, up from the original
/// 8 MB, for a database heading past a gigabyte. Applied to every
/// connection in both pools; pages are demand-allocated up to the limit,
/// so the resident cost tracks real working set, not the ceiling.
const CACHE_SIZE_KIB: &str = "-64000";

/// The application database handle. Cloning shares the underlying pools
/// (the composition root still opens the database exactly once); a clone
/// is a handle, never a second owner.
///
/// Reads and writes travel separate pools: one dedicated writer
/// connection serialises every write in-process (so two writers queue at
/// the pool rather than colliding on SQLite's single-writer lock), while
/// a small reader pool serves dashboard reads concurrently against the
/// WAL. This is what stops a live write stream from stalling reads. See
/// [`Db::read`] and [`Db::write`].
#[derive(Debug, Clone)]
pub struct Db {
    /// The single write connection. Every statement that mutates the
    /// database (including write transactions and the migration chain)
    /// runs here, so writes serialise through one owner.
    writer: SqlitePool,
    /// The reader pool. Plain reads run here, concurrently with the
    /// writer under WAL, so a dashboard GET does not wait behind combat
    /// writes.
    reader: SqlitePool,
}

impl Db {
    /// The reader pool, for plain reads (`SELECT` / `fetch_*`). Reads on
    /// this pool run concurrently with the writer under WAL.
    pub fn read(&self) -> &SqlitePool {
        &self.reader
    }

    /// The writer pool (a single connection), for every mutating
    /// statement and every write transaction (`execute`, `begin`). All
    /// writes serialise here.
    pub fn write(&self) -> &SqlitePool {
        &self.writer
    }

    /// Transitional alias returning the writer pool, so call sites not yet
    /// routed to [`Db::read`]/[`Db::write`] keep compiling (and keep the
    /// original single-owner behaviour) during the migration. Removed once
    /// every caller is routed; no production caller should remain on it.
    #[doc(hidden)]
    pub fn pool(&self) -> &SqlitePool {
        &self.writer
    }

    /// Checkpoint the WAL and truncate it to zero, bounding WAL growth
    /// over a long-running session. Runs on the writer (a checkpoint is a
    /// write operation). `TRUNCATE` blocks until it can reset the log,
    /// which is the intended behaviour at a quiescent boundary (session
    /// end), not on a hot path.
    pub async fn checkpoint_truncate(&self) -> Result<(), DbError> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(self.write())
            .await?;
        Ok(())
    }

    /// Rebind a handle over an already-opened pool, used as both the
    /// reader and the writer. The composition root opens the application
    /// database via [`Db::open`] (which builds the genuine writer/reader
    /// split); this exists for harnesses attaching to a database another
    /// process created and migrated, and for in-memory test pools that
    /// cannot span two pools (each `:memory:` connection is its own
    /// database). Sharing one pool for both roles reproduces the original
    /// single-owner behaviour exactly, which is what those harnesses want.
    pub fn from_pool(pool: SqlitePool) -> Db {
        Db {
            writer: pool.clone(),
            reader: pool,
        }
    }

    /// The connect options shared by both pools: WAL, NORMAL sync, a
    /// five-second busy timeout, foreign keys off (matching the backend's
    /// effective pragma surface, where `REFERENCES` clauses are
    /// declarative), and the 64 MB page cache.
    fn connect_options(path: &Path) -> SqliteConnectOptions {
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(false)
            .pragma("cache_size", CACHE_SIZE_KIB)
    }

    /// Open (creating if missing), adopt or refuse an existing schema,
    /// and bring the migration chain up to date.
    ///
    /// The writer pool is built and migrated first; the reader pool is
    /// opened only after the schema is current, so a reader connection
    /// never observes a pre-migration database (reader connections are
    /// lazy, but ordering the build removes any doubt).
    pub async fn open(path: &Path) -> Result<Db, DbError> {
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(Self::connect_options(path))
            .await?;
        adopt_or_refuse(&writer).await?;
        MIGRATOR.run(&writer).await?;
        let reader = SqlitePoolOptions::new()
            .max_connections(READER_POOL_SIZE)
            .connect_with(Self::connect_options(path))
            .await?;
        Ok(Db { writer, reader })
    }

    /// Open the application's own database at the composition root.
    ///
    /// Distinguishes failure on a PRE-EXISTING database from failure on
    /// a fresh path: an existing file that cannot be adopted or
    /// migrated is a quarantine signal, not a bare error. The file is
    /// left exactly as found (it is the user's data); the caller declines
    /// composition and surfaces the condition loudly.
    pub async fn open_adopted(path: &Path) -> Result<Db, AdoptError> {
        let pre_existing = path.exists();
        match Db::open(path).await {
            Ok(db) => Ok(db),
            Err(source) if pre_existing => Err(AdoptError::Quarantined {
                path: path.to_path_buf(),
                source,
            }),
            Err(source) => Err(AdoptError::Fresh(source)),
        }
    }

    /// The catalogue rows for the DB-state snapshot, each table in its
    /// deterministic order, shaped for the snapshot emitter.
    pub async fn snapshot_rows(&self) -> Result<Map<String, Value>, DbError> {
        snapshot_rows(self.read()).await
    }

    /// One equipment-library row by id and item type: (id, name,
    /// properties JSON), or None when absent. The typed accessor the
    /// trifecta resolution reads through.
    pub async fn equipment_item(
        &self,
        id: i64,
        item_type: &str,
    ) -> Result<Option<(i64, String, String)>, DbError> {
        let row = sqlx::query_as::<_, (i64, String, String)>(
            "SELECT id, name, properties_json FROM equipment_library \
             WHERE id = ? AND item_type = ?",
        )
        .bind(id)
        .bind(item_type)
        .fetch_optional(self.read())
        .await?;
        Ok(row)
    }

    /// One equipment-library row by id alone: `(name, item_type, properties
    /// JSON)`, or None when absent. The hotbar resolver reads it to branch on
    /// the item type the slot's bound id resolves to (mirroring the backend's
    /// `SELECT id, name, item_type FROM equipment_library WHERE id = ?`, with
    /// the properties carried so the healing branch reads them without a
    /// second query).
    pub async fn hotbar_equipment_row(
        &self,
        id: i64,
    ) -> Result<Option<(String, String, String)>, DbError> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT name, item_type, properties_json FROM equipment_library \
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.read())
        .await?;
        Ok(row)
    }

    /// The first weapon-row `properties_json` whose name contains the
    /// supplied fragment, ported from the backend's
    /// `_equipment_profile_lookup`: a `LIKE '%fragment%'` over weapon
    /// rows, with the fragment's own `%` / `_` / `\` escaped (so an
    /// embedded wildcard cannot widen the match) under an explicit
    /// `ESCAPE '\'`. The fragment is trimmed exactly as the backend
    /// trims it before the query.
    pub async fn weapon_properties_by_name_fragment(
        &self,
        fragment: &str,
    ) -> Result<Option<String>, DbError> {
        let safe = fragment
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT properties_json FROM equipment_library \
             WHERE item_type = 'weapon' AND name LIKE ? ESCAPE '\\'",
        )
        .bind(format!("%{safe}%"))
        .fetch_optional(self.read())
        .await?;
        Ok(row.map(|(properties_json,)| properties_json))
    }

    /// Test seeding for equipment-reading services (compiled into the
    /// crate's own test builds only).
    #[cfg(test)]
    pub(crate) async fn insert_equipment_for_tests(
        &self,
        id: i64,
        name: &str,
        item_type: &str,
        properties_json: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO equipment_library (id, name, item_type, properties_json) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(item_type)
        .bind(properties_json)
        .execute(self.write())
        .await?;
        Ok(())
    }

    /// The schema objects as (type, name, statement) in (type, name)
    /// order, excluding SQLite's own bookkeeping tables: the surface the
    /// schema-conformance acceptance compares across implementations.
    pub async fn schema_master(&self) -> Result<Vec<(String, String, String)>, DbError> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL \
             AND name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .fetch_all(self.read())
        .await?;
        Ok(rows)
    }
}

/// Mark the baseline as applied on a database the backend has already
/// created at the baseline version; refuse older schemas.
async fn adopt_or_refuse(pool: &SqlitePool) -> Result<(), DbError> {
    let has_metadata = table_exists(pool, "db_metadata").await?;
    if !has_metadata {
        // A fresh (or empty) database: the migration chain owns it.
        return Ok(());
    }
    if table_exists(pool, "_sqlx_migrations").await? {
        // Already adopted (or natively created); the chain validates.
        return Ok(());
    }
    let version: Option<String> =
        sqlx::query_scalar("SELECT value FROM db_metadata WHERE key = 'version'")
            .fetch_optional(pool)
            .await?;
    let version: i64 = version.and_then(|raw| raw.parse().ok()).unwrap_or_default();

    // Upgrade-and-adopt as one transaction: the in-place bridge (below) and the
    // baseline stamp commit together or not at all. A failure in either rolls
    // back the file to exactly as it was found, honouring the `open_adopted`
    // "left untouched on a decline" contract; without this, a stamp failure
    // after the bridge mutated the file would leave a half-upgraded database.
    let mut tx = pool.begin().await?;
    if version < BASELINE_SCHEMA_VERSION {
        // A below-baseline database the backend process now owns: the
        // co-bundled Python sidecar that used to migrate it forward to the
        // baseline on the first launch after an upgrade is gone, so the
        // upgrade runs natively here, in place, before the baseline is
        // stamped. Only the single rung an in-the-wild v0.1.0-lineage
        // database occupies is bridged; older schemas stay a refusal.
        upgrade_to_baseline(&mut tx, version).await?;
    }

    // The ledger row sqlx's own runner would have written had it created
    // the schema; the post-adoption `MIGRATOR.run` validates it (version
    // and checksum), so drift in this DDL or the row fails loudly.
    let baseline = MIGRATOR
        .migrations
        .first()
        .expect("the migration chain carries the baseline");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
         version BIGINT PRIMARY KEY, description TEXT NOT NULL, \
         installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, \
         success BOOLEAN NOT NULL, checksum BLOB NOT NULL, \
         execution_time BIGINT NOT NULL)",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
         VALUES (?, ?, TRUE, ?, 0)",
    )
    .bind(baseline.version)
    .bind(baseline.description.as_ref())
    .bind(baseline.checksum.as_ref())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Upgrade a below-baseline database up to the adoptable baseline in place,
/// then return so [`adopt_or_refuse`] stamps the baseline ledger row over the
/// now-current schema.
///
/// Only the rung the real v0.1.0-lineage database occupies is implemented:
///
/// - **v32 -> v33**: drop the unused `tt_curve_observations` table. It was
///   write-only in the v32 surface (the skill tracker wrote a row on every
///   suppressed codex skill gain, but no read path ever consumed it), so the
///   drop is lossless. This mirrors the Python ladder's v33 step, and leaves
///   the database where a freshly-created v33 one sits: no
///   `tt_curve_observations`, `db_metadata.version = 33`.
///
/// Anything below v32 is refused exactly as before. No database in the wild
/// occupies those versions, so the earlier rungs are deliberately not ported;
/// pinning the bridge to v32 (rather than `BASELINE_SCHEMA_VERSION - 1`) keeps
/// the rung's DDL coupled to the concrete v32 -> v33 delta it actually applies.
/// Runs inside [`adopt_or_refuse`]'s adopt transaction, so the bridge and the
/// subsequent baseline stamp share one commit boundary (a failure leaves the
/// file untouched).
async fn upgrade_to_baseline(
    conn: &mut sqlx::sqlite::SqliteConnection,
    version: i64,
) -> Result<(), DbError> {
    /// The one below-baseline schema version with a native upgrade path.
    const BRIDGEABLE_VERSION: i64 = 32;
    if version != BRIDGEABLE_VERSION {
        return Err(DbError::UnsupportedSchemaVersion {
            found: version,
            supported: BASELINE_SCHEMA_VERSION,
        });
    }
    // v33 rung: drop the retired write-only observations table.
    sqlx::query("DROP TABLE IF EXISTS tt_curve_observations")
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE db_metadata SET value = ? WHERE key = 'version'")
        .bind(BASELINE_SCHEMA_VERSION.to_string())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, DbError> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

/// One row as the snapshot emitter expects it: column-ordered keys, JSON
/// values typed by the stored value (integer, real, text, null).
fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Result<Value, DbError> {
    use sqlx::{Column, TypeInfo, ValueRef};
    let mut object = Map::new();
    for column in row.columns() {
        let raw = row.try_get_raw(column.ordinal())?;
        let value = if raw.is_null() {
            Value::Null
        } else {
            match raw.type_info().name() {
                "INTEGER" | "BOOLEAN" => Value::from(row.try_get::<i64, _>(column.ordinal())?),
                "REAL" => Value::from(row.try_get::<f64, _>(column.ordinal())?),
                "TEXT" => Value::from(row.try_get::<String, _>(column.ordinal())?),
                other => {
                    return Err(DbError::UnsupportedValueType {
                        type_name: other.to_string(),
                        column: column.name().to_string(),
                    })
                }
            }
        };
        object.insert(column.name().to_string(), value);
    }
    Ok(Value::Object(object))
}

/// Execute the snapshot catalogue: each table's query with its
/// deterministic ORDER BY, exactly as the catalogue documents.
async fn snapshot_rows(pool: &SqlitePool) -> Result<Map<String, Value>, DbError> {
    let mut tables = Map::new();
    for spec in eo_wire::db_snapshot::CATALOGUE {
        // Composed exclusively from the catalogue's compile-time constants
        // (no caller input reaches this string), so the safety assertion
        // is genuine rather than a lint bypass.
        let sql = format!("{} ORDER BY {}", spec.query, spec.order_by.join(", "));
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await?;
        let mut json_rows = Vec::with_capacity(rows.len());
        for row in &rows {
            json_rows.push(row_to_json(row)?);
        }
        tables.insert(spec.name.to_string(), Value::Array(json_rows));
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fresh_db(dir: &std::path::Path) -> Db {
        Db::open(&dir.join("entropia_orme.db")).await.unwrap()
    }

    /// A database exactly as a version-33 backend leaves it: the baseline
    /// schema and version row, with no sqlx migration ledger and none of the
    /// post-baseline migration objects. Built by running the baseline
    /// migration SQL directly rather than the full native chain, so an
    /// adoption test then exercises the post-baseline chain against a genuine
    /// baseline, and the helper stays correct as later migrations are added.
    async fn backend_baseline_pool(path: &std::path::Path) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/0001_schema_baseline.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn hotbar_equipment_row_reads_name_type_and_properties_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;
        db.insert_equipment_for_tests(7, "Healer", "healing", r#"{"tool_entity":{"x":1}}"#)
            .await
            .unwrap();
        db.insert_equipment_for_tests(8, "Opalo", "weapon", r#"{"weapon_entity":{}}"#)
            .await
            .unwrap();

        assert_eq!(
            db.hotbar_equipment_row(7).await.unwrap(),
            Some((
                "Healer".to_string(),
                "healing".to_string(),
                r#"{"tool_entity":{"x":1}}"#.to_string(),
            )),
        );
        assert_eq!(
            db.hotbar_equipment_row(8).await.unwrap(),
            Some((
                "Opalo".to_string(),
                "weapon".to_string(),
                r#"{"weapon_entity":{}}"#.to_string(),
            )),
        );
        // An absent id yields None.
        assert_eq!(db.hotbar_equipment_row(999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn weapon_profile_lookup_matches_on_a_fragment_and_escapes_wildcards() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;
        db.insert_equipment_for_tests(1, "ArMatrix LR-35", "weapon", r#"{"weapon_entity":{}}"#)
            .await
            .unwrap();
        db.insert_equipment_for_tests(2, "Healer", "healing", r#"{"tool_entity":{}}"#)
            .await
            .unwrap();
        db.insert_equipment_for_tests(
            3,
            "100% Plain Name",
            "weapon",
            r#"{"weapon_entity":{"id":3}}"#,
        )
        .await
        .unwrap();

        // A fragment matches the weapon row.
        let found = db
            .weapon_properties_by_name_fragment("LR-35")
            .await
            .unwrap();
        assert_eq!(found.as_deref(), Some(r#"{"weapon_entity":{}}"#));

        // Healing rows are never returned (weapon-only filter).
        let absent = db
            .weapon_properties_by_name_fragment("Healer")
            .await
            .unwrap();
        assert_eq!(absent, None);

        // A literal `%` in the fragment is escaped: it matches the row
        // whose name actually contains `%`, not every row.
        let percent = db.weapon_properties_by_name_fragment("100%").await.unwrap();
        assert_eq!(percent.as_deref(), Some(r#"{"weapon_entity":{"id":3}}"#));
        // A bare wildcard, were it unescaped, would match everything;
        // escaped, it matches nothing because no name contains a literal
        // percent-followed-by-space-P beyond row 3, and the leading `%`
        // here is a literal.
        let only_literal = db
            .weapon_properties_by_name_fragment("%Plain")
            .await
            .unwrap();
        assert_eq!(
            only_literal, None,
            "the leading % is a literal, not a wildcard"
        );
    }

    #[tokio::test]
    async fn fresh_database_migrates_with_session_pragmas_in_effect() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;

        // The session pragmas hold on BOTH pools: the split configures the
        // writer and the reader connections identically at connect time.
        for (label, pool) in [("writer", db.write()), ("reader", db.read())] {
            let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(journal, "wal", "{label} journal_mode");
            let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(synchronous, 1, "{label} synchronous NORMAL");
            let cache: i64 = sqlx::query_scalar("PRAGMA cache_size")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(cache, -64000, "{label} cache_size 64 MB");
            let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(
                foreign_keys, 0,
                "{label}: referential enforcement stays off, matching the backend's pragma surface"
            );
            let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(busy, 5000, "{label} busy_timeout");
        }
    }

    #[tokio::test]
    async fn checkpoint_truncate_resets_the_wal_and_keeps_data_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        let db = Db::open(&path).await.unwrap();

        // Grow the WAL with a batch of committed writes.
        for i in 0..200 {
            sqlx::query("INSERT INTO tracking_sessions (id, started_at, is_active) VALUES (?, ?, 0)")
                .bind(format!("s-{i}"))
                .bind(i as f64)
                .execute(db.write())
                .await
                .unwrap();
        }
        let wal = path.with_extension("db-wal");
        let grown = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
        assert!(grown > 0, "the WAL should carry frames before the checkpoint");

        db.checkpoint_truncate().await.unwrap();

        // TRUNCATE resets the log to zero bytes (no reader held a snapshot).
        let after = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
        assert_eq!(after, 0, "wal_checkpoint(TRUNCATE) empties the WAL");

        // The committed data is intact and readable through the reader pool.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracking_sessions")
            .fetch_one(db.read())
            .await
            .unwrap();
        assert_eq!(count, 200);
    }

    #[tokio::test]
    async fn baseline_creates_the_full_schema_surface_and_version_row() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;

        let count = |kind: &'static str| {
            let pool = db.write().clone();
            async move {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = ? AND sql IS NOT NULL \
                     AND name != '_sqlx_migrations' AND name NOT LIKE 'sqlite_%'",
                )
                .bind(kind)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        // The fresh backend schema at version 33 (23 declared tables;
        // sqlite_sequence arrives automatically) plus the daily-rollup
        // migration's three projection tables, and the index migrations:
        // 18 baseline + 4 analytical + the ledger date index = 23 indexes,
        // 8 triggers.
        assert_eq!(count("table").await, 26);
        assert_eq!(count("index").await, 23);
        assert_eq!(count("trigger").await, 8);

        let version: String =
            sqlx::query_scalar("SELECT value FROM db_metadata WHERE key = 'version'")
                .fetch_one(db.write())
                .await
                .unwrap();
        assert_eq!(version, "33");
    }

    #[tokio::test]
    async fn empty_database_snapshot_yields_every_catalogue_table_empty() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;
        let rows = db.snapshot_rows().await.unwrap();
        assert_eq!(rows.len(), eo_wire::db_snapshot::CATALOGUE.len());
        for (table, value) in &rows {
            assert_eq!(
                value.as_array().map(Vec::len),
                Some(0),
                "{table} should be empty"
            );
        }
    }

    #[tokio::test]
    async fn snapshot_rows_carry_typed_values_in_deterministic_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;
        sqlx::query(
            "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES \
             ('s-2', 200.0, 0), ('s-1', 100.0, 1)",
        )
        .execute(db.write())
        .await
        .unwrap();

        let rows = db.snapshot_rows().await.unwrap();
        let sessions = rows["tracking_sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2);
        // rowid order: insertion order, not id order.
        assert_eq!(sessions[0]["id"], "s-2");
        assert_eq!(sessions[0]["started_at"], 200.0);
        assert_eq!(sessions[0]["is_active"], 0);
        assert_eq!(sessions[0]["heal_cost"], 0.0, "COALESCE default");
        assert_eq!(sessions[1]["id"], "s-1");
    }

    #[tokio::test]
    async fn backend_created_baseline_database_is_adopted_with_data_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        // A backend-created version-33 database: the baseline schema, seeded,
        // with no sqlx ledger (exactly what the backend leaves).
        {
            let pool = backend_baseline_pool(&path).await;
            sqlx::query(
                "INSERT INTO tracking_sessions (id, started_at, is_active) \
                 VALUES ('kept', 1.0, 0)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

        // Re-open: adoption marks the baseline applied without DDL, then the
        // post-baseline chain runs, and the migrator validates every ledger row.
        let db = Db::open(&path).await.unwrap();
        let kept: String = sqlx::query_scalar("SELECT id FROM tracking_sessions")
            .fetch_one(db.write())
            .await
            .unwrap();
        assert_eq!(kept, "kept");
        let ledger: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(db.write())
            .await
            .unwrap();
        // The baseline stamp plus every post-baseline migration.
        assert_eq!(ledger, MIGRATOR.migrations.len() as i64);
    }

    #[tokio::test]
    async fn pre_baseline_schema_versions_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        {
            let db = Db::open(&path).await.unwrap();
            sqlx::query(
                "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                 VALUES ('keep-me', '2026-01-01', 'markup', 'survives refusal', 1.25, 'manual')",
            )
            .execute(db.write())
            .await
            .unwrap();
            sqlx::query("UPDATE db_metadata SET value = '28' WHERE key = 'version'")
                .execute(db.write())
                .await
                .unwrap();
            sqlx::query("DROP TABLE _sqlx_migrations")
                .execute(db.write())
                .await
                .unwrap();
        }
        let err = Db::open(&path).await.unwrap_err();
        match err {
            DbError::UnsupportedSchemaVersion { found, supported } => {
                assert_eq!((found, supported), (28, 33));
            }
            other => panic!("expected a schema-version refusal, got {other}"),
        }

        // The refusal is lossless: the user's rows are untouched (the
        // connect-time pragmas may legitimately convert the journal
        // mode, so the assertion is content-level, not byte-level).
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let (description, amount): (String, f64) =
            sqlx::query_as("SELECT description, amount FROM ledger_entries WHERE id = 'keep-me'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(description, "survives refusal");
        assert_eq!(amount, 1.25);
        let version: String =
            sqlx::query_scalar("SELECT value FROM db_metadata WHERE key = 'version'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, "28", "the stamp is left for the upgrade owner");
    }

    #[tokio::test]
    async fn below_baseline_v32_database_upgrades_to_the_baseline_with_data_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        // Synthesise the real in-the-wild surface: a v0.1.0-lineage database
        // last owned by the Python backend at version 32. Start from the
        // backend baseline (v33 schema, no sqlx ledger), then walk it back to
        // v32: re-create the table v33 dropped (with a row, to prove the drop
        // is the only loss) and stamp the version row back to 32.
        {
            let pool = backend_baseline_pool(&path).await;
            sqlx::query(
                "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
                 VALUES ('keep-me', '2026-01-01', 'markup', 'survives upgrade', 4.2, 'manual')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("CREATE TABLE tt_curve_observations (id INTEGER PRIMARY KEY, value REAL)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO tt_curve_observations (value) VALUES (1.0)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("UPDATE db_metadata SET value = '32' WHERE key = 'version'")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Re-open: the v32 rung runs, then adoption stamps the baseline and the
        // post-adoption migrator validates the ledger. No refusal.
        let db = Db::open(&path).await.unwrap();

        // The retired table is gone, and the version row now matches a fresh
        // v33 database.
        assert!(
            !table_exists(db.write(), "tt_curve_observations")
                .await
                .unwrap(),
            "the v33 rung drops tt_curve_observations"
        );
        let version: String =
            sqlx::query_scalar("SELECT value FROM db_metadata WHERE key = 'version'")
                .fetch_one(db.write())
                .await
                .unwrap();
        assert_eq!(
            version, "33",
            "the upgrade bumps the version row to the baseline"
        );
        let ledger: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(db.write())
            .await
            .unwrap();
        assert_eq!(
            ledger,
            MIGRATOR.migrations.len() as i64,
            "the baseline is stamped once, then the post-baseline chain runs"
        );

        // The user's data survives the upgrade untouched.
        let (description, amount): (String, f64) =
            sqlx::query_as("SELECT description, amount FROM ledger_entries WHERE id = 'keep-me'")
                .fetch_one(db.write())
                .await
                .unwrap();
        assert_eq!((description.as_str(), amount), ("survives upgrade", 4.2));
    }

    #[tokio::test]
    async fn schema_versions_below_the_v32_bridge_are_still_refused() {
        // The bridge is the single v32 rung; v31 (and anything older) remains a
        // terminal refusal, with the user's rows left untouched.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        {
            let db = Db::open(&path).await.unwrap();
            sqlx::query("UPDATE db_metadata SET value = '31' WHERE key = 'version'")
                .execute(db.write())
                .await
                .unwrap();
            sqlx::query("DROP TABLE _sqlx_migrations")
                .execute(db.write())
                .await
                .unwrap();
        }
        let err = Db::open(&path).await.unwrap_err();
        match err {
            DbError::UnsupportedSchemaVersion { found, supported } => {
                assert_eq!((found, supported), (31, 33));
            }
            other => panic!("expected a schema-version refusal, got {other}"),
        }
    }

    #[tokio::test]
    async fn schema_master_lists_the_real_objects_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path()).await;
        let master = db.schema_master().await.unwrap();
        // 26 declared tables (23 baseline + 3 daily-rollup projection) +
        // the migration ledger + 23 indexes (18 baseline + 4 analytical +
        // the ledger date index) + 8 triggers (only SQLite's own
        // bookkeeping is excluded; the conformance comparison filters the
        // ledger externally as its one deliberate difference).
        assert_eq!(master.len(), 27 + 23 + 8);
        let mut sorted = master.clone();
        sorted.sort();
        assert_eq!(master, sorted, "ordered by (type, name)");
        assert!(master.iter().any(|(kind, name, sql)| {
            kind == "table"
                && name == "tracking_sessions"
                && sql.contains("CREATE TABLE tracking_sessions")
        }));
        assert!(master.iter().any(|(_, name, _)| name == "_sqlx_migrations"));
    }

    #[test]
    fn refusal_error_formats_the_exact_message() {
        let err = DbError::UnsupportedSchemaVersion {
            found: 28,
            supported: 33,
        };
        assert_eq!(
            err.to_string(),
            "database schema version 28 predates the supported baseline 33"
        );
    }

    #[tokio::test]
    async fn open_adopted_succeeds_on_fresh_and_healthy_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        // Fresh path: created and migrated.
        let db = Db::open_adopted(&path).await.unwrap();
        drop(db);
        // Healthy pre-existing database: adopted.
        Db::open_adopted(&path).await.unwrap();
    }

    #[tokio::test]
    async fn open_adopted_quarantines_an_unadoptable_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entropia_orme.db");
        std::fs::write(&path, b"this is not a sqlite database").unwrap();
        let before = std::fs::read(&path).unwrap();
        match Db::open_adopted(&path).await {
            Err(AdoptError::Quarantined { path: reported, .. }) => {
                assert_eq!(reported, path);
            }
            other => panic!("expected quarantine, got {other:?}"),
        }
        // The quarantine left the user's file byte-identical.
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[tokio::test]
    async fn is_below_baseline_distinguishes_the_pre_upgrade_race_from_a_real_fault() {
        let dir = tempfile::tempdir().unwrap();

        // A database the backend created but has not yet migrated up to the
        // baseline (version below it, no sqlx ledger): the first-launch-
        // after-upgrade race. open_adopted quarantines it, and
        // is_below_baseline() flags it as the retry-worthy case.
        let below = dir.path().join("below.db");
        {
            let db = Db::open(&below).await.unwrap();
            sqlx::query("UPDATE db_metadata SET value = '28' WHERE key = 'version'")
                .execute(db.write())
                .await
                .unwrap();
            sqlx::query("DROP TABLE _sqlx_migrations")
                .execute(db.write())
                .await
                .unwrap();
        }
        let err = Db::open_adopted(&below).await.unwrap_err();
        assert!(
            err.is_below_baseline(),
            "a pre-baseline database is the retry-worthy race, got {err:?}"
        );

        // A genuinely unadoptable file also quarantines, but is NOT the
        // race: retrying would never help, so it must not be flagged.
        let corrupt = dir.path().join("corrupt.db");
        std::fs::write(&corrupt, b"this is not a sqlite database").unwrap();
        let err = Db::open_adopted(&corrupt).await.unwrap_err();
        assert!(
            !err.is_below_baseline(),
            "a corrupt file is a permanent fault, not the race, got {err:?}"
        );

        // A fresh-path failure is likewise never the race.
        let boom = DbError::Driver(sqlx::Error::Protocol("boom".into()));
        assert!(!AdoptError::Fresh(boom).is_below_baseline());
    }

    #[test]
    fn adopt_error_display_carries_the_path_and_the_stand_down() {
        let quarantined = AdoptError::Quarantined {
            path: std::path::PathBuf::from("somewhere/entropia_orme.db"),
            source: DbError::Driver(sqlx::Error::Protocol("file is not a database".into())),
        };
        let rendered = quarantined.to_string();
        assert!(rendered.contains("somewhere"), "{rendered}");
        assert!(rendered.contains("cannot be adopted"), "{rendered}");
        assert!(rendered.contains("file is not a database"), "{rendered}");
        assert!(rendered.contains("left untouched"), "{rendered}");

        // `Fresh` renders its source plainly, adding nothing of its own.
        let inner = sqlx::Error::Protocol("boom".into());
        let expected = inner.to_string();
        assert_eq!(
            AdoptError::Fresh(DbError::Driver(inner)).to_string(),
            expected
        );

        // A contextualising variant carries its cause as a walkable source
        // chain, not a flattened string.
        let parse_err = serde_json::from_str::<Value>("not json").unwrap_err();
        let err = DbError::Decode {
            context: "equipment properties parse",
            source: parse_err,
        };
        assert!(err.to_string().starts_with("equipment properties parse: "));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[tokio::test]
    async fn open_adopted_reports_fresh_path_failures_plainly() {
        let dir = tempfile::tempdir().unwrap();
        // A directory in the file's place defeats creation without any
        // pre-existing database file at the path... but exists() is
        // true for directories, so use a missing parent instead.
        let path = dir.path().join("missing-parent").join("entropia_orme.db");
        match Db::open_adopted(&path).await {
            Err(AdoptError::Fresh(_)) => {}
            other => panic!("expected a fresh-path error, got {other:?}"),
        }
    }
}
