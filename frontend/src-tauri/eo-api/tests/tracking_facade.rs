//! Behavioural pins for the tracking family over the typed facade, ported
//! from the family's HTTP-era route behaviour: the session reads (list,
//! detail, tag suggestions), the idle dashboard snapshot, the post-hoc
//! session edits (rename mob, armour cost), the lifecycle guards (start /
//! stop), and the repair-scan gate. Each shape carries a transport-
//! invariance pin: the typed response serialises to the exact bytes the
//! HTTP route answered, save for the one ratified movement documented in
//! `eo_api::tracking` (the snapshot's exclude-unset -> exclude-none
//! narrowing: a present-null field is dropped rather than serialised null).

use std::path::Path;
use std::sync::Arc;

use eo_api::Api;
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

mod common;

/// The composed facade over a fresh migrated database. `settings` writes a
/// `settings.json` into the data dir first (for the config-flag reads);
/// `seed` places one ended session with two `Atrox` kills.
async fn make_api(dir: &Path, seed: bool, settings: Option<&str>) -> Api {
    let snapshot = dir.join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    if let Some(settings) = settings {
        std::fs::write(data_dir.join("settings.json"), settings).unwrap();
    }
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");
    if seed {
        seed_ended(&db).await;
    }
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current());
    Api::new(
        db,
        game_data,
        clock,
        data_dir,
        handles.config_service,
        handles.tracker,
        handles.hotbar,
        handles.watcher,
        handles.skill_tracker,
        handles.skill_scan,
        handles.spacebar,
        handles.repair_ocr,
        None,
    )
}

/// One ended session (`ended`) with two `Atrox` kills carrying loot.
async fn seed_ended(db: &Db) {
    sqlx::query(
        "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,heal_cost,\
         dangling_cost,mob_tracking_mode,updated_at) \
         VALUES('ended',1000.0,4600.0,0,0,0,0,'mob',4600.0)",
    )
    .execute(db.write())
    .await
    .unwrap();
    for (id, loot) in [("k1", 10.0), ("k2", 20.0)] {
        sqlx::query(
            "INSERT INTO kills(id,session_id,mob_name,timestamp,loot_total_ped) VALUES(?,?,?,?,?)",
        )
        .bind(id)
        .bind("ended")
        .bind("Atrox")
        .bind(1001.0)
        .bind(loot)
        .execute(db.write())
        .await
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_empty_session_list_serialises_to_the_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let sessions = api.tracking_sessions().await.unwrap();
    assert_eq!(serde_json::to_string(&sessions).unwrap(), "[]");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_idle_snapshot_serialises_the_dashboard_way() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    // The resting idle dashboard over a default config: the exclude-none
    // projection drops the present-null mob / tool fields (the ratified
    // movement), while the trifecta summary (a fixed-shape nested object,
    // present because the default config ships a `default` preset) keeps
    // its own null bindings on the wire.
    let snapshot = api.tracking_snapshot().await.unwrap();
    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        "{\"status\":\"idle\",\"hotbarListenerActive\":false,\"weaponAttribution\":\"trifecta\",\
         \"repairOcrEnabled\":false,\"endOfSessionArmourReminderEnabled\":false,\
         \"mobEntryMode\":\"mob\",\"trifectaAttribution\":{\"activePresetId\":\"default\",\
         \"presetName\":\"Default\",\"presets\":[{\"id\":\"default\",\"name\":\"Default\"}],\
         \"smallWeapon\":null,\"bigWeapon\":null,\"healTool\":null},\"recentEvents\":[]}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_tag_suggestions_serialise_to_the_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let suggestions = api
        .tracking_tag_suggestions(String::new(), None)
        .await
        .unwrap();
    assert_eq!(serde_json::to_string(&suggestions).unwrap(), "[]");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_session_detail_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let error = api
        .tracking_session_detail("nope".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&error).unwrap()["kind"], "notFound");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_seeded_session_detail_shapes_the_summary() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), true, None).await;
    let detail = api
        .tracking_session_detail("ended".to_string())
        .await
        .unwrap();
    // Two Atrox kills, 30 PED loot; the summary carries the float-typed
    // returns and the integer kill count.
    assert_eq!(detail.session_id, "ended");
    assert_eq!(detail.summary.kills, 2);
    assert_eq!(detail.summary.returns, 30.0);
    assert_eq!(detail.mob_breakdown.len(), 1);
    assert_eq!(detail.mob_breakdown[0].current_name, "Atrox");
    // The nullable per-mob original serialises present-null (a fixed-shape
    // response, unlike the polymorphic snapshot).
    let wire = serde_json::to_string(&detail).unwrap();
    assert!(
        wire.contains("\"currentName\":\"Atrox\",\"originalName\":null,\"killCount\":2"),
        "{wire}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repair_scan_is_bad_request_when_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let error = api.tracking_repair_scan("ended".to_string()).unwrap_err();
    assert_eq!(
        serde_json::to_value(&error).unwrap(),
        serde_json::json!({"kind": "badRequest", "message": "Repair OCR is disabled"})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repair_scan_soft_error_rides_the_body() {
    let dir = tempfile::tempdir().unwrap();
    // Enabled, but the inert providers find no game window: the service's
    // logical refusal rides the 200 body (declared fields first, then the
    // extra `error` key), byte-for-byte as the HTTP plain-200.
    let api = make_api(dir.path(), false, Some("{\"repair_ocr_enabled\": true}")).await;
    let result = api.tracking_repair_scan("ended".to_string()).unwrap();
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        "{\"cost_ped\":0.0,\"raw_text\":\"\",\"confidence\":0.0,\
         \"error\":\"Entropia Universe window not found: start the game first\"}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_mob_happy_path_and_validation_legs() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), true, None).await;

    // Happy path: both Atrox kills rename to Argo, byte-for-byte.
    let result = api
        .tracking_rename_mob("ended".to_string(), "Atrox".to_string(), "Argo".to_string())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        "{\"sessionId\":\"ended\",\"mobName\":\"Argo\",\"killCount\":2}"
    );

    // A blank name is a bad-request.
    let blank = api
        .tracking_rename_mob("ended".to_string(), "  ".to_string(), "X".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&blank).unwrap()["kind"], "badRequest");

    // An absent session is a not-found.
    let missing = api
        .tracking_rename_mob("nope".to_string(), "Argo".to_string(), "Zed".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&missing).unwrap()["kind"], "notFound");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn armour_cost_echoes_the_submitted_value() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), true, None).await;
    let result = api
        .tracking_armour_cost("ended".to_string(), 2.5)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        "{\"sessionId\":\"ended\",\"armourCost\":2.5}"
    );
    // An absent session is a not-found (no active-session guard on this leg).
    let missing = api
        .tracking_armour_cost("nope".to_string(), 1.0)
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&missing).unwrap()["kind"], "notFound");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lifecycle_guards_hold() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;

    // Stop with no active session -> conflict.
    let stop = api.tracking_stop().await.unwrap_err();
    assert_eq!(serde_json::to_value(&stop).unwrap()["kind"], "conflict");

    // Start under the default config (trifecta mode, the default preset
    // present but its weapon / heal slots unbound) fails the attribution
    // gate with `validate_trifecta`'s verbatim reason.
    let start = api.tracking_start().await.unwrap_err();
    assert_eq!(
        serde_json::to_value(&start).unwrap(),
        serde_json::json!({
            "kind": "badRequest",
            "message": "Trifecta attribution requires a configured small weapon, big weapon, and healing tool"
        })
    );
}
