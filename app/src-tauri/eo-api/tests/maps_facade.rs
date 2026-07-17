//! Behavioural pins for the maps family over the typed facade: the
//! bundled catalogue read (with and without the bundle composed), the
//! pin CRUD roundtrip with its DTO wire shape, the map-bounds gate on
//! create and move, the patch's double-option null semantics, and the
//! not-found / bad-request legs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eo_api::maps::{MapPinInput, MapPinPatch};
use eo_api::{Api, ApiError};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use eo_services::planet_maps::PlanetMapStore;

mod common;

/// The repo's bundled maps directory (the dev-layout resolution).
fn bundled_maps_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("entropia-orme")
        .join("resources")
        .join("maps")
}

/// The composed facade over a fresh migrated database and an empty
/// catalogue snapshot, with or without the planet-map bundle.
async fn maps_api(dir: &Path, with_bundle: bool) -> Api {
    let snapshot = dir.join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let planet_maps = with_bundle
        .then(|| Arc::new(PlanetMapStore::new(&bundled_maps_dir()).expect("bundled maps load")));
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current()).await;
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
        handles.quests.clone(),
        None,
        planet_maps,
    )
}

fn calypso_pin() -> MapPinInput {
    MapPinInput {
        planet: "Calypso".to_string(),
        lon: 61400.0,
        lat: 75800.0,
        altitude: Some(103.0),
        name: "Port Atlantis TP".to_string(),
        icon: "teleporter".to_string(),
        kind: "travel".to_string(),
        radius_m: None,
        notes: Some("the sanity anchor".to_string()),
        session_id: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_catalogue_serialises_with_its_bundle_shape() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), true).await;

    let maps = api.planet_maps().unwrap();
    assert_eq!(maps.len(), 20);

    let calypso = serde_json::to_value(maps.iter().find(|m| m.name == "Calypso").unwrap()).unwrap();
    assert_eq!(calypso["imageMime"], "image/jpeg");
    assert_eq!(calypso["imageWidthPx"], 4608);
    assert_eq!(calypso["calibration"]["unitsPerPixelX"], 16.0);
    assert_eq!(calypso["calibration"]["bounds"]["lonMin"], 16384);

    // The view-only map: present, explicitly null-calibrated.
    let thule = serde_json::to_value(maps.iter().find(|m| m.name == "Thule").unwrap()).unwrap();
    assert_eq!(thule["calibration"], serde_json::Value::Null);
    assert_eq!(thule["technicalName"], serde_json::Value::Null);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_facade_without_the_bundle_serves_an_empty_catalogue() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), false).await;
    assert!(api.planet_maps().unwrap().is_empty());
    assert!(api.planet_map_image("Calypso").is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pin_roundtrips_with_its_wire_shape() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), true).await;

    let created = api.map_pin_create(calypso_pin()).await.unwrap();
    let listed = api.map_pins_list("Calypso".to_string()).await.unwrap();
    assert_eq!(listed.len(), 1);

    let wire = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(wire["id"], created.id);
    assert_eq!(wire["planet"], "Calypso");
    assert_eq!(wire["lon"], 61400.0);
    assert_eq!(wire["radiusM"], serde_json::Value::Null);
    assert_eq!(wire["notes"], "the sanity anchor");
    assert_eq!(wire["sessionId"], serde_json::Value::Null);
    assert!(wire["createdAt"].is_f64());

    // Another planet's list stays empty (planet-scoped reads).
    assert!(api
        .map_pins_list("Arkadia".to_string())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn implausible_coordinates_are_refused_on_create_and_move() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), true).await;

    let mut outside = calypso_pin();
    outside.lat = 999_999.0;
    assert!(matches!(
        api.map_pin_create(outside).await,
        Err(ApiError::BadRequest { .. })
    ));

    let created = api.map_pin_create(calypso_pin()).await.unwrap();
    let move_outside = MapPinPatch {
        lon: Some(1.0),
        ..MapPinPatch::default()
    };
    assert!(matches!(
        api.map_pin_update(created.id, move_outside).await,
        Err(ApiError::BadRequest { .. })
    ));

    // Without the bundle there is nothing authoritative to gate against:
    // the same out-of-bounds pin is accepted rather than invented-refused.
    let bare = maps_api(&dir.path().join("bare"), false).await;
    let mut outside = calypso_pin();
    outside.lat = 999_999.0;
    assert!(bare.map_pin_create(outside).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_patch_distinguishes_absent_from_explicit_null() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), true).await;
    let created = api.map_pin_create(calypso_pin()).await.unwrap();

    // The wire shape: notes explicitly nulled, name changed, the rest
    // absent. Deserialised through serde to exercise the double-option
    // semantics the typed transport carries.
    let patch: MapPinPatch =
        serde_json::from_value(serde_json::json!({"name": "PA", "notes": null})).unwrap();
    let updated = api.map_pin_update(created.id, patch).await.unwrap();
    assert_eq!(updated.name, "PA");
    assert_eq!(*updated.notes, None);
    // Untouched fields survived.
    assert_eq!(*updated.altitude, Some(103.0));
    assert_eq!(updated.lon, 61400.0);

    // An empty patch is a no-op update, not an error.
    let unchanged = api
        .map_pin_update(created.id, MapPinPatch::default())
        .await
        .unwrap();
    assert_eq!(unchanged.name, "PA");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validation_and_not_found_legs_answer_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let api = maps_api(dir.path(), true).await;

    let mut nameless = calypso_pin();
    nameless.name = "   ".to_string();
    assert!(matches!(
        api.map_pin_create(nameless).await,
        Err(ApiError::BadRequest { .. })
    ));

    let mut degenerate = calypso_pin();
    degenerate.radius_m = Some(0.0);
    assert!(matches!(
        api.map_pin_create(degenerate).await,
        Err(ApiError::BadRequest { .. })
    ));

    assert!(matches!(
        api.map_pin_update(9999, MapPinPatch::default()).await,
        Err(ApiError::NotFound { .. })
    ));
    assert!(matches!(
        api.map_pin_delete(9999).await,
        Err(ApiError::NotFound { .. })
    ));

    let created = api.map_pin_create(calypso_pin()).await.unwrap();
    api.map_pin_delete(created.id).await.unwrap();
    assert!(api
        .map_pins_list("Calypso".to_string())
        .await
        .unwrap()
        .is_empty());
}
