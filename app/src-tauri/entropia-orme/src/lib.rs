mod commands;
mod composition;
mod crash;
mod resources;
mod telemetry;
mod updater;

#[cfg(feature = "e2e-stub")]
mod e2e_stub;

use std::sync::Mutex;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

#[tauri::command]
fn toggle_overlay(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            // The overlay is a pre-spawned hidden window shown without focus, so
            // no focus/visibility event reaches its webview on show. Signal the
            // show explicitly so it can re-read config/runtime state that no
            // backend event announces (otherwise it stays stale until restart).
            let _ = app.emit("overlay-shown", ());
        }
    }
}

#[tauri::command]
fn show_scan_overlay(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("scan-overlay") {
        // Position near top-left so the overlay never collides with a
        // bottom-right docked in-game skills/professions panel.
        let _ = window.set_position(tauri::PhysicalPosition::new(40, 40));
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn hide_scan_overlay(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("scan-overlay") {
        let _ = window.hide();
    }
}

/// The manual-scan capture preview PNG for `page`, base64-encoded for an
/// `<img>` `data:` URL. The facade returns raw image bytes, which cannot ride
/// the typed-DTO command surface (the bindings are JSON), so this stays a
/// bespoke command outside the manifest, base64-encoding the bytes here.
#[tauri::command]
async fn capture_png(app: tauri::AppHandle, page: u32) -> Result<String, String> {
    use base64::Engine as _;
    let facade = commands::facade(&app).map_err(|error| error.to_string())?;
    let bytes = facade
        .scan_capture_png(i64::from(page))
        .map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

// Holds the substrate's live producer spine so the Tauri exit seam can
// stop it deterministically. The substrate task composes the producers
// inside its own async context (after the database opens) and hands the
// spine here; `RunEvent::Exit` then stops the chat-log tail thread and
// ends any open session before the process tears down. There is no
// graceful-shutdown signal into the substrate's compose path, so this exit
// seam is the producer teardown path.
struct Producers(Mutex<Option<composition::ProducerState>>);

// Holds the substrate's warmed OCR engine for the app's lifetime. The
// composition root constructs and warms it (binding the bundled ONNX
// Runtime) and the scan services capture their own clones at compose
// time; this managed handle is the lifetime anchor that keeps the warmed
// engine pinned for the whole substrate regardless of consumer teardown
// order. Unlike `Producers`, there is no exit-seam stop: the engine owns
// no thread and no subscription, its ONNX session drops with this managed
// state, and the ORT environment self-releases via its own process-exit
// hook. `None` when the runtime or model was unavailable (OCR is an
// optional faculty).
//
// The handle is held, never read: the scan services reach the engine
// through their own clones. The `dead_code` allow is scoped to this one
// field (not the struct, not the module) and justified by that
// held-not-read intent, the same shape the producer spine documents for
// its kept-alive bus subscriptions.
struct OcrEngineState(#[allow(dead_code)] Mutex<Option<std::sync::Arc<composition::OcrEngine>>>);

// Holds the manual-scan input listener and the scan state machine so the exit
// seam tears them down: the spacebar listener detaches its share of the shared
// OS keyboard hook (the hotbar listener detaches the other share via
// `ProducerState::stop`), and the scan resets any in-flight capture state.
// Both are the same `Arc`s the facade's scan family serves over.
struct ScanInput {
    spacebar: std::sync::Arc<composition::SpacebarCaptureListener>,
    skill_scan: std::sync::Arc<composition::SkillScanManual>,
}

// Holds the composed database handle so the exit seam can run the
// once-per-lifecycle `PRAGMA optimize` at shutdown. The composition task
// hands it here once the database opens; the typed-command facade holds its
// own clone for serving.
struct ShutdownDb(eo_services::db::Db);

#[cfg(windows)]
struct RuntimeWindowIcons(Mutex<Vec<isize>>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the process-wide tracing subscriber first, before anything
    // else runs, so every diagnostic and every instrumented seam is captured
    // from the first instant. The guard is held for the whole process so the
    // rolling log appender flushes at exit.
    let _telemetry = telemetry::init();

    // Install the default-off, opt-in crash reporter's panic hook. By default
    // it adds nothing to the standard panic behaviour; only when the user has
    // opted in does a panic write a PII-scrubbed, local-only report.
    crash::install_panic_hook(composition::data_dir());

    // Start the periodic resource sampler feeding the drift gauges (the metrics
    // page reads them live; each sample is also logged so the rolling file
    // carries the resource-drift series for long-running-session leak detection).
    resources::spawn_resource_sampler();

    let app = tauri::Builder::default()
        // The shell plugin stays for its `open` API (external links route to
        // the OS browser via `$lib/utils/openExternal`); the sidecar/execute
        // usage was removed when the Python backend was decommissioned.
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            toggle_overlay,
            show_scan_overlay,
            hide_scan_overlay,
            capture_png,
            // The typed IPC commands (held in lock-step with the eo-api
            // manifest by the parity test in `commands`).
            commands::equipment_search,
            commands::equipment_library,
            commands::equipment_add,
            commands::equipment_update,
            commands::equipment_delete,
            commands::equipment_detail,
            commands::character_calibration,
            commands::character_stats,
            commands::character_skills,
            commands::character_professions,
            commands::character_prospect_options,
            commands::character_prospect,
            commands::character_profession_optimizer,
            commands::character_path_optimizer,
            commands::character_hp_optimizer,
            commands::settings_get,
            commands::settings_overlay_position,
            commands::settings_set_overlay_position,
            commands::settings_update,
            commands::codex_species,
            commands::codex_species_ranks,
            commands::codex_recommend,
            commands::codex_meta_attributes,
            commands::codex_calibrate,
            commands::codex_claim,
            commands::codex_unclaim,
            commands::codex_meta_claim,
            commands::quests_list,
            commands::quest_get,
            commands::quest_create,
            commands::quest_update,
            commands::quest_delete,
            commands::quest_start,
            commands::quest_complete,
            commands::quest_cancel,
            commands::quests_mobs,
            commands::quests_analytics,
            commands::playlists_list,
            commands::playlist_create,
            commands::playlist_update,
            commands::playlist_delete,
            commands::playlists_analytics,
            commands::analytics_overview,
            commands::analytics_activity,
            commands::ledger_list,
            commands::ledger_create,
            commands::ledger_delete,
            commands::ledger_presets_list,
            commands::ledger_preset_create,
            commands::ledger_preset_delete,
            commands::inventory_list,
            commands::inventory_create,
            commands::inventory_update,
            commands::inventory_delete,
            commands::inventory_sell,
            commands::scan_status,
            commands::scan_start,
            commands::scan_capture,
            commands::scan_cancel,
            commands::scan_undo,
            commands::scan_process,
            commands::scan_accept,
            commands::scan_reject,
            commands::scan_pending,
            commands::scan_spacebar_capture,
            commands::tracking_sessions,
            commands::tracking_session_detail,
            commands::tracking_tag_suggestions,
            commands::tracking_manual_mob_suggestions,
            commands::tracking_snapshot,
            commands::tracking_quest_link_suggestion,
            commands::tracking_start,
            commands::tracking_stop,
            commands::tracking_release_mob,
            commands::tracking_manual_mob_lock,
            commands::tracking_tag_lock,
            commands::tracking_rename_mob,
            commands::tracking_restore_mob,
            commands::tracking_loot_item_activate,
            commands::tracking_loot_item_deactivate,
            commands::tracking_armour_cost,
            commands::tracking_quest_link,
            commands::tracking_repair_scan,
            commands::tracking_session_delete,
            commands::demo_analytics_overview,
            commands::demo_analytics_activity,
            commands::demo_ledger_list,
            commands::demo_ledger_presets_list,
            commands::demo_inventory_list,
            commands::demo_tracking_sessions,
            commands::demo_tracking_session_detail,
            commands::demo_tracking_snapshot,
            commands::dev_metrics,
            commands::dev_crash_reporting,
            commands::dev_set_crash_reporting,
            commands::dev_compact_database,
            commands::dev_rebuild_projections,
            updater::check_for_update,
            updater::download_update,
            updater::install_update,
            updater::get_update_channel,
            updater::set_update_channel
        ])
        .setup(|app| {
            // `app` is unused on a non-Windows debug build (the runtime icon
            // install is Windows-only); keep the binding alive there.
            let _ = &app;
            // Hold the downloaded-but-not-yet-installed update between the
            // updater's download and install commands (the deferred-install
            // split). Registered before any command can run.
            app.manage(updater::PendingUpdate::default());
            #[cfg(windows)]
            install_runtime_window_icons(app.handle());
            // The single pure-Rust binary: the frontend reaches the backend
            // through the in-process IPC command (no inbound socket) and every
            // route is served natively (the Python sidecar has been
            // decommissioned). Startup composes the native spine off
            // the setup path and publishes it to the IPC command when ready.
            // Dev and release compose identically; the resource dir (the
            // bundled snapshot / model / demo assets) resolves only in the
            // installed build, dev falling back to the repository copies.
            compose_substrate(app.handle().clone(), app.path().resource_dir().ok());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The overlay + scan-overlay windows are configured invisible
            // but still count toward Tauri's "exit when all windows close"
            // tally — closing main alone leaves them open and the app
            // keeps running headless. Treat main-window close as a
            // request to exit the whole app.
            if let WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            run_exit_teardown(app);
        }
    });
}

/// Wind the app's live faculties down deterministically before the process
/// ends. Driven from two paths: the Tauri `RunEvent::Exit` seam (a normal
/// quit), and the updater's `on_before_exit` hook (an updater install calls
/// `std::process::exit`, which bypasses the event loop, so that path must run
/// this itself). Idempotent: the producer handle is taken out under its lock,
/// so a second call is a no-op.
pub(crate) fn run_exit_teardown(app: &tauri::AppHandle) {
    #[cfg(windows)]
    destroy_runtime_window_icons(app);

    // Stop the producer spine: end any open session so its stop events
    // publish, then stop the chat-log tail thread. The substrate's compose
    // task has no shutdown signal, so this is where the producers wind down.
    if let Some(state) = app.try_state::<Producers>() {
        if let Ok(mut guard) = state.0.lock() {
            if let Some(producers) = guard.take() {
                producers.stop();
            }
        }
    }

    // Tear down the scan input listener and reset the scan: the spacebar
    // listener detaches its share of the shared OS hook.
    if let Some(state) = app.try_state::<ScanInput>() {
        state.spacebar.stop();
        state.skill_scan.shutdown();
    }

    // With the producers stopped (no writes in flight), refresh the database's
    // planner statistics via `PRAGMA optimize` before the connection closes: the
    // recommended once-per-lifecycle maintenance call, kept off the hot path by
    // running only here at exit.
    if let Some(db) = app.try_state::<ShutdownDb>() {
        if tauri::async_runtime::block_on(db.0.optimize_on_shutdown()) {
            tracing::info!(target: "eo::db", "ran PRAGMA optimize on shutdown");
        }
    }
}

#[cfg(windows)]
fn install_runtime_window_icons(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match windows_runtime_icons::set_for_main_window(&window) {
            Ok(handles) => {
                app.manage(RuntimeWindowIcons(Mutex::new(handles)));
            }
            Err(err) => tracing::warn!(target: "eo::icon", "runtime icon install failed: {err}"),
        }
    }
}

#[cfg(windows)]
fn destroy_runtime_window_icons(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<RuntimeWindowIcons>() {
        if let Ok(mut handles) = state.0.lock() {
            windows_runtime_icons::destroy(handles.drain(..));
        }
    }
}

/// Compose the native backend spine and publish it to the typed commands.
/// The single pure-Rust binary serves every backend call over a typed Tauri
/// command dispatched into the composed facade: there is no socket, no
/// sidecar, and no proxy. This composes the native service spine off the setup
/// path, and on success installs the services, publishes the facade to the
/// typed commands, and signals the frontend (see [`install_native_services`]).
/// Until composition lands the typed commands answer their not-ready contract;
/// the frontend's initial reads are re-driven by the
/// `substrate:native-installed` event the install emits (there is no
/// transport-level retry). A declined composition is logged and the backend
/// does not come up for the session (an unopenable database, or one below the
/// supported baseline the retired sidecar used to migrate forward).
fn compose_substrate(app: tauri::AppHandle, resource_dir: Option<std::path::PathBuf>) {
    tauri::async_runtime::spawn(async move {
        match composition::compose_native(resource_dir).await {
            composition::Composition::Ready(composed) => {
                install_native_services(&app, composed);
            }
            composition::Composition::Declined => {
                tracing::error!(
                    target: "eo::substrate",
                    "native services did not compose; the backend is unavailable for this session"
                );
            }
        }
    });
}

/// Install the composed services into the Tauri-managed exit seam, then publish
/// the facade to the typed commands (so they answer only once every service is
/// present) and signal the frontend that the backend is live.
fn install_native_services(app: &tauri::AppHandle, composed: composition::Composed) {
    // Clones for the exit seam, taken before the originals move into the
    // installed bundle below: the spacebar listener detaches its share of
    // the shared OS hook and the scan resets in-flight state on close.
    let exit_spacebar = composed.spacebar_listener.clone();
    let exit_skill_scan = composed.skill_scan.clone();
    // The typed-command facade, published to its managed slot below.
    let composed_api = composed.api.clone();
    // The composed database handle, held so the exit seam can run the
    // once-per-lifecycle `PRAGMA optimize` on close.
    let shutdown_db = composed.db.clone();
    // Forward the producer spine's domain events onto the Tauri event bus, the
    // native replacement for the frontend's old EventSource relay. Subscribes
    // the producer spine's typed domain channel while the spine is still in
    // hand (it moves into managed state just below).
    spawn_domain_event_bridge(app, &composed.producers.domain_bus_handle());
    // Hand the producer spine to the exit seam so it stops the tail thread,
    // the hotbar listener, and ends any session on close.
    app.manage(Producers(Mutex::new(Some(composed.producers))));
    // Hold the warmed OCR engine for the app's lifetime (no exit stop: the
    // session drops with the managed state, the ORT env self-releases at
    // process exit).
    app.manage(OcrEngineState(Mutex::new(composed.ocr_engine)));
    // Hand the scan input listener and the scan state machine to the exit
    // seam for deterministic teardown.
    app.manage(ScanInput {
        spacebar: exit_spacebar,
        skill_scan: exit_skill_scan,
    });
    // Hold the database handle for the exit-seam `PRAGMA optimize`.
    app.manage(ShutdownDb(shutdown_db));
    // Publish the composed facade to the typed commands LAST: until now the
    // typed commands answer their not-ready contract, so by the time any
    // request dispatches every native service is present (there is no
    // absent-service window to fall back from). Then signal the frontend that
    // the backend is live so it (re-)hydrates its initial reads.
    app.manage(commands::ApiFacade(composed_api));
    let _ = app.emit("substrate:native-installed", ());
}

/// Map a dotted domain wire topic to its colon-form Tauri event name (Tauri
/// event names forbid dots). Mirrors the frontend's `toTauriEventName`.
fn domain_topic_to_tauri_event(topic: &str) -> String {
    topic.replace('.', ":")
}

/// Forward the producer spine's domain events onto the Tauri event bus: the
/// native replacement for the frontend's old `EventSource` relay. Subscribes
/// the producer spine's typed domain channel and re-emits each envelope on
/// the colon-form Tauri topic every window subscribes to
/// (`tracking:session:updated`, `scan:status:changed`). The webview sees the
/// identical envelope JSON the wire contract pins (`eo-wire`'s serde shape),
/// so the topic-aware consumers are unchanged. A lagging receiver skips
/// ahead to live events (drop-oldest, the same shedding the frame queues
/// applied); the channel closing ends the task with the spine. The hydrate
/// nudge (a payload-less frame on start) stays frontend-owned: it must fire
/// after the webview is listening, which an emit at install time cannot
/// guarantee on a cold load.
fn spawn_domain_event_bridge(
    app: &tauri::AppHandle,
    domain_bus: &std::sync::Arc<eo_wire::bus::DomainBus>,
) {
    let mut receiver = domain_bus.subscribe();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let _ = app.emit(domain_topic_to_tauri_event(event.topic()).as_str(), &event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        target: "eo::substrate",
                        skipped,
                        "domain-event bridge lagged; skipping to live events"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(windows)]
mod windows_runtime_icons {
    use tauri::WebviewWindow;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{HWND, LPARAM, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::{
                HiDpi::GetDpiForWindow,
                WindowsAndMessaging::{
                    DestroyIcon, LoadImageW, SendMessageW, HICON, ICON_BIG, ICON_SMALL,
                    ICON_SMALL2, IMAGE_ICON, LR_DEFAULTCOLOR, WM_SETICON,
                },
            },
        },
    };

    const APP_ICON_RESOURCE_ID: u16 = 32512;
    const USER_DEFAULT_SCREEN_DPI: u32 = 96;
    const ICON_SIZES: [i32; 8] = [16, 20, 24, 32, 48, 64, 128, 256];

    pub fn set_for_main_window(window: &WebviewWindow) -> Result<Vec<isize>, String> {
        let hwnd = window
            .hwnd()
            .map_err(|err| format!("failed to resolve main HWND: {err}"))?;
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi = if dpi == 0 {
            USER_DEFAULT_SCREEN_DPI
        } else {
            dpi
        };

        // Tauri's default Windows path decodes only the first ICO entry and
        // sets ICON_SMALL. Load from the embedded icon group instead so the
        // taskbar gets a DPI-appropriate ICON_BIG handle.
        let small_icon_size = choose_icon_size(16, dpi);
        let taskbar_icon_size = choose_icon_size(24, dpi);

        let small_icon = load_icon(small_icon_size)?;
        let taskbar_icon = load_icon(taskbar_icon_size)?;

        set_window_icon(hwnd, ICON_SMALL, small_icon);
        set_window_icon(hwnd, ICON_SMALL2, small_icon);
        set_window_icon(hwnd, ICON_BIG, taskbar_icon);

        Ok(vec![small_icon.0 as isize, taskbar_icon.0 as isize])
    }

    pub fn destroy(handles: impl Iterator<Item = isize>) {
        for handle in handles {
            let _ = unsafe { DestroyIcon(HICON(handle as _)) };
        }
    }

    fn choose_icon_size(base_size: u32, dpi: u32) -> i32 {
        let desired = base_size
            .saturating_mul(dpi)
            .div_ceil(USER_DEFAULT_SCREEN_DPI) as i32;
        ICON_SIZES
            .iter()
            .copied()
            .find(|size| *size >= desired)
            .unwrap_or(256)
    }

    fn load_icon(size: i32) -> Result<HICON, String> {
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }
            .map(Into::into)
            .ok();
        let resource = PCWSTR::from_raw(APP_ICON_RESOURCE_ID as usize as *const u16);
        let handle =
            unsafe { LoadImageW(module, resource, IMAGE_ICON, size, size, LR_DEFAULTCOLOR) }
                .map_err(|err| format!("failed to load {size}x{size} icon resource: {err}"))?;

        Ok(HICON(handle.0))
    }

    fn set_window_icon(hwnd: HWND, icon_type: u32, icon: HICON) {
        unsafe {
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(icon_type as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{choose_icon_size, ICON_SIZES};

        #[test]
        fn icon_sizes_are_strictly_ascending() {
            // choose_icon_size's `find` returns the first entry >= desired,
            // which is only the *smallest sufficient* size if the table is
            // sorted; this pins that load-bearing ordering invariant.
            assert!(ICON_SIZES.windows(2).all(|pair| pair[0] < pair[1]));
        }

        #[test]
        fn standard_dpi_maps_to_exact_base_sizes() {
            assert_eq!(choose_icon_size(16, 96), 16);
            assert_eq!(choose_icon_size(24, 96), 24);
        }

        #[test]
        fn scaled_dpi_rounds_up_to_next_available_size() {
            // 150% scaling: 16 -> 24 (exact), 24 -> 36 -> next size up is 48.
            assert_eq!(choose_icon_size(16, 144), 24);
            assert_eq!(choose_icon_size(24, 144), 48);
            // 125% scaling: 16 -> 20 (exact table entry via div_ceil).
            assert_eq!(choose_icon_size(16, 120), 20);
        }

        #[test]
        fn oversized_demand_clamps_to_largest_icon() {
            assert_eq!(choose_icon_size(256, 480), 256);
            // Saturating multiply keeps absurd DPI values from overflowing.
            assert_eq!(choose_icon_size(u32::MAX, u32::MAX), 256);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::domain_topic_to_tauri_event;

    #[test]
    fn domain_topics_namespace_dots_to_colons_for_the_tauri_bus() {
        assert_eq!(
            domain_topic_to_tauri_event("tracking.session.updated"),
            "tracking:session:updated"
        );
        assert_eq!(
            domain_topic_to_tauri_event("scan.status.changed"),
            "scan:status:changed"
        );
    }

    #[tokio::test]
    async fn the_domain_bridge_receives_typed_envelopes_with_their_wire_shape() {
        // The bridge's emit payload is the typed envelope itself; its serde
        // value equals the pinned wire JSON, so the webview's topic-aware
        // consumers see the exact shape the old EventSource data field
        // carried.
        use eo_wire::domain_events::{
            DomainEvent, ScanPhase, ScanStatusChanged, ScanStatusChangedPayload,
            ScanStatusChangedTag,
        };
        let bus = eo_wire::bus::DomainBus::new(8);
        let mut receiver = bus.subscribe();
        let event = DomainEvent::ScanStatusChanged(ScanStatusChanged {
            topic: ScanStatusChangedTag,
            event_version: 1,
            occurred_at: "2024-12-31T21:20:00+00:00".into(),
            payload: ScanStatusChangedPayload {
                phase: ScanPhase::Capturing,
            },
        });
        bus.publish(event.clone());
        let received = receiver.recv().await.expect("the envelope arrives");
        assert_eq!(received, event);
        assert_eq!(
            domain_topic_to_tauri_event(received.topic()),
            "scan:status:changed"
        );
        assert_eq!(
            serde_json::to_value(&received).expect("envelopes serialise"),
            serde_json::from_str::<serde_json::Value>(&received.to_wire_json())
                .expect("the wire JSON parses"),
        );
    }

    /// The frontend reaches the backend only through the in-process IPC command,
    /// so the substrate binds no inbound listener (see `compose_substrate`) and
    /// the security policy grants no loopback origin: the CSP carries no
    /// `127.0.0.1`/`8421` in connect-src or img-src. The external news origin,
    /// the IPC scheme, and the base64 image `data:` source survive.
    #[test]
    fn the_security_policy_grants_no_loopback_origin() {
        let conf = include_str!("../tauri.conf.json");
        let after = conf
            .split("\"csp\":")
            .nth(1)
            .expect("the security CSP is configured");
        let csp = after
            .split('"')
            .nth(1)
            .expect("the CSP is a string literal");
        assert!(
            !csp.contains("127.0.0.1") && !csp.contains("8421"),
            "the CSP must grant no loopback origin once the frontend is IPC-only: {csp}"
        );
        assert!(
            csp.contains("https://entropiaorme.com"),
            "the external news origin must survive the loopback strip: {csp}"
        );
        assert!(csp.contains("ipc:"), "the IPC scheme must remain: {csp}");
        assert!(
            csp.contains("img-src 'self' data:"),
            "img-src keeps data: for the base64 capture preview: {csp}"
        );
    }

    /// The bundle ships no Python sidecar. The packaging spec declares
    /// no `externalBin`, and the shell `execute`/sidecar capability is gone
    /// (only `open` survives, for external links), so the installed artefact
    /// carries the single native binary alone.
    #[test]
    fn the_bundle_declares_no_sidecar_binary() {
        let conf = include_str!("../tauri.conf.json");
        assert!(
            !conf.contains("externalBin"),
            "the packaging spec must declare no externalBin once the sidecar is decommissioned"
        );
        let capabilities = include_str!("../capabilities/default.json");
        assert!(
            !capabilities.contains("shell:allow-execute"),
            "the shell execute/sidecar capability must be gone: {capabilities}"
        );
        assert!(
            capabilities.contains("shell:allow-open"),
            "the shell open capability must survive for external links: {capabilities}"
        );
    }
}
