//! Behavioural pins for the quests + playlists family over the typed
//! facade, ported from the family's HTTP-era hermetic router tests: the
//! reads over an empty database, the quest create / read-back / update
//! (present-null clears) ladder, the lifecycle (start / complete /
//! cancel), the playlist create-with-items / update / delete, and the
//! not-found legs. Plus transport-invariance pins: the created quest and
//! playlist serialise to the exact bytes the HTTP routes answered.
//!
//! The framework-validation legs (the create/update field-type 422s, the
//! string→type lax coercions, the surrogate-taint / beyond-`i64` deferred
//! 500s) do not port: they are unrepresentable over the typed DTOs and
//! retire as ratified contract movements.

use std::path::Path;
use std::sync::Arc;

use eo_api::quests::{PlaylistInput, PlaylistItemInput, QuestInput};
use eo_api::{Api, ApiError};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

mod common;

/// The composed facade over a fresh migrated database and an empty
/// catalogue snapshot, matching the HTTP-era quests router assertions
/// (quest/playlist CRUD is catalogue-independent).
async fn quests_api(dir: &Path) -> Api {
    let snapshot = dir.join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");
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
    )
}

/// A minimal quest payload (name only; every other field defaults).
fn minimal(name: &str) -> QuestInput {
    QuestInput {
        name: name.to_string(),
        planet: "Calypso".to_string(),
        category: None,
        waypoint: None,
        cooldown_hours: None,
        reward_ped: None,
        reward_is_skill: false,
        expected_reward_markup_percent: None,
        reward_description: None,
        notes: None,
        chain_name: None,
        chain_position: None,
        chain_total: None,
        mobs: Vec::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reads_answer_the_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    assert!(api.quests_list().await.unwrap().is_empty());
    assert!(api.playlists_list().await.unwrap().is_empty());
    assert!(api.quests_mobs().await.unwrap().is_empty());
    assert!(api.quests_analytics().await.unwrap().is_empty());
    assert!(api.playlists_analytics().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_minimal_create_reads_back_the_wire_shape() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    // Transport invariance: the created quest serialises to the exact
    // bytes the HTTP route answered (id "1" over the fresh database, the
    // planet/reward_is_skill defaults, null-or-empty text columns).
    let created = api.quest_create(minimal("Alpha")).await.unwrap();
    assert_eq!(
        serde_json::to_string(&created).unwrap(),
        "{\"id\":\"1\",\"name\":\"Alpha\",\"category\":null,\"targetMobs\":[],\
         \"planet\":\"Calypso\",\"waypoint\":null,\"cooldownDurationHours\":null,\
         \"cooldownExpiresAt\":null,\"reward\":null,\"rewardIsSkill\":false,\
         \"expectedRewardMarkupPercent\":null,\"rewardDescription\":\"\",\"notes\":\"\",\
         \"chainName\":null,\"chainPosition\":null,\"chainTotal\":null,\
         \"playlistIds\":[],\"startedAt\":null}"
    );

    // The read-back through the listing and the by-id read agree.
    assert_eq!(api.quests_list().await.unwrap().len(), 1);
    let one = api.quest_get(1).await.unwrap();
    assert_eq!(one.id, "1");
    assert_eq!(one.name, "Alpha");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_populated_create_carries_its_typed_fields() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    let mut input = minimal("Beta");
    input.reward_ped = Some(10.5);
    input.reward_is_skill = true;
    input.chain_position = Some(2);
    input.mobs = vec!["Atrox".to_string()];
    let created = api.quest_create(input).await.unwrap();

    assert_eq!(created.reward, Some(10.5));
    assert!(created.reward_is_skill);
    assert_eq!(created.chain_position, Some(2));
    assert_eq!(created.target_mobs, vec!["Atrox".to_string()]);

    // The distinct mob names surface for autocomplete.
    assert_eq!(api.quests_mobs().await.unwrap(), vec!["Atrox".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_applies_sent_fields_and_present_null_clears() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    let mut input = minimal("Alpha");
    input.reward_ped = Some(5.0);
    api.quest_create(input).await.unwrap();

    // The client sends the full field set; a present null clears the
    // column, and the name it re-sends is unchanged.
    let mut patch = minimal("Alpha");
    patch.notes = Some("updated".to_string());
    patch.reward_ped = None; // present-null clears the reward
    let updated = api.quest_update(1, patch).await.unwrap();
    assert_eq!(updated.notes, "updated");
    assert_eq!(updated.reward, None);
    assert_eq!(updated.name, "Alpha");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lifecycle_starts_completes_and_cancels() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    let mut input = minimal("Cycle");
    input.cooldown_hours = Some(0.0);
    api.quest_create(input).await.unwrap();

    let started = api.quest_start(1).await.unwrap();
    assert!(started.started_at.is_some());

    let completed = api.quest_complete(1).await.unwrap();
    assert_eq!(completed.cooldown_expires_at, None); // zero-cooldown

    api.quest_start(1).await.unwrap();
    let cancelled = api.quest_cancel(1, false).await.unwrap();
    assert_eq!(cancelled.started_at, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_playlist_create_derives_membership_from_its_items() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    let input = PlaylistInput {
        name: "Run".to_string(),
        planet: "Calypso".to_string(),
        estimated_minutes: 45,
        items: vec![PlaylistItemInput {
            quest_id: 3,
            description: None,
            group_type: "long_horizon".to_string(),
        }],
    };
    let created = api.playlist_create(input).await.unwrap();

    // Transport invariance: the item list drives the classified id sets,
    // ids stringify, and the shape matches the HTTP route byte for byte.
    assert_eq!(
        serde_json::to_string(&created).unwrap(),
        "{\"id\":\"1\",\"name\":\"Run\",\"planet\":\"Calypso\",\"estimatedMinutes\":45,\
         \"questIds\":[\"3\"],\"immediateQuestIds\":[],\"longHorizonQuestIds\":[\"3\"],\
         \"items\":[{\"questId\":\"3\",\"description\":null,\"groupType\":\"long_horizon\"}]}"
    );

    // Rename with the membership re-sent leaves it intact.
    let renamed = PlaylistInput {
        name: "Run 2".to_string(),
        planet: "Calypso".to_string(),
        estimated_minutes: 45,
        items: vec![PlaylistItemInput {
            quest_id: 3,
            description: None,
            group_type: "long_horizon".to_string(),
        }],
    };
    let updated = api.playlist_update(1, renamed).await.unwrap();
    assert_eq!(updated.name, "Run 2");
    assert_eq!(updated.long_horizon_quest_ids, vec!["3".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_not_found_legs_answer_the_typed_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    assert_eq!(
        api.quest_get(424242).await.unwrap_err(),
        ApiError::not_found("Quest not found")
    );
    assert_eq!(
        api.quest_update(424242, minimal("Z")).await.unwrap_err(),
        ApiError::not_found("Quest not found")
    );
    assert_eq!(
        api.quest_delete(424242).await.unwrap_err(),
        ApiError::not_found("Quest not found")
    );
    assert_eq!(
        api.quest_start(424242).await.unwrap_err(),
        ApiError::not_found("Quest not found")
    );
    assert_eq!(
        api.playlist_update(
            424242,
            PlaylistInput {
                name: "Z".to_string(),
                planet: "Calypso".to_string(),
                estimated_minutes: 30,
                items: Vec::new(),
            }
        )
        .await
        .unwrap_err(),
        ApiError::not_found("Playlist not found")
    );
    assert_eq!(
        api.playlist_delete(424242).await.unwrap_err(),
        ApiError::not_found("Playlist not found")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_soft_deletes_off_the_active_list() {
    let dir = tempfile::tempdir().unwrap();
    let api = quests_api(dir.path()).await;

    api.quest_create(minimal("Doomed")).await.unwrap();
    api.quest_delete(1).await.unwrap();
    // Delete is a soft delete: the quest drops off the active listing but
    // stays retrievable by id (the HTTP route's own contract; `get_quest`
    // carries no `is_active` filter).
    assert!(api.quests_list().await.unwrap().is_empty());
    assert_eq!(api.quest_get(1).await.unwrap().name, "Doomed");

    // A second delete finds no active row to soft-delete: the 404 leg.
    assert_eq!(
        api.quest_delete(1).await.unwrap_err(),
        ApiError::not_found("Quest not found")
    );
}
