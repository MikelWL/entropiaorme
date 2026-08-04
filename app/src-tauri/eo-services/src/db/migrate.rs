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
    Migration {
        version: 7,
        description: "map pins",
        sql: include_str!("../../migrations/0007_map_pins.sql"),
    },
    Migration {
        version: 8,
        description: "map views",
        sql: include_str!("../../migrations/0008_map_views.sql"),
    },
    Migration {
        version: 9,
        description: "map navigation",
        sql: include_str!("../../migrations/0009_map_navigation.sql"),
    },
    Migration {
        version: 10,
        description: "navigation runtime fields",
        sql: include_str!("../../migrations/0010_navigation_runtime_fields.sql"),
    },
    Migration {
        version: 11,
        description: "pin configs",
        sql: include_str!("../../migrations/0011_pin_configs.sql"),
    },
    Migration {
        version: 12,
        description: "harvest stock removed",
        sql: include_str!("../../migrations/0012_harvest_stock_removed.sql"),
    },
    Migration {
        version: 13,
        description: "harvest yield tier",
        sql: include_str!("../../migrations/0013_harvest_yield_tier.sql"),
    },
    Migration {
        version: 14,
        description: "auction sales",
        sql: include_str!("../../migrations/0014_auction_sales.sql"),
    },
    Migration {
        version: 15,
        description: "stock movement tool",
        sql: include_str!("../../migrations/0015_stock_movement_tool.sql"),
    },
    Migration {
        version: 16,
        description: "stock opening balance",
        sql: include_str!("../../migrations/0016_stock_opening_balance.sql"),
    },
    Migration {
        version: 17,
        description: "undone entries",
        sql: include_str!("../../migrations/0017_undone_entries.sql"),
    },
    Migration {
        version: 18,
        description: "session facets",
        sql: include_str!("../../migrations/0018_session_facets.sql"),
    },
    Migration {
        version: 19,
        description: "session intervals",
        sql: include_str!("../../migrations/0019_session_intervals.sql"),
    },
    Migration {
        version: 20,
        description: "signal quests",
        sql: include_str!("../../migrations/0020_signal_quests.sql"),
    },
    Migration {
        version: 21,
        description: "quest families",
        sql: include_str!("../../migrations/0021_quest_families.sql"),
    },
    Migration {
        version: 22,
        description: "session definitions",
        sql: include_str!("../../migrations/0022_session_definitions.sql"),
    },
    Migration {
        version: 23,
        description: "default session definition",
        sql: include_str!("../../migrations/0023_default_session_definition.sql"),
    },
];

// Applied migrations are immutable. These hashes are a deliberate second
// ratchet beside the runtime ledger validation: changing an old SQL file now
// fails locally even when the test database is fresh and has no historic row
// capable of exposing the drift. Schema refinements always add a new version.
#[cfg(test)]
const FROZEN_CHECKSUMS: &[&str] = &[
    "92076B31C71D64008E027CB9016A4495BD477CB53BFAB3738926964A241CED0F8D8B2BB81BE70C673A3F86BFF2ED83CA",
    "4F167CE13DFEEB846931505C633FCF6F5D45E8FA760F3014DD6273796CA7E3DD81ED1A08527C7C7DAC85301B9B46FA10",
    "09ECF202A7D3CF35185195D0C623FB3BCC53CD598B915D8A62CB60EAFF698C266355EC7BB49EF4B2C14513681AD2D732",
    "4008598E5E86F950CA7758016F79D51A9C530D61388D0FBC2B6D080C46219371D1DB1DAA99F5BB3CDC1A7F0D9C9CDA60",
    "8B4DAB6032687FFAE181CEE89731D792A6AF12D122F471FC6AADC391A96C14251B16DCF0C1E3E5FA4701393C3BACB1B9",
    "84AD81FF155635AA349517B71A1264FF75D7D758AD6BD3FFE742DFBA4DFD246B1984613593346EDA6704C846044594DD",
    "E1E390E195B57A2AEE8B4F9D58FDC398FCA2B3200B9473D256D9295D15EE4608E3A9873F4EBC9DAA64BB692802C28B23",
    "667746910B06E0DBC590639E4159B3357316BAFFA22F4B10BEADB4FDF2E015D2E5978A1BDF78B91CEB136B9C489DDA5D",
    "0B3F86B763DB00318691AB7640AAB1901A5FE8A7A6FDE0B1D61B9C71048529DE6119D4E4CFDFD97711B15DFF2287307D",
    "14807806F2AEEA83A890C0114AABEAA2B1DAF3749D335CCFB0EADD3945CEB115C7788FA0FD1D340741FDC951986D677A",
    "D4CA3183B196882C7684EE6819E1761F35F33807EA01D8A63EB16E0E26FDFCEE3D74860C42CFF525D6F9E923B9AF0F0E",
    "E55EA0D16225309C60A08E5EC3B56D25AEA330B05A66C9D7EC97842F3985DCDB93AE74B66A27A5608F95EB891186FA56",
    "810DB5755C5307D254A7A39FEDE6629A930F6855593D9D081FDBCF346814E8AF0EDC020671F62D96B49BCB92F8E4BEED",
    "6F42FEFA98CF2C742D74B0A10F9C8C334C0FFCCCBAD422E13E3F25BEE5DEB88AA48D9767D16D170CCA5209E1DA731E0A",
    "EAC4ADF328EE46DB3F3A16D4DFB5629A004027A24C7F63AD86CE97D3F28535FAE8AFB2E45E166DC8B79708A118526796",
    "DF66ED36758ED3D19D74D142C3CA02712B3435685037C57D15B6790FC226FA71EEE2ACDADEF13C42F59977C14FF23C29",
    "6F4D4FCDB6F696BD1FBAEBFAEFF1A794D09D5FE26F0BB55BA51817B057B29DCAC8E83534881B42FE5FB6671288B1A4E9",
    "451A61961B15890F10954495E2E2E151BBC4AF47B36B498794B64FF65A07C89F420E205AAF7B8817295E74AE7F48C410",
    "147A07C1055FDD09810E13259D24FEBB4E078C2EA9D8C0FF6631943038D8900AC8031AFCD56F17A22C56F00FEEE46799",
    "321911B525CF5B0EFFB1F763563CC3F982F20930DA7B8B4754E5AD777394EDE6F15A7D1419E41BF8058CB70634BED329",
    "E1754050167F73A9B97809EC1399B88C016A5F71422B8684225FD7B834546545A2766FCF0C072BE7089B5DEEA855A41C",
    "EF1309FFA7D79EDEDD0AA4AB9106066C7E6B2CB9856DDE380DB348A5FE9D105C8ADA28D4405594EE3F2988B7E114D3A5",
    "708CB017C278DF637B27B2B8E787A6DC26722B951872932E99C4F50E7974995C14C3065403FD940BF31559D5ED21F3AA",
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

    /// Every migration checksum is frozen, not only checked against a newly
    /// created ledger. This catches an edit before a persistent development
    /// database has to quarantine itself to reveal the drift.
    #[test]
    fn migration_checksums_are_immutable() {
        assert_eq!(MIGRATIONS.len(), FROZEN_CHECKSUMS.len());
        for (migration, expected) in MIGRATIONS.iter().zip(FROZEN_CHECKSUMS) {
            let actual: String = migration
                .checksum()
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect();
            assert_eq!(
                &actual, expected,
                "migration {} is immutable; add a new migration instead",
                migration.version
            );
        }
    }

    #[test]
    fn harvest_yield_migration_backfills_direct_inferred_conflict_and_unknown_rows() {
        let mut connection = Connection::open_in_memory().expect("memory database");
        connection.execute_batch(LEDGER_DDL).expect("ledger");
        for migration in &MIGRATIONS[..12] {
            let tx = connection.transaction().expect("migration transaction");
            tx.execute_batch(migration.sql).expect("migration SQL");
            tx.execute(
                "INSERT INTO _sqlx_migrations \
                 (version, description, success, checksum, execution_time) \
                 VALUES (?1, ?2, TRUE, ?3, 0)",
                rusqlite::params![
                    migration.version,
                    migration.description,
                    migration.checksum()
                ],
            )
            .expect("ledger row");
            tx.commit().expect("migration commit");
        }
        connection
            .execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at) \
                 VALUES('session',0,300)",
                [],
            )
            .expect("session");
        connection
            .execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at) \
                 VALUES('other-session',0,300)",
                [],
            )
            .expect("other session");
        for (id, timestamp, tool) in [
            ("short", 0.0, Some("PH-3")),
            ("conflict", 10.0, Some("PH-3")),
            ("huge", 20.0, Some("PH-3")),
            ("long", 50.0, Some("PH-3")),
            ("after", 60.0, Some("PH-3")),
            ("other-tool", 55.0, Some("PH-4")),
            ("isolated", 200.0, Some("PH-3")),
        ] {
            connection
                .execute(
                    "INSERT INTO harvest_events \
                     (id,session_id,timestamp,success,tool_name,cost_ped,loot_total_ped) \
                     VALUES(?1,'session',?2,1,?3,0.1,0)",
                    rusqlite::params![id, timestamp, tool],
                )
                .expect("harvest");
        }
        connection
            .execute(
                "INSERT INTO harvest_events \
                 (id,session_id,timestamp,success,tool_name,cost_ped,loot_total_ped) \
                 VALUES('other-session-row','other-session',55,1,'PH-3',0.1,0)",
                [],
            )
            .expect("other-session harvest");
        for (harvest, item, deactivated_at) in [
            ("short", "Short Moonleaf Board", Some(1.0)),
            ("huge", "Long Moonleaf Board", None),
            ("long", "Moonleaf Board", None),
        ] {
            connection
                .execute(
                    "INSERT INTO harvest_loot_items \
                     (harvest_id,item_name,quantity,value_ped,deactivated_at) \
                     VALUES(?1,?2,1,0.1,?3)",
                    rusqlite::params![harvest, item, deactivated_at],
                )
                .expect("loot");
        }

        run(&mut connection).expect("yield migration");

        let mut stmt = connection
            .prepare(
                "SELECT id,yield_tier,yield_tier_source \
                 FROM harvest_events ORDER BY timestamp,id",
            )
            .expect("query");
        let rows: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("rows")
            .collect::<rusqlite::Result<_>>()
            .expect("collect");
        assert_eq!(
            rows,
            vec![
                ("short".into(), "short".into(), Some("board".into())),
                ("conflict".into(), "unknown".into(), None),
                ("huge".into(), "huge".into(), Some("board".into())),
                ("long".into(), "long".into(), Some("board".into())),
                ("other-session-row".into(), "unknown".into(), None),
                ("other-tool".into(), "unknown".into(), None),
                ("after".into(), "long".into(), Some("inferred".into())),
                ("isolated".into(), "unknown".into(), None),
            ]
        );
    }

    /// The board-to-tier rule exists twice: as Rust in `yield_tier_for_board`
    /// for live attribution, and as SQL inside migration 0013 for the
    /// historical backfill. The migration's bytes are frozen, so only the Rust
    /// side can drift, and a divergence would classify history differently
    /// from live tracking without failing anything.
    ///
    /// This applies the real migration rather than a transcription of its
    /// CASE, so the two implementations are compared as shipped. Each name
    /// gets its own session so no row can inherit a tier from a neighbour and
    /// mask a classification difference.
    #[test]
    fn the_backfill_sql_classifies_boards_exactly_as_the_rust_classifier_does() {
        use crate::harvest_yield::yield_tier_for_board;

        const NAMES: &[&str] = &[
            "Short Moonleaf Board",
            "Moonleaf Board",
            "Long Moonleaf Board",
            "Long Kaisenbrandt Board",
            // A species whose name merely begins with "Long": the space in the
            // "Long " prefix is the whole distinction.
            "Longleaf Board",
            " Board",
            // Non-board loot yields no tier evidence at all.
            "Wood Shavings",
            "Animal Muscle Oil",
        ];

        let mut connection = Connection::open_in_memory().expect("memory database");
        connection.execute_batch(LEDGER_DDL).expect("ledger");
        for migration in &MIGRATIONS[..12] {
            let tx = connection.transaction().expect("migration transaction");
            tx.execute_batch(migration.sql).expect("migration SQL");
            tx.execute(
                "INSERT INTO _sqlx_migrations \
                 (version, description, success, checksum, execution_time) \
                 VALUES (?1, ?2, TRUE, ?3, 0)",
                rusqlite::params![
                    migration.version,
                    migration.description,
                    migration.checksum()
                ],
            )
            .expect("ledger row");
            tx.commit().expect("migration commit");
        }

        for (index, name) in NAMES.iter().enumerate() {
            let session = format!("s{index}");
            let harvest = format!("h{index}");
            connection
                .execute(
                    "INSERT INTO tracking_sessions(id,started_at,ended_at) VALUES(?1,0,300)",
                    rusqlite::params![session],
                )
                .expect("session");
            connection
                .execute(
                    "INSERT INTO harvest_events \
                     (id,session_id,timestamp,success,tool_name,cost_ped,loot_total_ped) \
                     VALUES(?1,?2,10.0,1,'PH-3',0.1,0.1)",
                    rusqlite::params![harvest, session],
                )
                .expect("harvest");
            connection
                .execute(
                    "INSERT INTO harvest_loot_items \
                     (harvest_id,item_name,quantity,value_ped,deactivated_at) \
                     VALUES(?1,?2,1,0.1,NULL)",
                    rusqlite::params![harvest, name],
                )
                .expect("loot");
        }

        run(&mut connection).expect("yield migration");

        for (index, name) in NAMES.iter().enumerate() {
            let (tier, source): (String, Option<String>) = connection
                .query_row(
                    "SELECT yield_tier, yield_tier_source FROM harvest_events WHERE id = ?1",
                    rusqlite::params![format!("h{index}")],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("classified row");

            let rust = yield_tier_for_board(name);
            let expected_tier = rust.map_or("unknown", |t| t.as_str());
            let expected_source = rust.map(|_| "board".to_string());
            assert_eq!(
                (tier.as_str(), source.clone()),
                (expected_tier, expected_source),
                "{name:?} classifies differently in the migration SQL and in Rust"
            );
        }
    }

    /// A database that applied the original navigation migration upgrades
    /// forward without ledger surgery, preserving existing rows and filling
    /// the new route hotkey from its declared default.
    #[test]
    fn navigation_runtime_fields_upgrade_from_the_frozen_v9_schema() {
        let mut connection = Connection::open_in_memory().expect("memory database");
        connection.execute_batch(LEDGER_DDL).expect("ledger");
        for migration in &MIGRATIONS[..9] {
            let tx = connection.transaction().expect("migration transaction");
            tx.execute_batch(migration.sql).expect("migration SQL");
            tx.execute(
                "INSERT INTO _sqlx_migrations \
                 (version, description, success, checksum, execution_time) \
                 VALUES (?1, ?2, TRUE, ?3, 0)",
                rusqlite::params![
                    migration.version,
                    migration.description,
                    migration.checksum()
                ],
            )
            .expect("ledger row");
            tx.commit().expect("migration commit");
        }
        connection
            .execute(
                "INSERT INTO navigation_runs \
                 (planet, status, start_lon, start_lat, current_lon, current_lat, \
                  hop_count, created_at, updated_at) \
                 VALUES ('calypso', 'active', 1, 2, 1, 2, 3, 4, 4)",
                [],
            )
            .expect("v9 navigation row");

        run(&mut connection).expect("v10 upgrade");

        let upgraded: (Option<f64>, String) = connection
            .query_row(
                "SELECT last_position_at, hotkey FROM navigation_runs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("upgraded navigation row");
        assert_eq!(upgraded, (None, "f8".to_owned()));
        let ledger_tail: i64 = connection
            .query_row("SELECT MAX(version) FROM _sqlx_migrations", [], |row| {
                row.get(0)
            })
            .expect("ledger tail");
        assert_eq!(
            ledger_tail,
            MIGRATIONS.last().expect("chain is non-empty").version
        );
    }
}
