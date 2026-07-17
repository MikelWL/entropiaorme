//! The embedded migration chain and its runner.
//!
//! The ledger table (`_sqlx_migrations`), its column shapes, and its
//! checksum discipline (SHA-384 over the raw migration file bytes) are
//! inherited verbatim from the previous runner, so every database in
//! the wild validates unchanged: the same versions, the same
//! descriptions, the same checksums, byte-for-byte. The runner
//! re-implements the same semantics: validate every applied row
//! against the embedded chain, refuse a checksum mismatch or a
//! previously-failed application, then apply the missing tail each in
//! its own transaction with its ledger row.
//!
//! The chain is embedded at compile time; a unit test asserts the
//! embedded set matches the `migrations/` directory on disk, so a new
//! migration file cannot land without joining the chain here.

use rusqlite::Connection;
use sha2::{Digest, Sha384};

use super::DbError;

/// One embedded migration: the version and description the ledger
/// records (derived from the `NNNN_description.sql` filename exactly as
/// the previous runner derived them) and the raw SQL.
pub(super) struct Migration {
    pub(super) version: i64,
    pub(super) description: &'static str,
    pub(super) sql: &'static str,
}

impl Migration {
    /// The ledger checksum: SHA-384 over the raw migration file bytes.
    pub(super) fn checksum(&self) -> Vec<u8> {
        Sha384::digest(self.sql.as_bytes()).to_vec()
    }
}

/// The migration chain, in application order. Filenames map to ledger
/// rows as `NNNN_snake_description.sql` -> (NNNN, "snake description").
pub(super) static MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "schema baseline",
        sql: include_str!("../../migrations/0001_schema_baseline.sql"),
    },
    Migration {
        version: 2,
        description: "analytical indexes",
        sql: include_str!("../../migrations/0002_analytical_indexes.sql"),
    },
    Migration {
        version: 3,
        description: "session summary read columns",
        sql: include_str!("../../migrations/0003_session_summary_read_columns.sql"),
    },
    Migration {
        version: 4,
        description: "daily rollups",
        sql: include_str!("../../migrations/0004_daily_rollups.sql"),
    },
    Migration {
        version: 5,
        description: "market observations",
        sql: include_str!("../../migrations/0005_market_observations.sql"),
    },
    Migration {
        version: 6,
        description: "harvest events",
        sql: include_str!("../../migrations/0006_harvest_events.sql"),
    },
];

/// The ledger table, exactly as the previous runner created it (and as
/// every database in the wild carries it).
const LEDGER_DDL: &str = "CREATE TABLE IF NOT EXISTS _sqlx_migrations (\
    version BIGINT PRIMARY KEY, description TEXT NOT NULL, \
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, \
    success BOOLEAN NOT NULL, checksum BLOB NOT NULL, \
    execution_time BIGINT NOT NULL)";

/// Validate the applied ledger against the embedded chain and apply the
/// missing tail. Call on the write connection, after adoption has
/// stamped or refused (see [`super::adopt_or_refuse`]).
pub(super) fn run(connection: &mut Connection) -> Result<(), DbError> {
    connection.execute_batch(LEDGER_DDL)?;

    // Validate the applied ledger as a contiguous prefix of the embedded
    // chain: same versions in the same positions (which refuses both
    // unknown versions and holes that would otherwise apply out of
    // order), descriptions and checksums identical, every application
    // recorded as successful.
    let applied: Vec<(i64, String, Vec<u8>, bool)> = connection
        .prepare(
            "SELECT version, description, checksum, success FROM _sqlx_migrations \
             ORDER BY version",
        )?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    for (index, (version, description, checksum, success)) in applied.iter().enumerate() {
        let Some(known) = MIGRATIONS.get(index).filter(|m| m.version == *version) else {
            return Err(DbError::MigrationValidation {
                version: *version,
                problem: "the applied ledger is not a contiguous prefix of the embedded chain",
            });
        };
        if !success {
            return Err(DbError::MigrationValidation {
                version: *version,
                problem: "a previous application of this migration failed",
            });
        }
        if known.description != description {
            return Err(DbError::MigrationValidation {
                version: *version,
                problem: "applied description does not match the embedded migration",
            });
        }
        if known.checksum() != *checksum {
            return Err(DbError::MigrationValidation {
                version: *version,
                problem: "applied checksum does not match the embedded migration",
            });
        }
    }

    // Apply the missing tail (everything past the validated prefix),
    // each migration and its ledger row in one transaction, in order.
    for migration in &MIGRATIONS[applied.len()..] {
        let started = std::time::Instant::now();
        let tx = connection.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) \
             VALUES (?1, ?2, TRUE, ?3, ?4)",
            rusqlite::params![
                migration.version,
                migration.description,
                migration.checksum(),
                started.elapsed().as_nanos() as i64,
            ],
        )?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded chain matches the migrations directory on disk:
    /// same files, same version/description derivation, same bytes. A
    /// new migration file fails here until it joins the chain.
    #[test]
    fn embedded_chain_matches_the_migrations_directory() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("migrations directory")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
            .collect();
        files.sort();
        assert_eq!(
            files.len(),
            MIGRATIONS.len(),
            "every migration file joins the embedded chain (and nothing extra)"
        );
        for (file, embedded) in files.iter().zip(MIGRATIONS) {
            let stem = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("utf-8 migration filename");
            let (version, description) = stem
                .split_once('_')
                .expect("NNNN_description migration filename");
            assert_eq!(
                version.parse::<i64>().expect("numeric version prefix"),
                embedded.version,
                "{stem}: version prefix matches the chain"
            );
            assert_eq!(
                description.replace('_', " "),
                embedded.description,
                "{stem}: description matches the chain"
            );
            let bytes = std::fs::read(file).expect("migration bytes");
            assert_eq!(
                bytes,
                embedded.sql.as_bytes(),
                "{stem}: embedded bytes match the file"
            );
        }
    }

    /// A drifted ledger fails validation loudly: a description edit, a
    /// hole in the applied sequence, and an unknown version each refuse
    /// before any migration applies.
    #[test]
    fn drifted_ledgers_are_refused() {
        let assert_refused = |mutation: &str, expected_problem: &str| {
            let mut connection = Connection::open_in_memory().expect("memory database");
            run(&mut connection).expect("fresh chain applies");
            connection.execute_batch(mutation).expect("ledger mutation");
            match run(&mut connection) {
                Err(DbError::MigrationValidation { problem, .. }) => {
                    assert_eq!(problem, expected_problem, "for mutation: {mutation}")
                }
                other => panic!("expected a validation refusal for {mutation}, got {other:?}"),
            }
        };
        assert_refused(
            "UPDATE _sqlx_migrations SET description = 'edited' WHERE version = 2",
            "applied description does not match the embedded migration",
        );
        assert_refused(
            "DELETE FROM _sqlx_migrations WHERE version = 2",
            "the applied ledger is not a contiguous prefix of the embedded chain",
        );
        assert_refused(
            "UPDATE _sqlx_migrations SET version = 99 WHERE version = 4",
            "the applied ledger is not a contiguous prefix of the embedded chain",
        );
        assert_refused(
            "UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 3",
            "applied checksum does not match the embedded migration",
        );
        assert_refused(
            "UPDATE _sqlx_migrations SET success = FALSE WHERE version = 1",
            "a previous application of this migration failed",
        );
    }

    /// The checksum discipline is SHA-384 over the raw file bytes: the
    /// exact accounting the previous runner wrote, pinned here against
    /// the baseline's known digest prefix so an algorithm drift cannot
    /// pass silently.
    #[test]
    fn checksums_reproduce_the_inherited_ledger_accounting() {
        let baseline = &MIGRATIONS[0];
        let hex: String = baseline
            .checksum()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect();
        assert!(
            hex.starts_with("92076B31C71D6400"),
            "the baseline checksum must reproduce the ledger rows already \
             in the wild; got {hex}"
        );
    }
}
