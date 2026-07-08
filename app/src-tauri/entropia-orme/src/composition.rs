//! The native-services composition root.
//!
//! Startup composition: resolve the data directory, open the
//! application database, load the game-data snapshot, and construct
//! the services over the real clock. The facade serves every typed
//! command through the state composed here; when any step declines, composition is terminal and the
//! backend does not come up for the session (there is no upstream to fall
//! back to).
//!
//! Composition assembles the full native surface: the hydration read
//! surface, the producer spine, and the OCR recogniser with its ONNX
//! Runtime obligations, all built here ahead of the operations that consume
//! them.
//!
//! ## The ONNX Runtime obligations
//!
//! The recogniser (`eo_services::ocr_engine`) binds the ONNX Runtime
//! dynamically (the `load-dynamic` feature), so the composition root
//! must discharge three runtime obligations before any session is built:
//!
//! 1. **Pin the dylib to an absolute path.** [`init_ort_runtime`] calls
//!    [`ort::init_from`] with the *absolute* path to the bundled
//!    `onnxruntime.dll` (the installed resource dir in a release build,
//!    the repo copy in dev), never a bare name. A bare name would let
//!    the OS loader resolve a stray `onnxruntime.dll` off `PATH`/CWD; the
//!    absolute path makes the shipped runtime authoritative. Its siblings
//!    `DirectML.dll` and `onnxruntime_providers_shared.dll` sit next to
//!    it in both layouts, where ONNX Runtime's own module-relative load
//!    finds them at session-creation time.
//! 2. **Init the global environment once.** `init_from` + `commit()` is
//!    process-global and once-only (a second commit is a silent no-op),
//!    so [`init_ort_runtime`] guards it with a [`std::sync::Once`]:
//!    re-entry (tests, a second compose) never tries to reconfigure a
//!    committed environment.
//! 3. **Select the execution provider, with a guaranteed CPU fallback.**
//!    The EP ladder lives per-session in
//!    [`eo_services::ocr_engine::OcrEngine::new_with_providers`]
//!    (the platform's GPU provider preferred, CPU fallback), not on the
//!    global env, so the engine owns its full session config. The env
//!    here carries no EPs.
//!
//! Two deliberate divergences from the original (`local_ocr.py`),
//! recorded so a later reviewer does not read them as oversights:
//!
//! * **Eager warm-up at composition, not lazy on first use.** The
//!   original warms the engine on the first `get_engine()`; we warm it at
//!   startup so the first real scan never eats the GPU provider's shader
//!   compilation. The warm-up runs a synchronous, potentially multi-
//!   second inference, so it is offloaded onto a blocking thread
//!   ([`tokio::task::spawn_blocking`]) rather than stalling the
//!   substrate's async runtime worker during startup.
//! * **No queried provider string.** The original records
//!   `session.get_providers()[0]`; this ort version has no per-session
//!   provider readout, so the engine derives the provider from its
//!   construction control flow (the GPU-then-CPU attempt) instead.
//!   The behaviour (GPU-preferred with CPU fallback) is faithful;
//!   only the readback mechanism differs.
//!
//! A failed ORT init or engine load never declines composition: OCR is
//! one optional faculty, so the read surface and producer spine compose
//! regardless and the engine sits `None` (exactly as
//! `local_ocr.get_engine()` returns `None`, with the consumer seams
//! defaulting to unavailable until a rebuilt engine flips them).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eo_services::bus_events::BusEvent;
use eo_services::chatlog_watcher::ChatlogWatcher;
use eo_services::clock::{Clock, RealClock};
use eo_services::config_service::{
    active_trifecta_preset, load_config_readonly, AppConfig, ConfigReader, ConfigService,
};
use eo_services::db::{AdoptError, Db};
use eo_services::equipment_pricing::{
    cost_per_shot_ped, heal_cost_from_props, hotbar_equipment_row_sync, weapon_cost_by_name,
};
use eo_services::eu_window;
use eo_services::event_bus::{EventBus, Topic};
use eo_services::game_data_store::GameDataStore;
use eo_services::hotbar_listener::{HotbarListener, HotbarResolver, HOTBAR_SLOT_KEYS};
use eo_services::keystroke_source::{HookKeystrokeSource, KeystrokeSource, SharedKeystrokeSource};
use eo_services::ocr_engine::load_bgr_png;
pub use eo_services::ocr_engine::OcrEngine;
use eo_services::paths::{resolve_data_dir, DB_FILE_NAME};
use eo_services::quests::QuestService;
use eo_services::repair_ocr::{RepairOcrService, RepairProviders};
use eo_services::scan_completion::{complete_skill_scan, hydrate_skill_scan_state};
use eo_services::scan_presets::ScanPresets;
use eo_services::screen_capture::{capture_region_bgr, capture_region_png};
use eo_services::skill_panel::{read_skill_panel, BgrImage};
pub use eo_services::skill_scan_manual::SkillScanManual;
use eo_services::skill_scan_manual::{ScanProviders, ScanRegion};
use eo_services::skill_tracker::SkillTracker;
pub use eo_services::spacebar_capture_listener::SpacebarCaptureListener;
use eo_services::time::naive_to_epoch;
use eo_services::tracker::{
    EquipmentLibrary, EquipmentProfile, HuntTracker, Providers, TrackingConfig,
};
use eo_services::trifecta_service::{describe_trifecta, TrifectaPreset};
use eo_wire::bus::DomainBus;
use eo_wire::domain_events::DomainEvent;
use serde_json::{Map, Value};

/// The repository root, compiled into dev builds (the manifest dir is
/// `app/src-tauri/entropia-orme`). Release builds never read it.
fn dev_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Where the user's data lives (see `eo_services::paths`). Release
/// builds resolve the installed app's per-user data directory; dev
/// builds honour `ENTROPIAORME_DATA_DIR` and the repo default.
pub(crate) fn data_dir() -> PathBuf {
    let override_value = std::env::var("ENTROPIAORME_DATA_DIR").ok();
    let frozen = !cfg!(debug_assertions);
    let appdata_root = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("USERPROFILE").map(PathBuf::from))
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    resolve_data_dir(
        override_value.as_deref(),
        &dev_project_root(),
        frozen,
        &appdata_root,
    )
}

/// The rolling-log directory: a `logs/` subdirectory of the resolved data
/// directory, so the structured logs sit beside the database under the same
/// OS app-data root the backend already owns. Resolved the same way as
/// [`data_dir`], so the shell can create it at startup before composition.
pub(crate) fn log_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Where the game-data snapshot lives: the bundled resource directory
/// in an installed build, the repository copy in dev.
fn snapshot_dir(resource_dir: Option<&PathBuf>) -> PathBuf {
    match resource_dir {
        Some(dir) if !cfg!(debug_assertions) => dir.join("snapshot"),
        _ => dev_project_root()
            .join("app")
            .join("src-tauri")
            .join("entropia-orme")
            .join("resources")
            .join("snapshot"),
    }
}

/// Where the bundled guide-mode demo database lives: the bundled resource
/// directory in an installed build (`<resource_dir>/demo/entropia_orme.db`),
/// the repository copy (`data/demo/entropia_orme.db`) in dev. The demo
/// services copy it to a writable per-process file before opening, so the
/// bundled file is never mutated.
pub(crate) fn demo_db_path(resource_dir: Option<&PathBuf>) -> PathBuf {
    match resource_dir {
        Some(dir) if !cfg!(debug_assertions) => dir.join("demo").join("entropia_orme.db"),
        _ => dev_project_root()
            .join("data")
            .join("demo")
            .join("entropia_orme.db"),
    }
}

/// The ABSOLUTE path to the platform's bundled ONNX Runtime dylib: the
/// installed resource dir in a release build, the committed repo copy
/// (`app/src-tauri/entropia-orme/resources/ort*/...`) in dev. On
/// Windows its siblings `DirectML.dll` and
/// `onnxruntime_providers_shared.dll` live in the same directory in
/// both layouts, where ONNX Runtime resolves them module-relative at
/// session creation; the Linux build is a single dylib with its GPU
/// provider (WebGPU) compiled in. Always absolute (the dev branch resolves through
/// the compiled-in [`dev_project_root`], the installed branch through
/// the OS-resolved `resource_dir`), so the runtime is never sought on
/// `PATH`/CWD.
fn ort_dylib_path(resource_dir: Option<&PathBuf>) -> PathBuf {
    // The runtime is platform-forked: the Windows onnxruntime-directml
    // build under `ort/`, the Linux WebGPU-enabled build under
    // `ort-linux/` (see each dir's PROVENANCE.txt). Both are bundled
    // beside the same `resources/` subtree; the release branch flattens
    // each to its own bundle target.
    #[cfg(target_os = "linux")]
    let (subdir, file) = ("ort-linux", "libonnxruntime.so");
    #[cfg(not(target_os = "linux"))]
    let (subdir, file) = ("ort", "onnxruntime.dll");
    match resource_dir {
        Some(dir) if !cfg!(debug_assertions) => dir.join(subdir).join(file),
        _ => dev_project_root()
            .join("app")
            .join("src-tauri")
            .join("entropia-orme")
            .join("resources")
            .join(subdir)
            .join(file),
    }
}

/// Where the recogniser's model + dict live: the bundled resource dir
/// (`<resource_dir>/models/`) in a release build, the repo copy
/// (`entropia-orme/resources/models/`) in dev. The asymmetry mirrors
/// `tauri.conf.json`'s bundle map (`resources/models/` -> `models/`):
/// the model is `svtrv2_rec.onnx`, the dict `ppocr_keys_v1.txt`.
fn models_dir(resource_dir: Option<&PathBuf>) -> PathBuf {
    match resource_dir {
        Some(dir) if !cfg!(debug_assertions) => dir.join("models"),
        _ => dev_project_root()
            .join("app")
            .join("src-tauri")
            .join("entropia-orme")
            .join("resources")
            .join("models"),
    }
}

/// Pin the ONNX Runtime dylib to its absolute path and commit the global
/// environment, exactly once for the process. `init_from` + `commit()`
/// is once-only (a second commit silently no-ops and a second load keeps
/// the first dylib), so the [`std::sync::Once`] guard keeps a second
/// `compose_native` (tests, re-entry) from re-attempting it. No execution
/// providers are set on the env: the EP ladder is a per-session concern
/// owned by [`OcrEngine::new_with_providers`]. A failed init is logged,
/// not fatal: a later `OcrEngine::new_with_providers` simply fails and
/// the engine sits `None`.
///
/// SECURITY (the load-bearing invariant): this MUST run before any other
/// ORT API call in the process. It pins the dylib two ways: it sets
/// `ORT_DYLIB_PATH` to the absolute bundled path AND calls `init_from`
/// with it. The env pin matters because if `init_from` ever fails (a
/// missing/corrupt/quarantined dylib), ort's lazy fallback loader would
/// otherwise resolve a BARE `onnxruntime.dll` off the OS search order
/// (exe dir / PATH / CWD): a DLL-planting vector. Pinning `ORT_DYLIB_PATH`
/// to the absolute path closes that fallback so the loader can only ever
/// resolve the trusted bundled library, never a planted one. Our value
/// overwrites any attacker-set one because it runs first.
/// Pin `ORT_DYLIB_PATH` to the absolute bundled dylib path and return it.
/// This is the fallback-closing half of [`init_ort_runtime`]: even if
/// `init_from` later fails, ort's lazy loader resolves THIS absolute path
/// rather than a bare `onnxruntime.dll` off the OS search order. Split out
/// so the pin is unit-testable without the process-global `Once`.
fn pin_ort_dylib_env(resource_dir: Option<&PathBuf>) -> PathBuf {
    let dylib = ort_dylib_path(resource_dir);
    std::env::set_var("ORT_DYLIB_PATH", &dylib);
    dylib
}

fn init_ort_runtime(resource_dir: Option<&PathBuf>) {
    // A compatible runtime ships for Windows (onnxruntime-directml) and
    // Linux (onnxruntime CPU); macOS and the rest have no bundled runtime,
    // so OCR stays offline there rather than pinning a foreign binary
    // (loading a wrong-platform library hangs or aborts the loader).
    if !cfg!(any(windows, target_os = "linux")) {
        return;
    }
    static ORT_INIT: std::sync::Once = std::sync::Once::new();
    ORT_INIT.call_once(|| {
        // Pin the fallback loader to the absolute path before any ORT use
        // (see the SECURITY note above): even if `init_from` fails, the
        // lazy `setup_api` path resolves THIS path, never a bare name.
        let dylib = pin_ort_dylib_env(resource_dir);
        match ort::init_from(&dylib) {
            Ok(builder) => {
                // `commit()` returns false if an env was already
                // committed; the Once guard makes that unreachable here,
                // but the result is discarded either way (the env is now
                // configured).
                let _ = builder.with_name("entropiaorme").commit();
            }
            Err(err) => tracing::warn!(
                target: "eo::composition",
                "ONNX Runtime at {} unavailable ({err}); OCR will be offline until restart",
                dylib.display()
            ),
        }
    });
}

/// The live producer spine: the in-process event bus, the chat-log
/// watcher (tailing in its own thread), and the trackers subscribed to
/// the bus, all sharing the substrate's single-owner database pool and
/// one injected clock. Kept separate from the typed-command facade so the
/// read surface stays a pure read surface and the producers are a
/// separate, stoppable concern.
///
/// The struct owns the producers for the substrate's lifetime; the
/// trackers and the quest service hold their own bus registrations and
/// stay alive only because this struct does. [`ProducerState::stop`]
/// (driven from the Tauri exit seam) stops the watcher's tail thread,
/// ends any open session, and drops the bus, so the OS-thread machinery
/// the watcher owns is torn down deterministically rather than left to
/// process-exit teardown.
pub struct ProducerState {
    // The chat-log watcher (tailing in its own thread). Held as an `Arc` so a
    // clone reaches the facade: the settings-write operation restarts it when
    // the watched `chatlog_path` changes. The exit seam still stops it via
    // [`ProducerState::stop`]; `restart`/`stop` take `&self`, so sharing is safe.
    watcher: Arc<ChatlogWatcher>,
    tracker: Arc<HuntTracker>,
    // The substrate runtime handle, kept for the synchronous exit seam's
    // one bridge onto the async tracker.
    runtime: tokio::runtime::Handle,
    // The typed domain-event channel. The bus bridge (subscribed below)
    // holds clones of this through its subscriber closures, and the shell's
    // Tauri-emit bridge subscribes over a clone handed off before this
    // state moves into the Tauri holder, so the publisher side and the
    // emit side share one channel.
    domain_bus: Arc<DomainBus>,
    // The settings writer. Held here so the producer spine and the
    // facade's write path share one service; a clone is handed to the
    // facade at composition. Mutex-guarded because `update`/`reset` take `&mut self`.
    config_service: Arc<Mutex<ConfigService>>,
    // The skill tracker. Held to keep its permanent bus subscription alive
    // for the substrate's lifetime, and exposed: the codex claim
    // operations call `suppress_next` on it.
    skill_tracker: Arc<SkillTracker>,
    // The in-process event bus the whole spine publishes on. Stored (rather
    // than left implicit on the subscriber handles) so the scan services
    // composed alongside the spine publish `scan.status.changed` on the SAME
    // bus the domain bridge subscribes, and so the shell's Tauri-emit
    // bridge carries scan-status envelopes exactly as session ones.
    bus: Arc<EventBus>,
    // The hotbar key listener. A producer (it publishes tool-change events on
    // the bus), gated on the hotbar-hooks toggle and an active session; held
    // here so the snapshot read can see whether it is running and so the
    // exit seam stops it. Shares the one keystroke source below.
    hotbar: Arc<HotbarListener>,
    // The one OS keyboard hook the input listeners share (the hotbar listener
    // here and the spacebar listener composed alongside the scan services).
    // Held so the spacebar listener can be built over the SAME source; the
    // hook is single-instance, so two independent sources would have one
    // stand inert.
    keystroke_source: Arc<dyn KeystrokeSource>,
    // The quest service: its owning task holds the permanent bus
    // subscriptions for the substrate's lifetime, and the typed-command
    // facade serves the quest families over this same instance.
    quests: Arc<QuestService>,
}

impl ProducerState {
    /// Stop the producer spine: end any open session (so its stop
    /// events publish cleanly while the bus is still live), stop the
    /// watcher's tail thread, then drop the bus. Idempotent enough for
    /// the exit path: a second stop is a no-op on an already-stopped
    /// watcher and an already-idle tracker.
    pub fn stop(&self) {
        // Stop the input listener first (it detaches the shared OS hook), then
        // end any open session while the bus is live, then the watcher. The
        // exit seam is the one synchronous caller of the async tracker, so it
        // bridges onto the substrate runtime here.
        self.hotbar.stop();
        if self.tracker.is_tracking() {
            let _ = block_on_pool(&self.runtime, self.tracker.stop_session());
        }
        self.watcher.stop();
    }

    /// The composed watcher, for tests driving a replay through it.
    #[cfg(test)]
    pub fn watcher(&self) -> &ChatlogWatcher {
        self.watcher.as_ref()
    }

    /// A handle to the composed chat-log watcher. The settings-write
    /// operation restarts it on a `chatlog_path` change; cloned into the
    /// facade at the composition handoff so the facade side and the
    /// producer-spine side share one watcher.
    pub fn watcher_handle(&self) -> Arc<ChatlogWatcher> {
        self.watcher.clone()
    }

    /// The composed tracker, for tests asserting its readout.
    #[cfg(test)]
    pub fn tracker(&self) -> &Arc<HuntTracker> {
        &self.tracker
    }

    /// A handle to the composed tracker. The tracking operations serve over
    /// this same `Arc<HuntTracker>`: the composition handoff clones it into
    /// the facade before this `ProducerState` moves into the
    /// Tauri-managed producer holder, so the facade and the exit-seam
    /// teardown share one tracker.
    pub fn tracker_handle(&self) -> Arc<HuntTracker> {
        self.tracker.clone()
    }

    /// A handle to the composed domain-event channel. The shell's
    /// Tauri-emit bridge subscribes over this same `Arc<DomainBus>`: the
    /// composition handoff clones it before this `ProducerState` moves into
    /// the Tauri-managed producer holder, so the emit side and the
    /// producer-bus bridge share one channel.
    pub fn domain_bus_handle(&self) -> Arc<DomainBus> {
        self.domain_bus.clone()
    }

    /// A handle to the composed settings writer. The settings-write
    /// operations serve over this same `Arc<Mutex<ConfigService>>`: the
    /// composition handoff clones it into the facade before this
    /// `ProducerState`
    /// moves into the Tauri-managed holder, so the write path and the spine
    /// share one service (reads elsewhere stay file-based, coherent because
    /// every save reads-merges-before-write).
    pub fn config_service_handle(&self) -> Arc<Mutex<ConfigService>> {
        self.config_service.clone()
    }

    /// A handle to the composed skill tracker. The codex claim operations
    /// call `suppress_next` on this same `Arc<SkillTracker>`: cloned into
    /// the facade at the handoff, so the facade side and the producer-bus
    /// subscription side share one tracker.
    pub fn skill_tracker_handle(&self) -> Arc<SkillTracker> {
        self.skill_tracker.clone()
    }

    /// A handle to the composed quest service. The typed-command facade
    /// serves the quest and playlist families over this same
    /// `Arc<QuestService>`: cloned into the facade at the composition
    /// handoff, so the bus-fed flows (session tracking, mission
    /// auto-start, reward suppression) and the command surface share one
    /// instance.
    pub fn quests_handle(&self) -> Arc<QuestService> {
        self.quests.clone()
    }

    /// A handle to the spine's event bus. The scan services compose on this
    /// same `Arc<EventBus>`, so their `scan.status.changed` envelopes reach
    /// the domain bridge (subscribed in [`compose_producers`]) and the
    /// shell's Tauri-emit bridge, exactly as the tracker's session events do.
    pub fn bus_handle(&self) -> Arc<EventBus> {
        self.bus.clone()
    }

    /// A handle to the composed hotbar listener. The tracking snapshot reads
    /// its `is_running` flag; cloned into the facade at the handoff so the
    /// facade side and the producer-spine side share one listener.
    pub fn hotbar_handle(&self) -> Arc<HotbarListener> {
        self.hotbar.clone()
    }

    /// A handle to the shared OS keyboard hook. The spacebar-capture listener
    /// composes over this SAME source, so both input listeners ride one hook
    /// (it is single-instance) while each gates independently.
    pub fn keystroke_source_handle(&self) -> Arc<dyn KeystrokeSource> {
        self.keystroke_source.clone()
    }
}

/// What a successful composition yields: the read surface, the producer
/// spine (sharing one pool and one clock), and the warmed OCR engine
/// when the runtime loaded.
///
/// `ocr_engine` is a sibling of `producers`, not a member of it: the
/// producer spine is the bus-subscribed, stoppable concern (watcher tail
/// thread, trackers, an exit-seam `stop()`), whereas the engine owns no
/// thread, no subscription, and no teardown obligation (its ONNX session
/// drops with the handle and the ORT env self-releases at process exit).
/// It is `Option` because OCR is an optional faculty: a failed runtime
/// load leaves it `None` while the rest of composition still succeeds.
/// `Arc` because the scan consumer seams each hold their own clone,
/// captured at compose time.
pub struct Composed {
    /// The composed database handle, held so the exit seam can run the
    /// once-per-lifecycle `PRAGMA optimize` at shutdown (the last live user
    /// of the composed `Db` outside the typed-command facade).
    pub db: Db,
    /// The typed-command facade (the application boundary the typed
    /// Tauri commands dispatch into), sharing the database and catalogue
    /// handles.
    pub api: Arc<eo_api::Api>,
    pub producers: ProducerState,
    /// The warmed OCR engine when the runtime loaded. Production
    /// consumers (the scan services) hold their own clones captured at
    /// compose time, so nothing re-reads this copy outside the
    /// composition tests, which drive the warmed engine directly.
    #[cfg_attr(not(test), allow(dead_code))]
    pub ocr_engine: Option<Arc<OcrEngine>>,
    /// The manual skill-scan state machine, composed on the spine bus (its
    /// `scan.status.changed` envelopes reach the shell's event bridge) over
    /// the OCR extraction providers. Always constructed so the scan
    /// operations serve;
    /// its capture and extraction seams stand down to "engine unavailable"
    /// when the OCR runtime is absent (a golden-pinned reply).
    pub skill_scan: Arc<SkillScanManual>,
    /// The spacebar-capture listener, composed over the scan and the shared
    /// OS hook. Held for the spacebar-capture toggle and the exit
    /// seam (its teardown).
    pub spacebar_listener: Arc<SpacebarCaptureListener>,
}

/// The outcome of a composition attempt at the substrate's startup: the
/// native services are built and ready to install, or composition declined
/// with a logged reason. With the Python sidecar decommissioned (the
/// backend now collapsed into the shell process) there is no proxy fallback
/// and nothing to migrate an
/// existing database forward, so a decline is terminal for the session (the
/// backend cannot serve); there is no longer an interim "awaiting migration"
/// state to retry.
pub enum Composition {
    /// The native services are built and ready to install.
    Ready(Composed),
    /// A terminal decline (a missing/empty snapshot, a producer fault, or a
    /// database that cannot be opened or adopted, including one whose schema
    /// predates the supported baseline, which the retired sidecar used to
    /// migrate forward). Logged loudly; the backend does not come up.
    Declined,
}

/// Compose the native services, or decline with a logged reason. The ONNX
/// Runtime dylib is pinned (once) before any composition step, so the engine
/// constructed inside `compose_with` binds the bundled runtime, not a stray
/// one off `PATH`.
pub async fn compose_native(resource_dir: Option<PathBuf>) -> Composition {
    init_ort_runtime(resource_dir.as_ref());
    // The single shared OS keyboard hook, built at this production site and
    // injected down the compose chain. The allowlist filters at the hook
    // boundary to the hotbar digit keys and the space key, so out-of-scope
    // keystrokes never enter the event stream. Tests inject a hook-free
    // `MockKeystrokeSource` through the same parameter instead, so a generic
    // test run never installs the OS hook (whose attach/detach lifecycle can
    // intermittently wedge a headless run).
    let allowlist: std::collections::BTreeSet<String> = HOTBAR_SLOT_KEYS
        .iter()
        .map(|key| key.to_string())
        .chain(std::iter::once("space".to_string()))
        .collect();
    let keystroke_source: Arc<dyn KeystrokeSource> =
        Arc::new(HookKeystrokeSource::new(Some(allowlist)));
    compose_with(
        data_dir(),
        snapshot_dir(resource_dir.as_ref()),
        models_dir(resource_dir.as_ref()),
        Some(demo_db_path(resource_dir.as_ref())),
        keystroke_source,
    )
    .await
}

/// Composition over already-resolved locations (separated from the
/// environment-reading resolution so the decline paths are testable).
/// `models` is the recogniser's model+dict directory; the engine is
/// constructed and warmed from it, but a failed engine load never
/// declines composition (OCR is optional).
async fn compose_with(
    data_dir: PathBuf,
    snapshot: PathBuf,
    models: PathBuf,
    demo_db_path: Option<PathBuf>,
    keystroke_source: Arc<dyn KeystrokeSource>,
) -> Composition {
    if let Err(err) = std::fs::create_dir_all(&data_dir) {
        tracing::error!(
            target: "eo::composition",
            "data dir {} not creatable ({err}); native services stand down",
            data_dir.display()
        );
        return Composition::Declined;
    }
    let db_path = data_dir.join(DB_FILE_NAME);
    let db = match Db::open_adopted(&db_path).await {
        Ok(db) => db,
        Err(err) if err.is_below_baseline() => {
            // The existing database is below the supported baseline AND below
            // the single rung the native first-launch upgrade bridges
            // (v32 -> v33, the version every in-the-wild v0.1.0-lineage
            // database sits at). A v32 database is upgraded in place and
            // adopted; only older schemas reach here, and none exist in the
            // wild, so this is a deliberate terminal decline, not a missing
            // capability.
            tracing::error!(
                target: "eo::composition",
                "{err}; the backend cannot serve until the database is at the supported baseline"
            );
            return Composition::Declined;
        }
        Err(err @ AdoptError::Quarantined { .. }) => {
            // An existing database we cannot adopt (for any other reason) is
            // surfaced loudly and left untouched for diagnosis.
            tracing::error!(target: "eo::composition", "{err}");
            return Composition::Declined;
        }
        Err(err) => {
            tracing::error!(
                target: "eo::composition",
                "database open failed ({err}); native services stand down"
            );
            return Composition::Declined;
        }
    };
    // A bounded corruption probe: PRAGMA quick_check on a reader connection,
    // time-boxed and run off the launch path so it never stalls startup. A
    // problem, or a budget overrun on a very large database, is logged and
    // the app keeps serving (degrade, never crash).
    {
        let probe_db = db.clone();
        tokio::spawn(async move {
            use eo_services::db::QuickCheckOutcome;
            match probe_db
                .quick_check_budgeted(eo_services::db::STARTUP_QUICK_CHECK_BUDGET)
                .await
            {
                QuickCheckOutcome::Ok => {}
                QuickCheckOutcome::Corrupt(report) => tracing::error!(
                    target: "eo::composition",
                    "startup quick_check reported database problems: {report}"
                ),
                QuickCheckOutcome::OverBudget => tracing::warn!(
                    target: "eo::composition",
                    "startup quick_check exceeded its budget and was skipped; \
                     the database is left unprobed this launch"
                ),
                QuickCheckOutcome::Error(err) => tracing::warn!(
                    target: "eo::composition",
                    "startup quick_check could not run: {err}"
                ),
            }
        });
    }

    let game_data = match GameDataStore::new(&snapshot) {
        Ok(store) => Arc::new(store),
        Err(err) => {
            tracing::error!(
                target: "eo::composition",
                "game-data snapshot at {} unreadable ({err}); native services stand down",
                snapshot.display()
            );
            return Composition::Declined;
        }
    };
    // The store tolerates a missing directory (warn-and-continue), but an
    // empty store here means the bundled resources are absent or
    // broken: serving game-data-derived responses from it would
    // silently diverge from the reference's embedded copy. Stand down
    // (a terminal decline) rather than serve divergent data.
    if game_data.total_entities() == 0 {
        tracing::error!(
            target: "eo::composition",
            "game-data snapshot at {} is empty; native services stand down",
            snapshot.display()
        );
        return Composition::Declined;
    }
    let clock: Arc<dyn Clock> = Arc::new(RealClock::new());

    // The producer spine shares the substrate's single-owner pool with
    // the read surface: one connection, one owner, serialised access
    // (WAL + busy_timeout, max_connections(1)), so producer writes and
    // facade reads queue through the single connection without deadlock.
    // A second handle over the SAME pool: `Db` is a thin clonable handle
    // around the connection pool, so the producers and the read surface
    // share one connection (one owner, serialised access) rather than
    // opening a second.
    let producer_db = db.clone();
    let producers = match compose_producers(
        producer_db,
        clock.clone(),
        &data_dir,
        None,
        keystroke_source,
    ) {
        Ok(producers) => producers,
        Err(err) => {
            tracing::error!(
                target: "eo::composition",
                "producer spine failed ({err}); native services stand down"
            );
            return Composition::Declined;
        }
    };

    // Construct the recogniser off the runtime worker (awaited; the engine
    // must exist for the handoff), with warm-up detached so the slow first
    // inference does not gate compose -> serve (see `build_ocr_engine`). A
    // failed load is logged and leaves the engine `None`; OCR is optional,
    // so composition still succeeds. The GPU-then-CPU ladder and the
    // recorded provider live in `OcrEngine::new_with_providers`.
    let ocr_engine = build_ocr_engine(models).await;

    // The scan services compose on the spine bus (so their status frames
    // reach the shell's event bridge) over the OCR extraction providers
    // and the
    // shared single-owner pool, before the read surface takes ownership of
    // `db`/`game_data`/`clock`/`data_dir`. The calibration artefact sits
    // beside the snapshot dir in both the dev and installed layouts, so it
    // resolves as the snapshot's sibling.
    let geometry_path = snapshot
        .parent()
        .map(|parent| parent.join("panel_geometry.json"))
        .unwrap_or_else(|| snapshot.join("panel_geometry.json"));
    let (skill_scan, repair_ocr, spacebar_listener) = compose_scan_services(
        producers.bus_handle(),
        ocr_engine.clone(),
        game_data.clone(),
        db.clone(),
        clock.clone(),
        geometry_path,
        producers.keystroke_source_handle(),
    )
    .await;

    // The typed-command facade shares the read surface's handles plus the
    // producers the write families signal (the config writer, the hunt
    // tracker, the hotbar gate, the chat-log watcher, the skill tracker
    // a codex claim suppresses on, and the manual-scan state machine +
    // spacebar listener the scan family drives), so the facade and the
    // producer spine serve over the same live instances.
    let api = Arc::new(eo_api::Api::new(
        db.clone(),
        game_data.clone(),
        clock.clone(),
        data_dir.clone(),
        producers.config_service_handle(),
        producers.tracker_handle(),
        producers.hotbar_handle(),
        producers.watcher_handle(),
        producers.skill_tracker_handle(),
        skill_scan.clone(),
        spacebar_listener.clone(),
        repair_ocr.clone(),
        producers.quests_handle(),
        demo_db_path,
    ));
    Composition::Ready(Composed {
        db,
        api,
        producers,
        ocr_engine,
        skill_scan,
        spacebar_listener,
    })
}

/// Construct the OCR engine off the async runtime worker, then warm it up
/// DETACHED. Construction (a session commit) is awaited because the engine
/// must exist before the handoff to managed state, but it is quick. Warm-up
/// (a real inference; the GPU providers compile shaders on first run, seconds) is
/// NOT awaited: stalling it ahead of `serve()` would make every request the
/// webview fires during startup hang, so it runs on a detached blocking
/// thread, concurrent with the server coming up. The reference warms lazily
/// on first scan, so deferring the cost off the startup path is, if
/// anything, more faithful. Returns `None` (logged) on any load failure:
/// OCR is an optional faculty and never declines composition.
async fn build_ocr_engine(models: PathBuf) -> Option<Arc<OcrEngine>> {
    // OCR ships only where a compatible ONNX Runtime is bundled: Windows
    // (the onnxruntime-directml libraries) and Linux (the WebGPU-enabled
    // build). Elsewhere the engine stays absent rather than attempting to
    // load a foreign-platform runtime (which hangs the loader), exactly as
    // a failed load would leave it. Keep this gate aligned with
    // `init_ort_runtime`'s platform check.
    if !cfg!(any(windows, target_os = "linux")) {
        return None;
    }
    let model_path = models.join("svtrv2_rec.onnx");
    let dict_path = models.join("ppocr_keys_v1.txt");
    let constructed =
        tokio::task::spawn_blocking(move || OcrEngine::new_with_providers(&model_path, &dict_path))
            .await;
    let engine = match constructed {
        Ok(Ok(engine)) => Arc::new(engine),
        Ok(Err(err)) => {
            tracing::warn!(
                target: "eo::composition",
                "OCR engine unavailable ({err}); scan features offline until restart"
            );
            return None;
        }
        Err(err) => {
            tracing::warn!(
                target: "eo::composition",
                "OCR engine construction task failed ({err}); scan features offline"
            );
            return None;
        }
    };
    tracing::info!(
        target: "eo::composition",
        "OCR engine ready (provider={})",
        engine.provider()
    );
    // Warm up detached so the multi-second first inference never gates the
    // compose -> serve handoff. Best-effort: the result is discarded and a
    // failure cannot affect the already-constructed engine.
    let warming = engine.clone();
    tokio::task::spawn_blocking(move || warming.warm_up());
    Some(engine)
}

/// Compose the manual skill scan and the repair-cost OCR over the OCR
/// engine, the live game-window region lookups, and the on-demand screen
/// capturer. The scan publishes `scan.status.changed` on the spine `bus`
/// (so its frames reach the shell's event bridge), persists accepted
/// calibrations
/// through the shared `pool`, and hydrates its resting status from the
/// same. When the OCR runtime is absent the providers stand down to
/// "engine unavailable" rather than declining composition, so the scan
/// operations still serve (the scan reports offline, a golden-pinned
/// reply). Both services are always constructed.
async fn compose_scan_services(
    bus: Arc<EventBus>,
    ocr_engine: Option<Arc<OcrEngine>>,
    game_data: Arc<GameDataStore>,
    db: Db,
    clock: Arc<dyn Clock>,
    geometry_path: PathBuf,
    keystroke_source: Arc<dyn KeystrokeSource>,
) -> (
    Arc<SkillScanManual>,
    Arc<RepairOcrService>,
    Arc<SpacebarCaptureListener>,
) {
    let runtime = tokio::runtime::Handle::current();

    // The calibrated panel grid and the canonical skill vocabulary the
    // panel reader resolves names against; both read existing snapshot
    // assets (the geometry artefact beside the snapshot dir, the skill
    // names from the bundled `skills` endpoint), so this is port scope, not
    // new surface.
    let presets = Arc::new(ScanPresets::new(&geometry_path));
    let skill_geom = presets.skill.to_geom_value();
    let vocab: Vec<String> = game_data
        .get_entities("skills")
        .iter()
        .filter_map(|entity| {
            entity
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();

    // The skill-scan provider seams: engine availability is fixed at the
    // load attempt (the engine never reloads at runtime); the region lookup
    // reads the live game window through the calibrated anchor; the capturer
    // grabs the PNG on demand; the extractor decodes, slices, OCRs, and
    // filters to resolved (name, level) rows.
    let has_engine = ocr_engine.is_some();
    let region_presets = presets.clone();
    let skill_region: Arc<dyn Fn() -> Option<ScanRegion> + Send + Sync> =
        Arc::new(move || eu_window::skill_region(&region_presets));
    let capture_region: Arc<dyn Fn(ScanRegion) -> Option<Vec<u8>> + Send + Sync> =
        Arc::new(|(tl, br): ScanRegion| {
            capture_region_png(tl[0], tl[1], br[0] - tl[0], br[1] - tl[1])
        });
    let extract_engine = ocr_engine.clone();
    let extract_geom = skill_geom;
    let extract_vocab = vocab;
    let extract_page_levels: eo_services::skill_scan_manual::PageExtractor =
        Arc::new(move |png: &[u8]| {
            read_skill_page_levels(&extract_engine, &extract_geom, &extract_vocab, png)
        });

    let scan_providers = ScanProviders {
        engine_available: Arc::new(move || has_engine),
        skill_region,
        capture_region,
        extract_page_levels,
    };

    // Hydrate the resting status (last scan time, skills count) from the
    // persisted calibration history.
    let (initial_scan_time, initial_skills_count) =
        hydrate_skill_scan_state(&db).await.unwrap_or((None, 0));

    let skill_scan = SkillScanManual::new(
        scan_providers,
        clock.clone(),
        Some(bus),
        initial_scan_time,
        initial_skills_count,
    );

    // The completion callback persists accepted calibrations through the
    // shared database handle, bridging onto the runtime from the scan's
    // worker thread the same dual way the tracker's providers do; a
    // persist error surfaces on the scan status rather than panicking.
    let completion_db = db.clone();
    let completion_clock = clock.clone();
    let completion_runtime = runtime;
    skill_scan.set_completion_callback(Arc::new(move |levels: &[(String, f64)]| {
        let scan_time = naive_to_epoch(completion_clock.now());
        let levels = levels.to_vec();
        let db = completion_db.clone();
        block_on_pool(&completion_runtime, async move {
            complete_skill_scan(&db, &levels, scan_time).await
        })
        .map(|_drift| ())
        .map_err(Into::into)
    }));

    // The repair-OCR provider seams: the same calibrated region lookup and
    // capturer (BGR pixels here), recognised by the shared engine.
    let repair_presets = presets;
    let repair_engine = ocr_engine;
    let repair_ocr = Arc::new(RepairOcrService::new(RepairProviders {
        repair_region: Arc::new(move || eu_window::repair_region(&repair_presets)),
        capture_region: Arc::new(capture_region_bgr),
        read_text: Arc::new(move |frame: &BgrImage| {
            repair_engine
                .as_ref()?
                .recognize_bgr(&frame.data, frame.h, frame.w)
                .ok()
        }),
    }));

    // The spacebar-capture listener fires a manual-scan capture on a space
    // press while the scan is capturing, over the SAME OS hook the hotbar
    // listener rides (the hook is single-instance). Off until the overlay
    // toggle enables it through the spacebar-capture command.
    let spacebar = SpacebarCaptureListener::new(skill_scan.clone(), Some(keystroke_source));

    (skill_scan, repair_ocr, spacebar)
}

/// Extract `{canonical_name: level}` rows from one captured skill-panel
/// PNG: decode to BGR, slice the calibrated grid, OCR each name/level cell,
/// estimate each bar fill, and keep only the rows that both resolved a name
/// and parsed a level. Returns no rows when the engine is absent, the PNG
/// is unreadable, or the grid is uncalibrated (the reader would otherwise
/// have no rows to slice and would panic), so an installed build missing
/// the calibration artefact degrades to an empty scan rather than crashing.
fn read_skill_page_levels(
    engine: &Option<Arc<OcrEngine>>,
    skill_geom: &Value,
    vocab: &[String],
    png: &[u8],
) -> Vec<(String, f64)> {
    let Some(engine) = engine.as_ref() else {
        return Vec::new();
    };
    if skill_geom.get("n_rows").and_then(Value::as_i64).is_none() {
        return Vec::new();
    }
    let Ok((data, h, w)) = load_bgr_png(png) else {
        return Vec::new();
    };
    let panel = BgrImage { data, h, w };
    let read = |crop: &BgrImage| -> (String, f64) {
        engine
            .recognize_bgr(&crop.data, crop.h, crop.w)
            .unwrap_or_default()
    };
    read_skill_panel(&read, &panel, skill_geom, vocab)
        .into_iter()
        .filter_map(|row| match (row.name, row.level) {
            (Some(name), Some(level)) => Some((name, level)),
            _ => None,
        })
        .collect()
}

/// Subscriber backlog bound for the typed domain channel (the shell's
/// emit bridge is the one live subscriber; a receiver that falls behind
/// observes a lag error and skips ahead rather than stalling publishers).
const DOMAIN_BUS_CAPACITY: usize = 256;

/// Why the producer spine declined to compose: a database fault, or a
/// settings store that refused to load (a corrupt or wrong-shape
/// `settings.json` fails composition loudly rather than silently
/// resetting user settings). The caller logs it and stands the native
/// services down.
#[derive(Debug, thiserror::Error)]
enum ComposeError {
    #[error(transparent)]
    Db(#[from] eo_services::db::DbError),
    #[error("settings store: {0}")]
    Config(#[from] std::io::Error),
}

/// Build and start the producer spine over the shared pool and clock.
/// Every lookup the tracker consults reads through the same database or
/// the same config read-through the `ConfigService` writes.
fn compose_producers(
    db: Db,
    clock: Arc<dyn Clock>,
    data_dir: &std::path::Path,
    chatlog_override: Option<PathBuf>,
    keystroke_source: Arc<dyn KeystrokeSource>,
) -> Result<ProducerState, ComposeError> {
    // The producers run on the substrate's tokio runtime; the trackers
    // bridge their database work onto this handle from their own
    // (non-runtime) producer threads.
    let runtime = tokio::runtime::Handle::current();
    let bus = Arc::new(EventBus::new());

    // The domain bridge: forward the frontend-facing domain topics off the
    // in-process bus onto the typed broadcast channel the shell's
    // Tauri-emit bridge consumes. Subscribed here, before the watcher's
    // tail thread starts, so an event published the instant a producer
    // ticks is never raced away from a connected subscriber. The channel is
    // held in `ProducerState` (and through these subscriber closures), and
    // a clone is handed to the shell at composition.
    let domain_bus = Arc::new(DomainBus::new(DOMAIN_BUS_CAPACITY));
    subscribe_domain_bridge(&bus, &domain_bus);

    // The config read-through: producers read the live config the same
    // way the read surface does (`settings.json`, now written solely by
    // the native `ConfigService`), so a wrong default never silently
    // corrupts tracker DB state. A read failure falls back to the typed
    // defaults.
    let config = load_config_readonly(data_dir).unwrap_or_default();

    // The settings writer the write operations serve over. A corrupt or
    // wrong-shape settings file fails composition loudly (a terminal
    // decline) rather than silently resetting user settings.
    let config_service = Arc::new(Mutex::new(ConfigService::new(data_dir)?));

    // The quest service is bus-subscribed (session tracking + mission
    // auto-start) and supplies the watcher's quest-reward filter, so a
    // mission completion can suppress its reward echo. Its owning task
    // serialises those flows;
    // the filter closure is a rendezvous into it.
    let quests = QuestService::start(&bus, db.clone(), clock.clone(), runtime.clone());

    let watched_chatlog = chatlog_override.unwrap_or_else(|| PathBuf::from(&config.chatlog_path));
    let watcher = Arc::new(ChatlogWatcher::new(
        bus.clone(),
        watched_chatlog,
        Some(quests.watcher_filter()),
    ));

    let skill_tracker = SkillTracker::new(&bus, db.clone(), clock.clone());

    // The input listeners share ONE keyboard source (the OS hook is
    // single-instance): a ref-counted wrapper makes the injected source
    // shareable. The hotbar listener is a producer (it publishes tool-change
    // events on the bus), gated on the hotbar-hooks toggle AND an active
    // session; the spacebar listener is composed alongside the scan services
    // over this same source. The underlying source is injected by the caller
    // (`compose_native` builds the real boundary-filtered OS hook; tests pass a
    // hook-free mock), so a generic test run cannot wedge on the hook's
    // attach/detach lifecycle. Off Windows (or in a headless run) the real hook
    // is inert, so both listeners never run.
    let keystroke_source: Arc<dyn KeystrokeSource> =
        Arc::new(SharedKeystrokeSource::new(keystroke_source));
    let hotbar = HotbarListener::new(
        bus.clone(),
        Some(keystroke_source.clone()),
        Some(build_hotbar_resolver(db.clone(), data_dir)),
    );
    // Apply the stored toggle; the source still only attaches while a session
    // is active (the listener reconciles on the session bus events).
    hotbar.set_hotbar_hooks_enabled(config.hotbar_hooks_enabled);

    let config_reader = config_service
        .lock()
        .expect("config service lock at composition")
        .reader();
    let tracker = block_on_pool(
        &runtime,
        HuntTracker::new(
            bus.clone(),
            db.clone(),
            clock.clone(),
            build_providers(db, config_reader, &config, runtime.clone()),
        ),
    )?;

    // Start the tail thread last, after every subscriber is registered,
    // so no published tick can land before the trackers can see it.
    watcher.start();

    Ok(ProducerState {
        watcher,
        tracker,
        runtime,
        domain_bus,
        config_service,
        skill_tracker,
        bus,
        hotbar,
        keystroke_source,
        quests,
    })
}

/// The hotbar slot resolver: the
/// live config maps the slot key to an equipment-library id; the row's item
/// type selects the outcome (a healing tool's per-use cost and reload from its
/// entity, a consumable's zero-cost one-off, or a weapon's per-shot cost
/// looked up by name fragment exactly as the cost provider does). An unbound
/// slot, an absent row, or a read failure yields None (no tool change).
fn build_hotbar_resolver(db: Db, data_dir: &std::path::Path) -> HotbarResolver {
    let data_dir = data_dir.to_path_buf();
    Arc::new(move |slot: &str| {
        let config = load_config_readonly(&data_dir).ok()?;
        let equip_id = config.hotbar.get(slot).and_then(Value::as_i64)?;
        let db = db.clone();
        // The hotbar listener runs on the input listener's plain OS key
        // thread (no async runtime), so the slot lookup reads through the
        // synchronous reader core rather than bridging an async query onto
        // the runtime. A read failure yields None (no tool change), exactly
        // as the swallowed async error did.
        db.with_reader_blocking(move |conn| {
            let Some((name, item_type, properties_json)) =
                hotbar_equipment_row_sync(conn, equip_id)?
            else {
                return Ok(None);
            };
            let outcome = match item_type.as_str() {
                "healing" => {
                    let (cost_ped, reload_seconds) = heal_cost_from_props(&properties_json);
                    (name, cost_ped, "healing".to_string(), reload_seconds)
                }
                "consumable" => (name, 0.0, "consumable".to_string(), 0.0),
                _ => {
                    let cost = weapon_cost_by_name(conn, &name);
                    (name, cost, "weapon".to_string(), 0.0)
                }
            };
            Ok(Some(outcome))
        })
        .ok()
        .flatten()
    })
}

/// Bridge the producer bus's frontend-facing domain topics onto the typed
/// broadcast channel: the same two topics, now carried as typed envelopes
/// end to end (the tracker and skill-scan publish sites construct them
/// directly, so there is no serialise-and-reparse step left on the path).
/// The subscriptions live in the bus (held alive by the producer spine)
/// and capture channel clones, so they need no separate registration
/// store; they drop with the bus when the spine tears down.
fn subscribe_domain_bridge(bus: &EventBus, domain_bus: &Arc<DomainBus>) {
    for topic in [Topic::TrackingSessionUpdated, Topic::ScanStatusChanged] {
        let domain_bus = domain_bus.clone();
        bus.subscribe(topic, move |event| match event {
            BusEvent::TrackingSessionUpdated(envelope) => {
                domain_bus.publish(DomainEvent::TrackingSessionUpdated(envelope.clone()));
            }
            BusEvent::ScanStatusChanged(envelope) => {
                domain_bus.publish(DomainEvent::ScanStatusChanged(envelope.clone()));
            }
            // A foreign event on a domain topic is unrepresentable at the
            // publish site; nothing to forward.
            _ => {}
        });
    }
}

/// The live equipment library behind the tracker's equipment seam:
/// profile and cost lookups over the equipment tables, and the
/// trifecta resolution over the live config plus those tables. The
/// lookups bridge onto the runtime from the synchronous trait calls
/// (the tracker invokes them inline on its own task).
struct LiveEquipmentLibrary {
    db: Db,
    reader: ConfigReader,
    runtime: tokio::runtime::Handle,
}

impl EquipmentLibrary for LiveEquipmentLibrary {
    fn weapon_profile(&self, tool_name: &str) -> EquipmentProfile {
        let tool_name = tool_name.to_string();
        let db = self.db.clone();
        let json = block_on_pool(&self.runtime, async move {
            db.weapon_properties_by_name_fragment(&tool_name)
                .await
                .ok()
                .flatten()
        })?;
        match serde_json::from_str::<Value>(&json) {
            Ok(Value::Object(map)) => Some(map),
            _ => None,
        }
    }

    fn cost_per_shot(&self, tool_name: &str) -> f64 {
        // The per-shot cost in PED derived from the profile, or 0.0 when
        // the tool is unknown.
        match self.weapon_profile(tool_name) {
            Some(props) => cost_per_shot_ped(&Value::Object(props)),
            None => 0.0,
        }
    }

    fn resolve_trifecta(&self) -> Option<Map<String, Value>> {
        // Resolve the active preset's attribution map off the live
        // config and the equipment library; the resolver discards the
        // validation reason and yields just the data.
        let config = self.reader.current();
        let preset = active_trifecta_preset(&config).map(|p| TrifectaPreset {
            small_weapon_id: p.small_weapon_id,
            big_weapon_id: p.big_weapon_id,
            heal_id: p.heal_id,
        });
        let db = self.db.clone();
        block_on_pool(&self.runtime, async move {
            describe_trifecta(&db, preset.as_ref())
                .await
                .ok()
                .and_then(|(data, _error)| data)
        })
    }
}

/// The live session-capture configuration behind the tracker's config
/// seam: every read borrows the config service's published snapshot,
/// so settings writes are visible immediately and no call touches the
/// filesystem.
struct LiveTrackingConfig {
    reader: ConfigReader,
}

impl TrackingConfig for LiveTrackingConfig {
    fn mob_tracking_mode(&self) -> String {
        self.reader.current().mob_tracking_mode.clone()
    }

    fn mob_tracking_tag(&self) -> String {
        self.reader.current().mob_tracking_tag.clone()
    }

    fn manual_mob_entry_enabled(&self) -> bool {
        self.reader.current().mob_tracking_mode != "tag"
    }

    fn manual_mob(&self) -> Option<(String, String)> {
        let config = self.reader.current();
        let species = config.manual_mob_species.trim().to_string();
        let maturity = config.manual_mob_maturity.trim().to_string();
        if species.is_empty() {
            return None;
        }
        Some((species, maturity))
    }

    fn weapon_attribution_trifecta(&self) -> bool {
        // `not hotbar_hooks_enabled`: hotbar hooks off selects
        // trifecta attribution.
        !self.reader.current().hotbar_hooks_enabled
    }

    fn loot_filter_blacklist(&self) -> Vec<String> {
        self.reader.current().loot_filter_blacklist.clone()
    }
}

/// Wire the hunt-tracker seams to their live sources: the equipment
/// library over the database, and the
/// session-capture config over the config service's live snapshot.
fn build_providers(
    db: Db,
    reader: ConfigReader,
    initial_config: &AppConfig,
    runtime: tokio::runtime::Handle,
) -> Providers {
    Providers {
        equipment: Arc::new(LiveEquipmentLibrary {
            db,
            reader: reader.clone(),
            runtime,
        }),
        config: Arc::new(LiveTrackingConfig { reader }),
        player_name: initial_config.player_name.clone(),
    }
}

/// Run a database future from inside a synchronous provider callback,
/// from either calling context: a runtime worker thread (a facade-driven
/// reload) yields its slot, while a plain producer thread parks. The
/// tracker's own `block_on` uses this exact dual shape.
fn block_on_pool<F: std::future::Future>(handle: &tokio::runtime::Handle, future: F) -> F::Output {
    // Never `Handle::current()`: the provider callbacks run on the
    // chat-log watcher's plain OS thread (no current runtime), so the
    // handle is the one captured at composition time. A runtime worker
    // thread (a facade-driven reload) yields its slot via `block_in_place`;
    // a plain producer thread parks directly. The tracker's and quest
    // service's bus forwarders wait on their reply channels in this same
    // dual shape.
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        handle.block_on(future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The hook-free keystroke source every compose test injects in place of the
    // real OS hook, so a generic test run never installs the shared hook (whose
    // attach/detach lifecycle can intermittently wedge a headless run).
    use eo_services::keystroke_source::MockKeystrokeSource;

    /// Serialises the tests that mutate process-global, not-thread-safe
    /// state: `ORT_DYLIB_PATH` (the env var ORT reads via `getenv`) and the
    /// current working directory (`set_current_dir`). Async-aware so the
    /// guard can be held across the `compose_with(...).await` that builds
    /// the engine and reads the env, without the `await_holding_lock`
    /// hazard a `std::sync::Mutex` would raise. Only ever contended across
    /// distinct tests, so no intra-test deadlock.
    static ORT_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn repo_snapshot() -> PathBuf {
        dev_project_root()
            .join("app")
            .join("src-tauri")
            .join("entropia-orme")
            .join("resources")
            .join("snapshot")
    }

    /// The repo's recogniser model+dict directory (the dev `models_dir`).
    fn repo_models() -> PathBuf {
        dev_project_root()
            .join("app")
            .join("src-tauri")
            .join("entropia-orme")
            .join("resources")
            .join("models")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn composes_over_a_fresh_data_dir_and_the_repo_snapshot() {
        let _ort = ORT_TEST_LOCK.lock().await;
        let dylib = ort_dylib_path(None);
        if dylib.is_file() {
            unsafe {
                std::env::set_var("ORT_DYLIB_PATH", &dylib);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let Composition::Ready(composed) = compose_with(
            dir.path().join("data"),
            repo_snapshot(),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await
        else {
            panic!("fresh-dir composition succeeds");
        };
        assert!(
            dir.path().join("data").join(DB_FILE_NAME).exists(),
            "the database file is created at the resolved location"
        );
        // The producer spine composed alongside the read surface; tear
        // it down so no tail thread outlives the test.
        composed.producers.stop();
    }

    #[tokio::test]
    async fn declines_on_a_quarantined_database_leaving_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join(DB_FILE_NAME);
        std::fs::write(&db_path, b"not a database").unwrap();
        let composed = compose_with(
            data_dir,
            repo_snapshot(),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await;
        assert!(
            matches!(composed, Composition::Declined),
            "quarantine declines composition"
        );
        assert_eq!(std::fs::read(&db_path).unwrap(), b"not a database");
    }

    #[tokio::test]
    async fn declines_on_a_below_baseline_database() {
        // A database the backend created but never migrated up to the baseline
        // (the hybrid-era first-launch-after-upgrade state). The Python
        // sidecar used to migrate it forward; with it decommissioned there is
        // nothing to do so, so composition now declines cleanly rather than
        // awaiting a migration that will never arrive.
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join(DB_FILE_NAME);
        {
            let db = eo_services::db::Db::open(&db_path).await.unwrap();
            db.with_writer(|conn| {
                conn.execute(
                    "UPDATE db_metadata SET value = '28' WHERE key = 'version'",
                    [],
                )?;
                conn.execute("DROP TABLE _sqlx_migrations", [])?;
                Ok(())
            })
            .await
            .unwrap();
        }
        let composed = compose_with(
            data_dir,
            repo_snapshot(),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await;
        assert!(
            matches!(composed, Composition::Declined),
            "a below-baseline database declines (nothing migrates it without the sidecar)"
        );
    }

    #[tokio::test]
    async fn declines_on_a_missing_snapshot_dir() {
        let dir = tempfile::tempdir().unwrap();
        let composed = compose_with(
            dir.path().join("data"),
            dir.path().join("no-such-snapshot"),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await;
        assert!(
            matches!(composed, Composition::Declined),
            "missing snapshot declines composition"
        );
    }

    #[test]
    fn snapshot_dir_prefers_the_repo_copy_in_dev_builds() {
        let resolved = snapshot_dir(Some(&PathBuf::from("X:/resources")));
        if cfg!(debug_assertions) {
            assert_eq!(resolved, repo_snapshot());
        } else {
            assert_eq!(resolved, PathBuf::from("X:/resources").join("snapshot"));
        }
    }

    /// DYLIB RESOLUTION: the pinned `onnxruntime.dll` path is always
    /// ABSOLUTE (never a bare filename the OS loader would resolve off
    /// `PATH`/CWD), resolves to the expected installed-vs-dev location,
    /// and does NOT depend on the process working directory.
    ///
    /// The CWD-independence is the load-bearing property: a bare name or a
    /// relative path would let a stray system `onnxruntime.dll` win, so
    /// the resolver must return the same absolute path regardless of where
    /// the process runs. We assert it directly (the resolver composes only
    /// the installed `resource_dir` and the compiled-in `dev_project_root`,
    /// neither of which reads CWD) and prove it by resolving, changing the
    /// CWD, resolving again, and asserting the two are byte-identical.
    #[tokio::test]
    async fn ort_dylib_path_is_absolute_and_cwd_independent() {
        // Serialised: this test mutates the process-global CWD, which would
        // race any concurrent test reading the CWD or a relative path.
        let _ort = ORT_TEST_LOCK.lock().await;
        // The expected leaf mirrors the resolver's platform fork: the
        // Windows onnxruntime-directml build under `ort/`, the Linux
        // build under `ort-linux/`.
        #[cfg(target_os = "linux")]
        let (subdir, file) = ("ort-linux", "libonnxruntime.so");
        #[cfg(not(target_os = "linux"))]
        let (subdir, file) = ("ort", "onnxruntime.dll");
        let resource = PathBuf::from("X:/resources");
        let resolved = ort_dylib_path(Some(&resource));

        // Always absolute, never a bare filename.
        assert!(
            resolved.is_absolute(),
            "the pinned dylib path must be absolute, got {}",
            resolved.display()
        );
        assert!(
            resolved.components().count() > 1,
            "the pinned dylib path is never a bare filename, got {}",
            resolved.display()
        );
        assert_eq!(
            resolved.file_name().and_then(|n| n.to_str()),
            Some(file),
            "the pinned path points at the platform's bundled ONNX Runtime dylib"
        );

        // The expected installed-vs-dev location.
        if cfg!(debug_assertions) {
            assert_eq!(
                resolved,
                dev_project_root()
                    .join("app")
                    .join("src-tauri")
                    .join("entropia-orme")
                    .join("resources")
                    .join(subdir)
                    .join(file),
                "dev resolves to the committed repo dylib"
            );
        } else {
            assert_eq!(
                resolved,
                resource.join(subdir).join(file),
                "installed resolves under the OS-given resource dir"
            );
        }

        // CWD-independence: resolving from a different working directory
        // yields the identical absolute path. (The dev branch ignores
        // `resource` and resolves through the compiled-in project root;
        // both branches are pure-path joins, so neither reads CWD.)
        let original_cwd = std::env::current_dir().expect("a current dir");
        let elsewhere = tempfile::tempdir().expect("temp dir");
        std::env::set_current_dir(elsewhere.path()).expect("chdir");
        let resolved_elsewhere = ort_dylib_path(Some(&resource));
        // Restore before any assertion so a failure never leaves the
        // process in the temp dir for the next test.
        std::env::set_current_dir(&original_cwd).expect("restore chdir");
        assert_eq!(
            resolved, resolved_elsewhere,
            "the resolved dylib path is identical regardless of the working directory"
        );
    }

    /// DYLIB PIN (the BLOCKER guard, on the PRODUCTION init path): pinning
    /// `ORT_DYLIB_PATH` to the absolute bundled path is what stops ort's
    /// fallback loader from ever resolving a bare `onnxruntime.dll` off the
    /// OS search order (a DLL-planting vector) when `init_from` fails. The
    /// production path (`compose_native` -> `init_ort_runtime`) does this;
    /// the engine tests use the env shortcut and never exercised it, the
    /// same production-vs-test-path gap that hid the original defect. Assert
    /// the pin sets the env to the absolute path, never a bare name. (Every
    /// writer in the suite pins the same dev path, so the shared-env set is
    /// idempotent.)
    #[tokio::test]
    async fn pin_writes_the_absolute_dylib_path_to_the_env_never_a_bare_name() {
        let _ort = ORT_TEST_LOCK.lock().await;
        let pinned = pin_ort_dylib_env(None);
        assert!(
            pinned.is_absolute(),
            "the pinned dylib path must be absolute, got {}",
            pinned.display()
        );
        assert_ne!(
            pinned.as_os_str(),
            std::ffi::OsStr::new("onnxruntime.dll"),
            "the pin must never be a bare filename the OS loader resolves off PATH/CWD"
        );
        assert_eq!(
            std::env::var("ORT_DYLIB_PATH").ok(),
            Some(pinned.to_string_lossy().into_owned()),
            "init pins ORT_DYLIB_PATH to the resolved absolute dylib"
        );
    }

    /// The models directory resolves the installed-vs-dev way, the same
    /// shape `snapshot_dir` and the dylib resolver use, so the engine
    /// reads the bundled model+dict and never a stray copy.
    #[test]
    fn models_dir_prefers_the_repo_copy_in_dev_builds() {
        let resolved = models_dir(Some(&PathBuf::from("X:/resources")));
        if cfg!(debug_assertions) {
            assert_eq!(resolved, repo_models());
        } else {
            assert_eq!(resolved, PathBuf::from("X:/resources").join("models"));
        }
    }

    /// COMPOSITION WITH A WARMED ENGINE: composing over the repo model
    /// yields a warmed engine reachable from `Composed.ocr_engine` when
    /// the ONNX Runtime is loadable on this host. Host-gated: the engine
    /// binds the runtime dynamically, so on a host without the dylib
    /// loadable the engine is `None` and the rest of composition still
    /// succeeds (OCR is optional) - we assert composition succeeded
    /// either way and that, WHEN the engine is present, it recorded a real
    /// provider and recognises a warm-up-shaped cell.
    ///
    /// The dylib is pinned via `ORT_DYLIB_PATH` (the committed repo copy)
    /// so the test can load the runtime without a system install; if the
    /// dylib is absent on this host the test skips with its reason.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn composes_with_a_warmed_ocr_engine_when_the_runtime_loads() {
        let _ort = ORT_TEST_LOCK.lock().await;
        let dylib = ort_dylib_path(None);
        if !dylib.is_file() {
            eprintln!(
                "committed ONNX Runtime dylib absent at {}; skipping",
                dylib.display()
            );
            return;
        }
        // SAFETY: set before any ORT use in this process. Process-global
        // and once-only, matching the production pin.
        unsafe {
            std::env::set_var("ORT_DYLIB_PATH", &dylib);
        }

        let dir = tempfile::tempdir().unwrap();
        let Composition::Ready(composed) = compose_with(
            dir.path().join("data"),
            repo_snapshot(),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await
        else {
            panic!("composition succeeds regardless of OCR availability");
        };

        match &composed.ocr_engine {
            Some(engine) => {
                let provider = engine.provider();
                #[cfg(target_os = "linux")]
                let gpu_provider = "WebGpuExecutionProvider";
                #[cfg(not(target_os = "linux"))]
                let gpu_provider = "DmlExecutionProvider";
                assert!(
                    provider == gpu_provider || provider == "CPUExecutionProvider",
                    "the composed engine recorded a real provider, got {provider:?}"
                );
                // The engine is genuinely live (warmed at composition):
                // a white cell recognises without panicking.
                let white = vec![255u8; 48 * 200 * 3];
                let (_text, score) = engine
                    .recognize_bgr(&white, 48, 200)
                    .expect("the composed engine recognises a warm-up-shaped cell");
                assert!(score.is_finite(), "the score is finite, got {score}");
                eprintln!("composed OCR engine provider={provider}");
            }
            None => {
                // On the platforms that bundle a runtime the dylib was
                // verified present above, so a missing engine is a real
                // regression in the composition path (the gate that once
                // skipped OCR off-Windows hid exactly this), not an
                // acceptable optional-faculty outcome.
                if cfg!(any(windows, target_os = "linux")) {
                    panic!(
                        "the committed ONNX Runtime is present on this platform, \
                         yet composition produced no OCR engine"
                    );
                }
                eprintln!(
                    "OCR engine did not load on this host (runtime/model unavailable); \
                     composition still succeeded, which is the optional-faculty contract"
                );
            }
        }

        composed.producers.stop();
    }

    /// The composed scan services are reachable and wired over the LIVE
    /// providers, not the inert defaults: the manual scan reports a resting
    /// idle status whose `configured` flag mirrors whether the OCR engine
    /// loaded (the inert default's `engine_available` is always false, so a
    /// host where the engine loads proves the real provider), and the repair
    /// reader runs its provider chain to the window-not-found leg. The scan
    /// also publishes onto the spine bus: a status-moving verb dispatches one
    /// `scan.status.changed` frame through the domain bridge composed
    /// alongside.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn composes_the_scan_services_over_live_providers() {
        let _ort = ORT_TEST_LOCK.lock().await;
        // Pin the repo dylib when present so the engine can load on a capable
        // host; absent, the services still compose with the engine `None`.
        let dylib = ort_dylib_path(None);
        if dylib.is_file() {
            unsafe {
                std::env::set_var("ORT_DYLIB_PATH", &dylib);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let Composition::Ready(composed) = compose_with(
            dir.path().join("data"),
            repo_snapshot(),
            repo_models(),
            None,
            Arc::new(MockKeystrokeSource::new()),
        )
        .await
        else {
            panic!("composition succeeds");
        };

        let status = composed.skill_scan.get_status();
        assert_eq!(status["phase"], "idle");
        assert_eq!(status["captured_pages"], 0);
        // `configured` mirrors whether the engine loaded (the real provider);
        // the game window is never present on a headless host.
        assert_eq!(status["configured"], composed.ocr_engine.is_some());
        assert_eq!(status["game_window_present"], false);
        // The scan composed on the spine bus (its status frames reach the
        // domain bridge); the forwarding is covered by
        // `the_domain_bridge_forwards_typed_envelopes_to_a_subscriber`. The
        // repair reader's no-window leg is covered by the facade test
        // `tracking_facade::repair_scan_soft_error_rides_the_body`.

        composed.producers.stop();
    }

    use std::io::Write as _;
    use std::time::Duration;

    use eo_services::clock::MockClock;

    /// Build the producer spine alone over an injected clock, pool, and
    /// explicit chat-log, mirroring `compose_with`'s producer step. The
    /// integration tests drive a deterministic replay through this spine
    /// without the data-dir/snapshot resolution `compose_native` does.
    fn compose_producers_for_test(
        db: Db,
        clock: Arc<dyn Clock>,
        data_dir: &std::path::Path,
        chatlog: PathBuf,
    ) -> ProducerState {
        compose_producers(
            db,
            clock,
            data_dir,
            Some(chatlog),
            Arc::new(MockKeystrokeSource::new()),
        )
        .expect("producer spine composes")
    }

    /// A composed-spine replay: feed a recorded-shape chat-log through
    /// the *composed* watcher (shared bus, shared single-owner pool,
    /// injected clock) and assert the composed hunt tracker persisted
    /// the expected session and kill rows. This proves the real-provider
    /// wiring and the shared bus/clock/single-Db composition preserve the
    /// pipeline; it does not claim byte-identical parity with the
    /// default-provider corpus goldens (the real providers stamp mobs and
    /// blacklist loot, which the inert defaults do not).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn composed_spine_replays_a_scenario_into_the_expected_db_rows() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        // Open the database the same single-owner way the substrate does.
        let db = Db::open_adopted(&data_dir.join(DB_FILE_NAME))
            .await
            .expect("fresh database adopts");

        // A frozen, plan-advanced clock, exactly the corpus oracle's
        // protocol: the watcher guards its own drain timeout against a
        // frozen clock internally.
        let start =
            chrono::NaiveDateTime::parse_from_str("2026-05-19 10:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(Some(start), 0.0));

        let chatlog = data_dir.join("chat_replay.log");
        std::fs::File::create(&chatlog).expect("empty chatlog exists before the watcher starts");

        let producers =
            compose_producers_for_test(db.clone(), clock.clone(), &data_dir, chatlog.clone());
        producers
            .tracker()
            .start_session()
            .await
            .expect("composed session starts");

        // Three lines across three ticks: a combat tick, then two loot
        // ticks that each close a kill.
        let appended = 3u64;
        {
            let mut sink = std::fs::OpenOptions::new()
                .append(true)
                .open(&chatlog)
                .expect("chatlog append");
            // One flush per tick so the tail never sees EOF mid-tick.
            sink.write_all(
                b"2026-05-19 10:00:01 [System] [] You inflicted 12.0 points of damage\n",
            )
            .unwrap();
            sink.flush().unwrap();
            sink.write_all(
                b"2026-05-19 10:00:02 [System] [] You received Shrapnel x (500) Value: 5.00 PED\n",
            )
            .unwrap();
            sink.flush().unwrap();
            sink.write_all(b"2026-05-19 10:00:03 [System] [] You received Wool Value: 1.50 PED\n")
                .unwrap();
            sink.flush().unwrap();
        }
        producers
            .watcher()
            .wait_until_drained(appended, Duration::from_secs(10))
            .expect("composed watcher drains the scenario");

        // A snapshot proves the live tracker accumulated through the
        // composed bus.
        let readout = producers.tracker().snapshot().await.expect("snapshot");
        let active = readout.active.expect("a session is active");
        assert_eq!(active.kill_count, 2, "two loot groups, two kills");

        producers
            .tracker()
            .stop_session()
            .await
            .expect("session stops");
        producers.stop();

        // The persisted rows match: one session, two kills.
        let session_count: i64 = db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM tracking_sessions WHERE is_active = 0",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(session_count, 1, "one closed session persisted");
        let kill_count: i64 = db
            .with_reader(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM kills", [], |row| row.get::<_, i64>(0))?)
            })
            .await
            .unwrap();
        assert_eq!(kill_count, 2, "two kills persisted by the composed tracker");
    }

    /// `ProducerState::stop` must end any session left open: it is the
    /// exit seam's one chance to close the session cleanly while the bus
    /// is still live. Start a session through the composed tracker, then
    /// call `stop()` WITHOUT a prior `stop_session()`, and assert the
    /// open session was ended (its `tracking_sessions` row flipped to
    /// `is_active = 0` with an `ended_at` stamp). Replacing `stop`'s body
    /// with `()` would leave the session open, so this assertion fails:
    /// the mutant is killed.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_ends_an_open_session_left_running() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let db = Db::open_adopted(&data_dir.join(DB_FILE_NAME))
            .await
            .expect("fresh database adopts");

        let start =
            chrono::NaiveDateTime::parse_from_str("2026-05-19 10:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap();
        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(Some(start), 0.0));

        let chatlog = data_dir.join("chat_replay.log");
        std::fs::File::create(&chatlog).expect("empty chatlog exists before the watcher starts");

        let producers =
            compose_producers_for_test(db.clone(), clock.clone(), &data_dir, chatlog.clone());
        producers
            .tracker()
            .start_session()
            .await
            .expect("composed session starts");

        // Before stop: exactly one active session row exists.
        let active_before: i64 = db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM tracking_sessions WHERE is_active = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(active_before, 1, "a session is open before stop");
        assert!(
            producers.tracker().is_tracking(),
            "the tracker reports an active session before stop"
        );

        // Stop the spine WITHOUT a prior stop_session: stop() itself must
        // end the open session. A stop body replaced with () leaves the
        // session open and fails the assertions below.
        producers.stop();

        let active_after: i64 = db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM tracking_sessions WHERE is_active = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(active_after, 0, "stop() ended the open session");
        let ended_set: i64 = db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM tracking_sessions WHERE is_active = 0 AND ended_at IS NOT NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(ended_set, 1, "the closed session carries an ended_at stamp");
    }

    /// Seed a weapon row directly through the shared handle, the same
    /// shape `Db::weapon_properties_by_name_fragment` reads (item_type
    /// 'weapon', a name carrying the lookup fragment, a JSON-object
    /// properties blob).
    async fn seed_weapon(db: &Db, id: i64, name: &str, properties_json: &str) {
        let name = name.to_string();
        let properties_json = properties_json.to_string();
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO equipment_library (id, name, item_type, properties_json) \
                 VALUES (?1, ?2, 'weapon', ?3)",
                rusqlite::params![id, name, properties_json],
            )?;
            Ok(())
        })
        .await
        .expect("weapon row seeds");
    }

    /// Write a minimal `settings.json` carrying just the keys a test
    /// pins; the read-through loader reads any JSON object and defaults
    /// the rest, so a partial object is enough to exercise the
    /// config-derived providers' live reads.
    fn write_settings(data_dir: &std::path::Path, settings: &Value) {
        std::fs::write(
            data_dir.join("settings.json"),
            serde_json::to_string(settings).unwrap(),
        )
        .expect("settings.json writes");
    }

    /// The `build_providers` transforms, each invoked against on-disk
    /// fixtures so a mutation to any one transform is observable:
    ///
    /// - equipment.weapon_profile returns the parsed property object for a
    ///   weapon matched by name fragment (kills the deleted `Ok(Object)`
    ///   match arm and the `Default::default()` whole-function replacement,
    ///   both of which yield `None`).
    /// - equipment.cost_per_shot returns `totalCostPerUse / 100`; the seeded
    ///   props make totalCostPerUse == 250 so the expected 2.5 differs from
    ///   both `250 % 100` (50.0) and `250 * 100` (25000.0): kills the `/`->`%`
    ///   and `/`->`*` mutants.
    /// - manual_mob_entry_enabled is `mob_tracking_mode != "tag"`: false
    ///   under "tag", true under "mob" (kills `!=`->`==`, which flips both).
    /// - weapon_attribution_trifecta is `!hotbar_hooks_enabled`: false when
    ///   hooks are on, true when off (kills the deleted `!`, which flips both).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn build_providers_transforms_pin_their_exact_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let db = Db::open_adopted(&data_dir.join(DB_FILE_NAME))
            .await
            .expect("fresh database adopts");

        // A weapon whose name carries the fragment "Korss", with a
        // property object whose economy yields a known totalCostPerUse.
        // ammo_burn 25000 -> 250 PEC ammo at markup 1.0, decay 0 ->
        // totalCostPerUse == 250, so equipment.cost_per_shot == 250/100 == 2.5.
        let props = serde_json::json!({
            "weapon_entity": {"economy": {"decay": 0.0, "ammo_burn": 25000}},
            "weapon_markup": 100,
        });
        seed_weapon(
            &db,
            1,
            "Korss H400 (L)",
            &serde_json::to_string(&props).unwrap(),
        )
        .await;

        // First on-disk config: tag mode, hotbar hooks ENABLED.
        write_settings(
            &data_dir,
            &serde_json::json!({
                "mob_tracking_mode": "tag",
                "hotbar_hooks_enabled": true,
            }),
        );
        let config = load_config_readonly(&data_dir).expect("config reads");
        let mut config_service = ConfigService::new(&data_dir).expect("config service");
        let providers = build_providers(
            db.clone(),
            config_service.reader(),
            &config,
            tokio::runtime::Handle::current(),
        );

        // The equipment seam: the parsed property object, by fragment.
        let profile = providers
            .equipment
            .weapon_profile("Korss")
            .expect("the seeded weapon resolves to a property object");
        assert_eq!(
            profile.get("weapon_markup").and_then(Value::as_f64),
            Some(100.0),
            "the profile carries the seeded keys"
        );
        assert!(
            profile.contains_key("weapon_entity"),
            "the profile carries the weapon entity"
        );

        // cost_per_shot: totalCostPerUse (250) / 100 == 2.5.
        let cost = providers.equipment.cost_per_shot("Korss");
        assert!(
            (cost - 2.5).abs() < 1e-9,
            "per-shot cost is totalCostPerUse/100 == 2.5, not {cost} \
             (% would be 50.0, * would be 25000.0)"
        );

        // The config seam reads the live snapshot: under tag mode +
        // hooks-on, manual entry is disabled and trifecta attribution
        // is off.
        assert!(
            !providers.config.manual_mob_entry_enabled(),
            "manual mob entry is disabled in tag mode"
        );
        assert!(
            !providers.config.weapon_attribution_trifecta(),
            "trifecta attribution is off when hotbar hooks are enabled"
        );

        // A config-service write publishes a new snapshot, and the SAME
        // seam follows it: mob mode + hooks-off -> manual entry enabled,
        // trifecta attribution on.
        let mut updates = serde_json::Map::new();
        updates.insert("mob_tracking_mode".into(), serde_json::json!("mob"));
        updates.insert("hotbar_hooks_enabled".into(), serde_json::json!(false));
        config_service.update(&updates).expect("settings write");
        assert!(
            providers.config.manual_mob_entry_enabled(),
            "manual mob entry is enabled outside tag mode"
        );
        assert!(
            providers.config.weapon_attribution_trifecta(),
            "trifecta attribution is on when hotbar hooks are disabled"
        );
    }

    /// REGRESSION: the equipment seam must resolve from a plain OS
    /// thread with NO current tokio runtime. Its production caller is
    /// now the tracker's actor task (a runtime context, bridged via
    /// `block_in_place`), but the seam's `block_on_pool` keeps the
    /// composition-time handle rather than `Handle::current()` (which
    /// panics off-runtime), so it stays robust from either context.
    /// Build the providers inside the runtime, then invoke the lookup
    /// from a plain `std::thread` and assert it resolves.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn equipment_provider_resolves_from_a_non_runtime_thread() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let db = Db::open_adopted(&data_dir.join(DB_FILE_NAME))
            .await
            .expect("fresh database adopts");
        let props = serde_json::json!({
            "weapon_entity": {"economy": {"decay": 0.0, "ammo_burn": 25000}},
            "weapon_markup": 100,
        });
        seed_weapon(
            &db,
            1,
            "Korss H400 (L)",
            &serde_json::to_string(&props).unwrap(),
        )
        .await;
        write_settings(&data_dir, &serde_json::json!({}));
        let config = load_config_readonly(&data_dir).expect("config reads");
        let config_service = ConfigService::new(&data_dir).expect("config service");
        let providers = build_providers(
            db.clone(),
            config_service.reader(),
            &config,
            tokio::runtime::Handle::current(),
        );

        // Invoke the lookup AND the derived cost from a plain OS thread
        // (no current runtime).
        let equipment = providers.equipment.clone();
        let outcome = std::thread::spawn(move || {
            let resolved = equipment.weapon_profile("Korss").is_some();
            let cost = equipment.cost_per_shot("Korss");
            (resolved, cost)
        })
        .join()
        .expect("the provider must not panic off-runtime");
        assert!(
            outcome.0,
            "equipment.weapon_profile resolves from a non-runtime thread"
        );
        assert!(
            (outcome.1 - 2.5).abs() < 1e-9,
            "equipment.cost_per_shot resolves off-runtime to 2.5, got {}",
            outcome.1
        );
    }

    /// The single-owner core tolerates a facade-shaped read concurrent
    /// with a producer-shaped write without deadlock and within a bounded
    /// latency: the writer thread serialises the write, while a reader
    /// thread serves the concurrent read against the WAL, so the read
    /// simply queues behind the write rather than locking.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_read_during_a_producer_write_does_not_deadlock() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_adopted(&dir.path().join(DB_FILE_NAME))
            .await
            .expect("fresh database adopts");

        // A producer-shaped write: a short transaction holding the writer
        // connection briefly, exactly the shape of a tracker persistence
        // write.
        let writer = {
            let db = db.clone();
            tokio::spawn(async move {
                db.with_writer(|conn| {
                    let tx = conn.transaction()?;
                    tx.execute(
                        "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES ('cc-test', 0, 0)",
                        [],
                    )?;
                    // Hold the writer connection a moment so the read genuinely contends.
                    std::thread::sleep(Duration::from_millis(50));
                    tx.commit()?;
                    Ok(())
                })
                .await
                .expect("write under transaction");
            })
        };

        // A facade-shaped read on the same handle, bounded by a generous
        // deadline: if the single connection deadlocked, this would
        // time out.
        let read = tokio::time::timeout(Duration::from_secs(5), async {
            db.with_reader(|conn| {
                Ok(
                    conn.query_row("SELECT COUNT(*) FROM tracking_sessions", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                )
            })
            .await
            .expect("read query")
        })
        .await;
        assert!(
            read.is_ok(),
            "the concurrent read completed without deadlock"
        );

        writer.await.expect("writer task joins");
        let final_count = db
            .with_reader(|conn| {
                Ok(
                    conn.query_row("SELECT COUNT(*) FROM tracking_sessions", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                )
            })
            .await
            .unwrap();
        assert_eq!(final_count, 1, "the write committed");
    }

    #[tokio::test]
    async fn the_domain_bridge_forwards_typed_envelopes_to_a_subscriber() {
        use eo_services::bus_events::BusEvent;
        use eo_wire::domain_events::{
            ScanPhase, ScanStatusChanged, ScanStatusChangedPayload, ScanStatusChangedTag,
            TrackingReason, TrackingSessionUpdated, TrackingSessionUpdatedPayload,
            TrackingSessionUpdatedTag, TrackingStatus,
        };

        let bus = EventBus::new();
        let domain_bus = Arc::new(DomainBus::new(DOMAIN_BUS_CAPACITY));
        subscribe_domain_bridge(&bus, &domain_bus);
        let mut receiver = domain_bus.subscribe();

        // A tracking envelope published on the bus (typed end to end,
        // exactly as the tracker publishes it) is forwarded to the
        // channel's subscriber unchanged.
        let tracking = TrackingSessionUpdated {
            topic: TrackingSessionUpdatedTag,
            event_version: 1,
            occurred_at: "2026-01-01T00:00:00Z".to_string(),
            payload: TrackingSessionUpdatedPayload {
                session_id: Some("s1".to_string()),
                status: TrackingStatus::Active,
                reason: TrackingReason::Started,
            },
        };
        bus.publish(&BusEvent::TrackingSessionUpdated(tracking.clone()));
        assert_eq!(
            receiver.recv().await.expect("the envelope arrives"),
            DomainEvent::TrackingSessionUpdated(tracking)
        );

        // The other bridged topic is delivered too, on the same channel.
        let scan = ScanStatusChanged {
            topic: ScanStatusChangedTag,
            event_version: 1,
            occurred_at: "2026-01-01T00:00:01Z".to_string(),
            payload: ScanStatusChangedPayload {
                phase: ScanPhase::Capturing,
            },
        };
        bus.publish(&BusEvent::ScanStatusChanged(scan.clone()));
        assert_eq!(
            receiver.recv().await.expect("the envelope arrives"),
            DomainEvent::ScanStatusChanged(scan)
        );
    }

    /// The LIVE scan-status path now the scan composes on the spine bus: a
    /// status-moving verb on a `SkillScanManual` built over the same bus the
    /// bridge subscribes reaches a domain-channel subscriber as a typed
    /// `scan.status.changed` envelope, end to end (the bridge-forwards test
    /// publishes the event directly; this proves the scan's own publish
    /// flows through it).
    #[tokio::test]
    async fn the_composed_scan_delivers_a_status_envelope_to_a_subscriber() {
        use eo_wire::domain_events::ScanPhase;

        let bus = Arc::new(EventBus::new());
        let domain_bus = Arc::new(DomainBus::new(DOMAIN_BUS_CAPACITY));
        subscribe_domain_bridge(&bus, &domain_bus);
        let mut receiver = domain_bus.subscribe();

        let clock: Arc<dyn Clock> = Arc::new(MockClock::new(None, 0.0));
        let scan = SkillScanManual::new(
            ScanProviders {
                engine_available: Arc::new(|| true),
                skill_region: Arc::new(|| Some(([0, 0], [100, 200]))),
                capture_region: Arc::new(|_| Some(vec![1, 2, 3])),
                extract_page_levels: Arc::new(|_: &[u8]| Vec::new()),
            },
            clock,
            Some(bus.clone()),
            None,
            0,
        );
        // `start` moves the status idle -> capturing, publishing one envelope.
        scan.start(Some(2));
        let event = receiver.recv().await.expect("the envelope arrives");
        let DomainEvent::ScanStatusChanged(envelope) = event else {
            panic!("the scan publish routed to the wrong variant: {event:?}");
        };
        assert_eq!(envelope.payload.phase, ScanPhase::Capturing);
    }
}
