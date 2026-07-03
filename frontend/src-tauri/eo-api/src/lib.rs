//! The IPC facade: the application boundary the typed Tauri commands
//! call into.
//!
//! Each operation the frontend invokes is one async method on [`Api`],
//! taking and returning the DTO types defined beside it (plain `serde`
//! structs with JSON-Schema derives). The shell wraps every method in a
//! thin `#[tauri::command]`; the TypeScript bindings for the DTOs and
//! the command signatures are generated from this crate by `cargo xtask
//! gen-ts`, so the wire contract has a single Rust source.
//!
//! The facade is built whole from the composed services once the
//! database has opened (construct-then-share): every handle is present
//! by value, there is no half-initialised state to observe, and the
//! shell publishes the finished value to the IPC layer in one step.
//! Route families still served over the in-process HTTP router migrate
//! here family by family; this crate replaces that transport rather
//! than fronting it.

use std::path::PathBuf;
use std::sync::Arc;

use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

pub mod equipment;
mod error;
pub mod manifest;

pub use error::ApiError;

/// The composed application facade the typed commands dispatch into.
pub struct Api {
    db: Db,
    game_data: Arc<GameDataStore>,
    /// The resolved data directory: configuration read-through
    /// (`settings.json`) for the operations that consult it.
    data_dir: PathBuf,
}

impl Api {
    pub fn new(db: Db, game_data: Arc<GameDataStore>, data_dir: PathBuf) -> Self {
        Self {
            db,
            game_data,
            data_dir,
        }
    }

    /// The reader pool, for plain reads (dashboard reads run
    /// concurrently with combat writes).
    pub(crate) fn read(&self) -> &sqlx::SqlitePool {
        self.db.read()
    }

    /// The writer pool, for mutations.
    pub(crate) fn write(&self) -> &sqlx::SqlitePool {
        self.db.write()
    }
}
