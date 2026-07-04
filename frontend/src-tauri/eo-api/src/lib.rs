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
use std::sync::{Arc, Mutex};

use eo_services::chatlog_watcher::ChatlogWatcher;
use eo_services::clock::Clock;
use eo_services::codex::CodexService;
use eo_services::config_service::ConfigService;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use eo_services::hotbar_listener::HotbarListener;
use eo_services::quests::QuestService;
use eo_services::skill_tracker::SkillTracker;
use eo_services::tracker::HuntTracker;

pub mod character;
pub mod codex;
pub mod equipment;
mod error;
pub mod manifest;
pub mod quests;
pub mod settings;

pub use error::ApiError;

/// The composed application facade the typed commands dispatch into.
pub struct Api {
    db: Db,
    game_data: Arc<GameDataStore>,
    /// The injectable wall clock: the calibration-staleness read compares
    /// the latest scan against it.
    clock: Arc<dyn Clock>,
    /// The resolved data directory: configuration read-through
    /// (`settings.json`) for the operations that consult it.
    data_dir: PathBuf,
    /// The sole settings writer: the settings-write operations lock and
    /// save through it, so there is no second writer to lose updates.
    config_service: Arc<Mutex<ConfigService>>,
    /// The live hunt tracker: a settings write re-signals it so an
    /// in-flight session re-reads its config, and a codex claim checks it
    /// before suppressing the claimed skill's next gain.
    tracker: Arc<HuntTracker>,
    /// The hotbar listener: a `hotbar_hooks_enabled` change flips its gate.
    hotbar: Arc<HotbarListener>,
    /// The chat-log watcher: a `chatlog_path` change restarts its tail.
    watcher: Arc<ChatlogWatcher>,
    /// The skill tracker: a codex claim suppresses the claimed skill's
    /// next gain on it while a session is live.
    skill_tracker: Arc<SkillTracker>,
    /// The codex service (species / ranks / recommendations / claims),
    /// built over the facade's shared db, catalogue, and clock.
    codex: CodexService,
    /// The quest service (quest + playlist CRUD, lifecycle, analytics),
    /// built over the facade's shared db and clock. This is the CRUD
    /// instance (unsubscribed); the bus-driven auto-start service is a
    /// separate producer concern.
    quests: QuestService,
}

impl Api {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        game_data: Arc<GameDataStore>,
        clock: Arc<dyn Clock>,
        data_dir: PathBuf,
        config_service: Arc<Mutex<ConfigService>>,
        tracker: Arc<HuntTracker>,
        hotbar: Arc<HotbarListener>,
        watcher: Arc<ChatlogWatcher>,
        skill_tracker: Arc<SkillTracker>,
    ) -> Self {
        let codex = codex::build_codex_service(db.clone(), game_data.clone(), clock.clone());
        let quests = quests::build_quests_service(db.clone(), clock.clone());
        Self {
            db,
            game_data,
            clock,
            data_dir,
            config_service,
            tracker,
            hotbar,
            watcher,
            skill_tracker,
            codex,
            quests,
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
