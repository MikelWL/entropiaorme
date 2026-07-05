//! The composed database handle the surviving native surface reaches
//! through.
//!
//! Every route family has migrated off the in-process HTTP layer onto typed
//! IPC commands (`eo_api`, ADR-0019), taking the response toolkit and the
//! read/snapshot computation with it. What remains is the thin
//! [`HydrationState`] wrapper the shell installs so two native seams still
//! reach the application database: the shutdown `PRAGMA optimize` and the
//! developer-mode maintenance routes (database compaction, projection
//! rebuild-and-verify).

use eo_services::db::Db;
use sqlx::SqlitePool;

/// The composed database handle the native maintenance seams read through.
pub struct HydrationState {
    pub(crate) db: Db,
}

impl HydrationState {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// The writer pool, for the shutdown `PRAGMA optimize`.
    pub(crate) fn write(&self) -> &SqlitePool {
        self.db.write()
    }
}
