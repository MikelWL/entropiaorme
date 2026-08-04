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
//! This crate owns the entire backend operation surface; it replaced
//! the in-process HTTP router the migration era ran on, since deleted
//! (ADR-0019).

// The tracking snapshot's `json!` literal expands one level per field
// and outgrew the default limit at 43; the literal stays declarative
// rather than assembling the map imperatively around a macro artefact.
#![recursion_limit = "256"]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eo_services::analytics::AnalyticsService;
use eo_services::chatlog_watcher::ChatlogWatcher;
use eo_services::clock::Clock;
use eo_services::codex::CodexService;
use eo_services::config_service::ConfigService;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use eo_services::hotbar_listener::HotbarListener;
use eo_services::market_service::MarketService;
use eo_services::quests::QuestService;
use eo_services::repair_ocr::RepairOcrService;
use eo_services::skill_scan_manual::SkillScanManual;
use eo_services::skill_tracker::SkillTracker;
use eo_services::spacebar_capture_listener::SpacebarCaptureListener;
use eo_services::tracker::HuntTracker;

pub mod activities;
pub mod analytics;
pub mod character;
pub mod codex;
pub mod demo;
pub mod dev;
pub mod equipment;
mod error;
pub mod manifest;
pub mod maps;
pub mod market;
mod nullable;
pub mod quests;
pub mod scan;
pub mod session_definitions;
pub mod settings;
pub mod tracking;

pub use error::ApiError;
pub use nullable::Nullable;

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
    /// The manual skill-scan state machine: the scan family's verbs drive
    /// it and read its status.
    skill_scan: Arc<SkillScanManual>,
    /// The hands-free spacebar-capture listener: the scan family's toggle
    /// flips its enabled gate.
    spacebar: Arc<SpacebarCaptureListener>,
    /// The one-shot repair-cost OCR: the tracking family's repair-scan leg
    /// drives it (gated on the `repair_ocr_enabled` config flag).
    repair_ocr: Arc<RepairOcrService>,
    /// The codex service (species / ranks / recommendations / claims),
    /// built over the facade's shared db, catalogue, and clock.
    codex: CodexService,
    /// The composed quest service (quest + playlist CRUD, lifecycle,
    /// analytics): the same instance whose owning task carries the
    /// bus-fed flows (session tracking, mission auto-start, reward
    /// suppression), so the command surface and the producer spine
    /// share one service.
    quests: Arc<QuestService>,
    /// The analytics service (Overview / Activity aggregates, ledger,
    /// presets, inventory), built over the facade's shared db and clock.
    analytics: AnalyticsService,
    /// The market service (markup-observation paste feed + reads), built
    /// over the facade's shared db and clock. An informational layer
    /// only: nothing here feeds the ledger or any realised P&L figure.
    market: MarketService,
    /// The session-definition service (definition + roster CRUD), built
    /// over the facade's shared db and clock; the tracking family's
    /// selection verb validates against it.
    session_definitions: Arc<eo_services::session_definitions::SessionDefinitionService>,
    /// The cartography-pin service (pin CRUD), built over the facade's
    /// shared db and clock; the facade adds the map-bounds gate on top.
    map_pins: eo_services::map_pins::MapPinsService,
    /// The pin-configuration service (the per-preset palette; pins are
    /// instances of a configuration), built over the same db and clock.
    pin_configs: eo_services::pin_configs::PinConfigsService,
    /// The bundled planet-map catalogue (a shipped resource), or `None`
    /// on a facade built without it (the maps family then serves an
    /// empty catalogue and the raster fetch reports unavailable).
    planet_maps: Option<Arc<eo_services::planet_maps::PlanetMapStore>>,
    /// The coordinate-capture service (the maps calibration flow and
    /// the one-shot coordinate scan), or `None` on a facade built
    /// without the native capture seams (those commands then report
    /// unavailable).
    coord_capture: Option<Arc<eo_services::coord_capture::CoordCaptureService>>,
    /// Persisted route navigation and radar guidance, composed only when
    /// the native capture and producer seams are available.
    navigation: Option<Arc<eo_services::navigation::NavigationService>>,
    /// The bundled guide-mode demo database path (a shipped resource), or
    /// `None` on a facade built without it (the demo commands then report the
    /// unavailable error). The demo services are a parallel database + tracker
    /// built lazily from it on first demo access.
    demo_db_path: Option<PathBuf>,
    /// The lazily-built demo services, stood up once on first demo access.
    /// The inner `None` records a build that could not be served, so a demo
    /// command degrades gracefully without retrying a hopeless build.
    demo: tokio::sync::OnceCell<Option<Arc<demo::DemoState>>>,
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
        skill_scan: Arc<SkillScanManual>,
        spacebar: Arc<SpacebarCaptureListener>,
        repair_ocr: Arc<RepairOcrService>,
        quests: Arc<QuestService>,
        demo_db_path: Option<PathBuf>,
        planet_maps: Option<Arc<eo_services::planet_maps::PlanetMapStore>>,
        coord_capture: Option<Arc<eo_services::coord_capture::CoordCaptureService>>,
        navigation: Option<Arc<eo_services::navigation::NavigationService>>,
    ) -> Self {
        let codex = codex::build_codex_service(db.clone(), game_data.clone(), clock.clone());
        let analytics = AnalyticsService::new(db.clone(), clock.clone());
        let market = MarketService::new(db.clone(), clock.clone());
        let session_definitions = eo_services::session_definitions::SessionDefinitionService::new(
            db.clone(),
            clock.clone(),
        );
        let map_pins = eo_services::map_pins::MapPinsService::new(db.clone(), clock.clone());
        let pin_configs =
            eo_services::pin_configs::PinConfigsService::new(db.clone(), clock.clone());
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
            skill_scan,
            spacebar,
            repair_ocr,
            codex,
            quests,
            analytics,
            market,
            session_definitions,
            map_pins,
            pin_configs,
            planet_maps,
            coord_capture,
            navigation,
            demo_db_path,
            demo: tokio::sync::OnceCell::new(),
        }
    }
}
