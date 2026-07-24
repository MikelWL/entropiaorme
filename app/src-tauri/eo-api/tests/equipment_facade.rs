//! Behavioural pins for the equipment family over the typed facade,
//! ported from the family's HTTP-era integration tests: the search
//! gates, the catalogue-less validation ladder, the custom-consumable
//! write cycle, the type-change and missing-row refusals, the
//! trifecta-reference delete guard, and the transport-invariance pins
//! (the typed response serialises to the exact bytes the HTTP route
//! answered, and the stored `properties_json` bytes are unchanged).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use eo_api::equipment::{EquipmentKind, EquipmentRequest, SearchKind};
use eo_api::{Api, ApiError};
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

mod common;

/// A minimal catalogue snapshot: one weapon (a limited one, so the
/// `(L)` flag pins), one stimulant.
fn write_snapshot(dir: &Path) {
    std::fs::write(
        dir.join("weapons.json"),
        r#"[{"id": "w1", "name": "Opalo Rifle (L)", "economy": {"decay": 0.5, "ammo_burn": 300}}]"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("stimulants.json"),
        r#"[{"id": "s1", "name": "Vita Bar", "economy": {}}]"#,
    )
    .unwrap();
}

/// The composed facade over a fresh migrated database and the test
/// snapshot, plus a database handle of its own for storage assertions.
async fn api_over(dir: &Path) -> (Api, Db) {
    let snapshot = dir.join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    write_snapshot(&snapshot);
    let data_dir: PathBuf = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("snapshot store"));
    let clock = Arc::new(eo_services::clock::RealClock::new());
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current()).await;
    (
        Api::new(
            db.clone(),
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
            None,
            None,
            None,
        ),
        db,
    )
}

fn consumable(name: &str) -> EquipmentRequest {
    EquipmentRequest {
        kind: EquipmentKind::Consumable,
        catalog_id: None,
        name: Some(name.to_string()),
        amp_catalog_id: None,
        scope_catalog_id: None,
        absorber_catalog_id: None,
        weapon_markup: 100,
        amp_markup: 100,
        scope_markup: 100,
        absorber_markup: 100,
        damage_enhancers: 0,
        implant_name: None,
        implant_share_percent: None,
        implant_markup: 100,
        extender_name: None,
        extender_absorption_percent: None,
        extender_markup: 100,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn search_gates_and_hits_match_the_route_behaviour() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = api_over(dir.path()).await;

    // Short queries answer empty before any lookup.
    let hits = api.equipment_search("o", SearchKind::Weapon).await.unwrap();
    assert!(hits.is_empty());

    // A hit carries the catalogue economy in the response shape.
    let hits = api
        .equipment_search("opalo", SearchKind::Weapon)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.catalog_id.as_deref(), Some("w1"));
    assert_eq!(hit.name, "Opalo Rifle (L)");
    assert_eq!(hit.decay, 0.5);
    assert_eq!(hit.ammo_burn, 3.0);
    assert!(hit.is_limited);

    // A miss in another vocabulary answers empty, not an error.
    let hits = api
        .equipment_search("opalo", SearchKind::Healer)
        .await
        .unwrap();
    assert!(hits.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_validation_ladder_matches_the_route_replies() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = api_over(dir.path()).await;

    // Weapon without a catalogue id.
    let mut req = consumable("x");
    req.kind = EquipmentKind::Weapon;
    req.name = None;
    assert_eq!(
        api.equipment_add(&req).await.unwrap_err(),
        ApiError::bad_request("catalog_id required for weapon"),
    );

    // A catalogue miss names the endpoint, exactly as before.
    req.catalog_id = Some("nope".to_string());
    assert_eq!(
        api.equipment_add(&req).await.unwrap_err(),
        ApiError::not_found("Entity 'nope' not found in catalogue endpoint 'weapons'."),
    );

    // A consumable needs an identity.
    let mut req = consumable("x");
    req.name = None;
    assert_eq!(
        api.equipment_add(&req).await.unwrap_err(),
        ApiError::bad_request(
            "Consumable requires either catalog_id (catalogue pick) or name (custom)"
        ),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_custom_consumable_cycle_matches_the_http_era_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (api, db) = api_over(dir.path()).await;

    // The library starts empty.
    assert!(api.equipment_library().await.unwrap().is_empty());

    // The stored name is stripped the reference way.
    let added = api
        .equipment_add(&consumable("  Nutrio Bar  "))
        .await
        .unwrap();
    assert_eq!(added.id, "1");
    assert_eq!(added.name, "Nutrio Bar");

    // Transport invariance: the typed summary serialises to the exact
    // body bytes the HTTP route answered for this row.
    assert_eq!(
        serde_json::to_string(&added).unwrap(),
        "{\"id\":\"1\",\"name\":\"Nutrio Bar\",\"type\":\"consumable\",\"amplifierName\":null,\
         \"costPerUse\":0.0,\"damageMin\":null,\"damageMax\":null,\"reloadSeconds\":null,\
         \"isLimited\":false,\"enrichmentLevel\":1}"
    );

    // Storage invariance: the stored props bytes are the reference
    // `json.dumps` form, unchanged by the transport migration.
    let stored: String = db
        .with_reader(|conn| {
            Ok(conn.query_row(
                "SELECT properties_json FROM equipment_library WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(stored, "{\"catalog_id\": null, \"entity\": null}");

    let listed = api.equipment_library().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "1");

    // The stored class is fixed.
    let mut as_weapon = consumable("x");
    as_weapon.kind = EquipmentKind::Weapon;
    as_weapon.catalog_id = Some("w1".to_string());
    assert_eq!(
        api.equipment_update(1, &as_weapon).await.unwrap_err(),
        ApiError::bad_request("Cannot change equipment type"),
    );

    // A missing row names itself.
    assert_eq!(
        api.equipment_update(9, &consumable("X")).await.unwrap_err(),
        ApiError::not_found("Equipment item 9 not found"),
    );

    // The detail mirrors the row into the weapon slot.
    let detail = api.equipment_detail(1).await.unwrap();
    assert_eq!(detail.weapon.name, "Nutrio Bar");
    assert_eq!(detail.total_cost_per_use, 0.0);
    assert!(detail.cost_breakdown.is_empty());
    assert_eq!(
        api.equipment_detail(9).await.unwrap_err(),
        ApiError::not_found("Equipment item 9 not found"),
    );

    // Deletes are idempotent acknowledgements, present row or not.
    api.equipment_delete(1).await.unwrap();
    api.equipment_delete(9).await.unwrap();
    assert!(api.equipment_library().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_weapon_setup_stores_and_lists_with_its_catalogue_economy() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = api_over(dir.path()).await;

    let mut req = consumable("");
    req.kind = EquipmentKind::Weapon;
    req.name = None;
    req.catalog_id = Some("w1".to_string());
    req.weapon_markup = 110;
    let added = api.equipment_add(&req).await.unwrap();
    assert_eq!(added.name, "Opalo Rifle (L)");
    assert!(added.is_limited);
    assert_eq!(added.enrichment_level, 1);
    assert_eq!(added.kind, EquipmentKind::Weapon);

    let detail = api.equipment_detail(1).await.unwrap();
    assert_eq!(detail.weapon.catalog_id.as_deref(), Some("w1"));
    assert_eq!(detail.weapon.markup_percent, 110.0);
    assert_eq!(detail.weapon.decay, 0.5);
    assert_eq!(detail.weapon.ammo_burn, 3.0);
    assert!(detail.amplifier.is_none());
    assert!(detail.scope.is_none());
    assert!(detail.absorber.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_split_devices_round_trip_and_reprice_the_weapon() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = api_over(dir.path()).await;

    let mut req = consumable("");
    req.kind = EquipmentKind::Weapon;
    req.name = None;
    req.catalog_id = Some("w1".to_string());
    req.weapon_markup = 1500;
    req.implant_name = Some("NeoPsion 85-B Mindforce Implant (L)".to_string());
    req.implant_share_percent = Some(20.0);
    req.implant_markup = 110;
    req.extender_absorption_percent = Some(20.0);
    req.extender_markup = 108;
    let added = api.equipment_add(&req).await.unwrap();
    // Implant 20% of 0.5 decay @ 1.10 = 0.11; extender 20% of the 0.4
    // remainder @ 1.08 = 0.0864; weapon keeps 0.32 @ 15.0 = 4.8; ammo 3.0.
    assert_eq!(added.cost_per_use, 7.9964);

    let detail = api.equipment_detail(1).await.unwrap();
    let implant = detail.implant.as_ref().unwrap();
    assert_eq!(
        implant.name.as_deref(),
        Some("NeoPsion 85-B Mindforce Implant (L)")
    );
    assert_eq!(implant.share_percent, 20.0);
    assert_eq!(implant.markup_percent, 110.0);
    let extender = detail.extender.as_ref().unwrap();
    assert!(extender.name.is_none());
    assert_eq!(extender.share_percent, 20.0);
    assert_eq!(extender.markup_percent, 108.0);
    let components: Vec<&str> = detail
        .cost_breakdown
        .iter()
        .map(|line| line.component.as_str())
        .collect();
    assert_eq!(
        components,
        ["Implant decay", "Extender decay", "Weapon decay", "Ammo"]
    );
    assert_eq!(detail.total_cost_per_use, 7.9964);

    // Clearing the shares on update removes the devices entirely.
    let mut cleared = req.clone();
    cleared.implant_share_percent = None;
    cleared.extender_absorption_percent = None;
    let updated = api.equipment_update(1, &cleared).await.unwrap();
    assert_eq!(updated.cost_per_use, 10.5);
    let detail = api.equipment_detail(1).await.unwrap();
    assert!(detail.implant.is_none());
    assert!(detail.extender.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trifecta_referenced_row_refuses_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = api_over(dir.path()).await;
    api.equipment_add(&consumable("Kept")).await.unwrap();

    // A preset referencing row 1 blocks its removal.
    std::fs::write(
        dir.path().join("data").join("settings.json"),
        r#"{"trifecta_presets": [{"id": "p1", "name": "P", "small_weapon_id": 1}]}"#,
    )
    .unwrap();
    assert_eq!(
        api.equipment_delete(1).await.unwrap_err(),
        ApiError::conflict("Cannot remove equipment selected in a trifecta preset"),
    );
    assert_eq!(api.equipment_library().await.unwrap().len(), 1);
}
