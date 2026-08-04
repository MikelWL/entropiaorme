//! Behavioural pins for the session-definitions family over the typed
//! facade: the empty-database read, the create / read-back / update
//! (roster replaced wholesale) / delete ladder with its not-found and
//! validation legs, the transport-invariance byte pin, and the
//! tracking-family selection verb (config writeback, snapshot
//! exposure, the free-text-rename disavow, and the fixed-while-active
//! conflict).

use std::path::Path;
use std::sync::Arc;

use eo_api::quests::QuestFamilyInput;
use eo_api::session_definitions::{
    SessionDefinitionInput, SessionRosterEntryInput, SessionRosterEntryKind,
};
use eo_api::{Api, ApiError};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

mod common;

/// The composed facade over a fresh migrated database and an empty
/// catalogue snapshot, with the database handle kept for timestamp
/// pinning (`created_at` stamps from the wall clock).
async fn definitions_api(dir: &Path) -> (Api, Db) {
    definitions_api_with_settings(dir, None).await
}

/// [`definitions_api`] with a seeded `settings.json` (the conflict pin
/// starts a real session, which needs a bound hotbar slot so the start
/// satisfies that gate rather than the trifecta loadout one).
async fn definitions_api_with_settings(dir: &Path, settings: Option<&str>) -> (Api, Db) {
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
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current()).await;
    let api = Api::new(
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
    );
    (api, db)
}

fn definition(name: &str, roster: Vec<SessionRosterEntryInput>) -> SessionDefinitionInput {
    SessionDefinitionInput {
        name: name.to_string(),
        ad_hoc_segments: false,
        roster,
    }
}

fn segment(label: &str) -> SessionRosterEntryInput {
    SessionRosterEntryInput {
        kind: SessionRosterEntryKind::Segment,
        ref_id: None,
        label: Some(label.to_string()),
    }
}

async fn pin_timestamps(db: &Db, definition_id: i64) {
    db.with_writer(move |conn| {
        conn.execute(
            "UPDATE session_definitions SET created_at = 1000.0, updated_at = NULL \
             WHERE id = ?",
            rusqlite::params![definition_id],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_list_answers_the_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = definitions_api(dir.path()).await;
    assert!(api.session_definitions_list().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_create_reads_back_the_wire_shape() {
    let dir = tempfile::tempdir().unwrap();
    let (api, db) = definitions_api(dir.path()).await;

    // A roster spanning all three kinds: a family (created over the
    // facade), a quest (seeded directly; the quest DTO ladder is the
    // quests family's own pin), and a plain segment label.
    api.quest_family_create(QuestFamilyInput {
        name: "Daily Hunting 1".to_string(),
        planet: "ARIS".to_string(),
        cooldown_hours: Some(20.0),
        cooldown_anchor: None,
    })
    .await
    .unwrap();
    db.with_writer(|conn| {
        conn.execute(
            "INSERT INTO quests (name) VALUES ('The Ultimate Threat')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let created = api
        .session_definition_create(SessionDefinitionInput {
            name: "ARIS Dailies".to_string(),
            ad_hoc_segments: true,
            roster: vec![
                SessionRosterEntryInput {
                    kind: SessionRosterEntryKind::QuestFamily,
                    ref_id: Some(1),
                    label: None,
                },
                SessionRosterEntryInput {
                    kind: SessionRosterEntryKind::Quest,
                    ref_id: Some(1),
                    label: None,
                },
                segment("Warm-up"),
            ],
        })
        .await
        .unwrap();
    assert_eq!(created.id, "1");

    // Transport invariance: the exact wire bytes, with the wall-clock
    // stamp pinned first.
    pin_timestamps(&db, 1).await;
    let listed = api.session_definitions_list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        serde_json::to_string(&listed[0]).unwrap(),
        "{\"id\":\"1\",\"name\":\"ARIS Dailies\",\"adHocSegments\":true,\
         \"instanceCount\":0,\"createdAt\":1000.0,\"updatedAt\":null,\"roster\":[\
         {\"id\":\"1\",\"kind\":\"quest_family\",\"refId\":\"1\",\"label\":null,\
         \"displayName\":\"Daily Hunting 1\"},\
         {\"id\":\"2\",\"kind\":\"quest\",\"refId\":\"1\",\"label\":null,\
         \"displayName\":\"The Ultimate Threat\"},\
         {\"id\":\"3\",\"kind\":\"segment\",\"refId\":null,\"label\":\"Warm-up\",\
         \"displayName\":\"Warm-up\"}]}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_update_and_delete_ladder_holds() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = definitions_api(dir.path()).await;

    let created = api
        .session_definition_create(definition("General Hunting", vec![segment("Travel")]))
        .await
        .unwrap();

    // Update replaces the roster wholesale and flips the flag.
    let updated = api
        .session_definition_update(
            1,
            SessionDefinitionInput {
                name: "General Hunting".to_string(),
                ad_hoc_segments: true,
                roster: vec![segment("Grind"), segment("Wind-down")],
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.id, created.id);
    assert!(updated.ad_hoc_segments);
    let labels: Vec<_> = updated
        .roster
        .iter()
        .map(|entry| entry.label.as_deref().unwrap().to_string())
        .collect();
    assert_eq!(labels, vec!["Grind", "Wind-down"]);

    // The not-found legs: an unknown id on update and delete, and both
    // again after the soft delete (a deleted definition reads as absent).
    assert_eq!(
        api.session_definition_update(99, definition("X", vec![]))
            .await
            .unwrap_err(),
        ApiError::not_found("Session definition not found")
    );
    assert_eq!(
        api.session_definition_delete(99).await.unwrap_err(),
        ApiError::not_found("Session definition not found")
    );
    api.session_definition_delete(1).await.unwrap();
    assert!(api.session_definitions_list().await.unwrap().is_empty());
    assert_eq!(
        api.session_definition_delete(1).await.unwrap_err(),
        ApiError::not_found("Session definition not found")
    );
    assert_eq!(
        api.session_definition_update(1, definition("X", vec![]))
            .await
            .unwrap_err(),
        ApiError::not_found("Session definition not found")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validation_rejections_are_bad_requests() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = definitions_api(dir.path()).await;

    assert!(matches!(
        api.session_definition_create(definition("   ", vec![]))
            .await
            .unwrap_err(),
        ApiError::BadRequest { .. }
    ));
    assert!(matches!(
        api.session_definition_create(definition("A", vec![segment("  ")]))
            .await
            .unwrap_err(),
        ApiError::BadRequest { .. }
    ));
    assert!(matches!(
        api.session_definition_create(definition(
            "A",
            vec![SessionRosterEntryInput {
                kind: SessionRosterEntryKind::QuestFamily,
                ref_id: Some(99),
                label: None,
            }],
        ))
        .await
        .unwrap_err(),
        ApiError::BadRequest { .. }
    ));

    // Duplicate active names split one family's instances invisibly.
    api.session_definition_create(definition("ARIS Dailies", vec![]))
        .await
        .unwrap();
    assert!(matches!(
        api.session_definition_create(definition("aris dailies", vec![]))
            .await
            .unwrap_err(),
        ApiError::BadRequest { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selection_writes_the_config_and_the_snapshot_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = definitions_api(dir.path()).await;

    api.session_definition_create(definition("ARIS Dailies", vec![]))
        .await
        .unwrap();

    // An unknown id is a 404 and writes nothing.
    assert_eq!(
        api.tracking_definition_select(Some(99)).await.unwrap_err(),
        ApiError::not_found("Session definition not found")
    );

    // Selection acknowledges and writes both config facets; the idle
    // snapshot reports them.
    let selected = api.tracking_definition_select(Some(1)).await.unwrap();
    assert_eq!(selected.session_definition_id, Some("1".to_string()));
    assert_eq!(selected.session_name, Some("ARIS Dailies".to_string()));
    let snapshot = api.tracking_snapshot().await.unwrap();
    assert_eq!(snapshot.session_definition_id, Some("1".to_string()));
    assert_eq!(snapshot.session_name, Some("ARIS Dailies".to_string()));

    // A boost-only config write (name unchanged) keeps the selection; a
    // free-text rename disavows it.
    api.tracking_session_config(Some("ARIS Dailies".into()), Some(50))
        .await
        .unwrap();
    let kept = api.tracking_snapshot().await.unwrap();
    assert_eq!(kept.session_definition_id, Some("1".to_string()));
    api.tracking_session_config(Some("Something Else".into()), None)
        .await
        .unwrap();
    let disavowed = api.tracking_snapshot().await.unwrap();
    assert_eq!(disavowed.session_definition_id, None);
    assert_eq!(disavowed.session_name, Some("Something Else".to_string()));

    // Withdrawing the selection clears both facets.
    api.tracking_definition_select(Some(1)).await.unwrap();
    api.tracking_definition_select(None).await.unwrap();
    let cleared = api.tracking_snapshot().await.unwrap();
    assert_eq!(cleared.session_definition_id, None);
    assert_eq!(cleared.session_name, None);

    // A selection whose definition is later deleted stops reading as
    // selected (the idle snapshot re-validates).
    api.tracking_definition_select(Some(1)).await.unwrap();
    api.session_definition_delete(1).await.unwrap();
    let stale = api.tracking_snapshot().await.unwrap();
    assert_eq!(stale.session_definition_id, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_selection_is_fixed_while_a_session_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _db) = definitions_api_with_settings(
        dir.path(),
        Some("{\"hotbar_hooks_enabled\": true, \"hotbar\": {\"1\": 1}}"),
    )
    .await;

    api.session_definition_create(definition("ARIS Dailies", vec![]))
        .await
        .unwrap();
    api.tracking_start().await.unwrap();

    // Changing the selection mid-session conflicts; re-affirming the
    // standing value (none) is a tolerated no-op.
    assert!(matches!(
        api.tracking_definition_select(Some(1)).await.unwrap_err(),
        ApiError::Conflict { .. }
    ));
    api.tracking_definition_select(None).await.unwrap();

    api.tracking_stop().await.unwrap();
    api.tracking_definition_select(Some(1)).await.unwrap();
}
