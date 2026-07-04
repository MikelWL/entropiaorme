//! Behavioural pins for the settings family over the typed facade,
//! ported from the family's HTTP-era hermetic handler tests: the
//! assembled settings read (defaults, the live db path, the version
//! stamp, the trifecta readiness, the hotbar slot order), the
//! overlay-position read/write, and the partial-update validation ladder
//! (the empty-patch refusal, the chat-log path checks, the mob-mode
//! gate), plus a transport-invariance pin (the typed overlay-position
//! response serialises to the exact bytes the HTTP route answered).

use std::path::Path;
use std::sync::Arc;

use eo_api::settings::SettingsPatch;
use eo_api::{Api, ApiError};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use serde_json::Value;

mod common;

/// The composed facade over a fresh migrated database, an empty catalogue
/// snapshot (so a default preset validates not-ready, catalogue-free), and
/// the live config writer.
async fn settings_api(dir: &Path) -> Api {
    let snapshot = dir.join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let (config_service, tracker, hotbar, watcher) =
        common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current());
    Api::new(
        db,
        game_data,
        clock,
        data_dir,
        config_service,
        tracker,
        hotbar,
        watcher,
    )
}

/// The persisted `settings.json` as JSON, for storage assertions.
fn read_settings(data_dir: &Path) -> Value {
    let raw = std::fs::read_to_string(data_dir.join("settings.json")).expect("settings.json reads");
    serde_json::from_str(&raw).expect("settings.json parses")
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settings_assembly_shapes_the_default_config() {
    let dir = tempfile::tempdir().unwrap();
    let api = settings_api(dir.path()).await;

    let settings = api.settings().await.unwrap();
    let body = serde_json::to_value(&settings).unwrap();

    // The wire order the frontend contract carries.
    assert_eq!(
        keys(&body),
        [
            "gameConnection",
            "hotbarHooksEnabled",
            "repairOcrEnabled",
            "endOfSessionArmourReminderEnabled",
            "developerModeEnabled",
            "mobTrackingMode",
            "mobTrackingTag",
            "hotbar",
            "trifecta",
            "lootFilterBlacklist",
            "dbPath",
            "appVersion",
        ]
    );
    assert_eq!(
        keys(&body["gameConnection"]),
        ["chatLogPath", "chatLogValid", "playerName"]
    );
    assert_eq!(
        keys(&body["trifecta"]),
        ["activePresetId", "activePresetName", "presets", "ready", "message"]
    );

    // The default values (mirrors the HTTP-era assembly test).
    assert_eq!(body["mobTrackingMode"], "mob");
    assert_eq!(body["lootFilterBlacklist"], serde_json::json!(["Universal Ammo"]));
    assert_eq!(body["trifecta"]["activePresetId"], "default");
    assert_eq!(body["trifecta"]["presets"][0]["ready"], false);
    assert_eq!(
        body["trifecta"]["message"],
        "Trifecta attribution requires a configured small weapon, big weapon, and healing tool"
    );
    assert_eq!(body["appVersion"], env!("CARGO_PKG_VERSION"));
    assert!(body["dbPath"].as_str().unwrap().ends_with("entropia_orme.db"));
    // The hotbar carries through in stored slot order (1..9 then 0), not
    // sorted: the order the byte-faithful response depends on.
    assert_eq!(
        keys(&body["hotbar"]),
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_overlay_position_reads_and_writes() {
    let dir = tempfile::tempdir().unwrap();
    let api = settings_api(dir.path()).await;

    // Transport invariance: the typed default response serialises to the
    // exact bytes the HTTP route answered.
    let position = api.settings_overlay_position().await.unwrap();
    assert_eq!(
        serde_json::to_string(&position).unwrap(),
        "{\"x\":null,\"y\":null}"
    );

    // The write persists the exact coordinates and carries no producer
    // side effects.
    api.settings_set_overlay_position(7, 9).await.unwrap();
    let cfg = read_settings(&dir.path().join("data"));
    assert_eq!(cfg["overlay_x"], 7);
    assert_eq!(cfg["overlay_y"], 9);
    let position = api.settings_overlay_position().await.unwrap();
    assert_eq!(position.x, Some(7));
    assert_eq!(position.y, Some(9));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_update_writes_and_returns_the_assembly() {
    let dir = tempfile::tempdir().unwrap();
    let api = settings_api(dir.path()).await;

    let updated = api
        .settings_update(SettingsPatch {
            player_name: Some("Mikel".into()),
            hotbar_hooks_enabled: Some(true),
            mob_tracking_tag: Some("Daily Hunt".into()),
            ..SettingsPatch::default()
        })
        .await
        .unwrap();

    // The reply is the full assembly reflecting the write.
    assert_eq!(updated.game_connection.player_name, "Mikel");
    assert!(updated.hotbar_hooks_enabled);
    assert_eq!(updated.mob_tracking_tag, "Daily Hunt");
    // The write reached the store.
    let cfg = read_settings(&dir.path().join("data"));
    assert_eq!(cfg["player_name"], "Mikel");
    assert_eq!(cfg["hotbar_hooks_enabled"], true);
    assert_eq!(cfg["mob_tracking_tag"], "Daily Hunt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_update_validation_ladder_refuses_the_backend_way() {
    let dir = tempfile::tempdir().unwrap();
    let api = settings_api(dir.path()).await;

    // An empty patch is nothing to update.
    assert_eq!(
        api.settings_update(SettingsPatch::default()).await.unwrap_err(),
        ApiError::bad_request("No fields to update")
    );

    // The chat-log path chain: empty, wrong basename, non-existent.
    assert_eq!(
        api.settings_update(SettingsPatch {
            chatlog_path: Some(String::new()),
            ..SettingsPatch::default()
        })
        .await
        .unwrap_err(),
        ApiError::bad_request("chat.log path is required")
    );
    assert_eq!(
        api.settings_update(SettingsPatch {
            chatlog_path: Some("not_a_chatlog.txt".into()),
            ..SettingsPatch::default()
        })
        .await
        .unwrap_err(),
        ApiError::bad_request("chat.log path must point to a chat.log file")
    );
    assert_eq!(
        api.settings_update(SettingsPatch {
            chatlog_path: Some("/no/such/place/chat.log".into()),
            ..SettingsPatch::default()
        })
        .await
        .unwrap_err(),
        ApiError::bad_request("chat.log path does not exist")
    );

    // An unknown mob-tracking mode is refused before the write.
    assert_eq!(
        api.settings_update(SettingsPatch {
            mob_tracking_mode: Some("bogus".into()),
            ..SettingsPatch::default()
        })
        .await
        .unwrap_err(),
        ApiError::bad_request("Unknown mob tracking mode")
    );
    // The refusals left the stored player name untouched (default empty).
    assert_eq!(read_settings(&dir.path().join("data"))["player_name"], "");
}
