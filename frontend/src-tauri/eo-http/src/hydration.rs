//! The composed database handle the surviving native surface reaches
//! through.
//!
//! Every route family has migrated off the in-process HTTP layer onto typed
//! IPC commands (`eo_api`, ADR-0019), taking the response toolkit and the
//! read/snapshot computation with it (the developer-tools maintenance
//! actions moved to the facade with the dev family). What remains is the
//! thin [`HydrationState`] wrapper the shell installs so one native seam
//! still reaches the application database: the shutdown `PRAGMA optimize`.

use eo_services::db::Db;
use sqlx::SqlitePool;

/// The composed database handle the shutdown-optimise seam reads through.
pub struct HydrationState {
    db: Db,
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
