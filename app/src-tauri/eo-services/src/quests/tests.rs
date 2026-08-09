// Expected values in these tests are the original implementation's
// outputs, computed by running the original Python implementation
// over byte-identical payloads and database seeds (created_at and
// updated_at pinned by direct UPDATE on both sides, since the schema
// stamps them from the wall clock).

use std::sync::Arc;

use rusqlite::params;
use serde_json::{json, Value};
use tokio::runtime::Handle;

use crate::bus_events::{
    BusEvent, MissionReceivedPayload, MissionReceivedTag, SessionLifecyclePayload,
};
use crate::chatlog_watcher::{MissionCompletion, SignalLoot};
use crate::db::Db;

use super::lifecycle::{delete_latest_quest_claim, delete_latest_quest_reward_entry};
use super::payload::json_truthy;
use super::QuestService;
use crate::ped::Ped;

type ServiceRig = (
    Arc<QuestService>,
    Db,
    Arc<crate::clock::MockClock>,
    Arc<crate::event_bus::EventBus>,
);

async fn service_with_clock(dir: &std::path::Path) -> ServiceRig {
    let db = Db::open(&dir.join("entropia_orme.db")).await.unwrap();
    let clock = Arc::new(crate::clock::MockClock::new(
        Some(
            chrono::NaiveDateTime::parse_from_str("2026-03-01 12:00:00", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        ),
        0.0,
    ));
    let bus = Arc::new(crate::event_bus::EventBus::new());
    let counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let svc = QuestService::start_with_id_source(
        &bus,
        db.clone(),
        clock.clone(),
        Handle::current(),
        Arc::new(move || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            format!("fixed-{n:04}")
        }),
    );
    (svc, db, clock, bus)
}

async fn service(dir: &std::path::Path) -> (Arc<QuestService>, Db) {
    let (svc, db, _clock, _bus) = service_with_clock(dir).await;
    (svc, db)
}

async fn pin_ts(db: &Db, table: &str, id: i64, ts: f64) {
    let table = table.to_string();
    db.with_writer(move |conn| {
        conn.execute(
            &format!("UPDATE {table} SET created_at = ?1, updated_at = ?2 WHERE id = ?3"),
            params![ts, ts, id],
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

/// A `SELECT COUNT(*) FROM ...` with no bound parameters, the shape
/// repeated throughout these tests to check a table's row count.
async fn count_rows(db: &Db, sql: &'static str) -> i64 {
    db.with_reader(move |conn| {
        conn.query_row(sql, [], |row| row.get(0))
            .map_err(Into::into)
    })
    .await
    .unwrap()
}

fn quest_id(value: &Value) -> i64 {
    value["id"].as_i64().unwrap()
}

fn full_quest_payload() -> Value {
    json!({
        "name": "Atrox Cull", "planet": "Foma", "waypoint": "/wp 1,2",
        "cooldown_hours": 24, "reward_ped": 12.5, "reward_is_skill": false,
        "expected_reward_markup_percent": 150.0, "notes": "bring fap",
        "chain_name": "Cull", "chain_position": 1, "chain_total": 3,
        "category": "hunt", "reward_description": "ammo",
        "mobs": [" Atrox ", "", "Atrax", "Atrox"],
    })
}

#[tokio::test]
async fn quest_claim_undo_relands_the_days_rollups() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("entropia_orme.db"))
        .await
        .unwrap();

    // A historical skill-reward claim and a liquid-reward ledger
    // entry, both two days behind the heal watermark.
    let claimed_at = 999_700_000.0; // inside 2001-09-05 UTC
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO quest_claims (quest_id, quest_name, ped_value, claimed_at) \
             VALUES (7, 'Iron Atrox', 2.5, ?1)",
            params![claimed_at],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
             VALUES ('q1', '2001-09-05', 'markup', 'Quest: Daily Feffoid', 4.0, 'quest_reward')",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    db.with_writer(move |conn| {
        crate::daily_rollup::heal_rollups(conn, claimed_at + 3.0 * 86_400.0)
    })
    .await
    .unwrap();
    let day = crate::daily_rollup::epoch_day(claimed_at);
    let day_for_read = day.clone();
    let quest_pes: Option<f64> = db
        .with_reader(move |conn| {
            conn.query_row(
                "SELECT quest_pes FROM daily_rollups WHERE day = ?1",
                params![day_for_read],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(quest_pes, Some(2.5));

    // Both undo paths reland their day inside the caller's commit
    // semantics.
    let (claim_undone, reward_undone) = db
        .with_writer(move |conn| {
            let claim_undone = delete_latest_quest_claim(conn, 7)?;
            let reward_undone = delete_latest_quest_reward_entry(conn, "Daily Feffoid", Ped(4.0))?;
            Ok((claim_undone, reward_undone))
        })
        .await
        .unwrap();
    assert!(claim_undone);
    assert!(reward_undone);
    let day_for_read = day.clone();
    let quest_pes: Option<f64> = db
        .with_reader(move |conn| {
            conn.query_row(
                "SELECT quest_pes FROM daily_rollups WHERE day = ?1",
                params![day_for_read],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(quest_pes, None, "the undone claim left the day");
    let day_for_read = day.clone();
    let ledger_rows: i64 = db
        .with_reader(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM daily_ledger_rollups WHERE day = ?1 AND tag = 'quest_reward'",
                params![day_for_read],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(ledger_rows, 0, "the undone reward left the day");
}

#[tokio::test]
async fn creates_apply_defaults_normalisation_and_mob_rules() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;

    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", q1, 1000.0).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());
    pin_ts(&db, "quests", q2, 1001.0).await;
    let q3 = quest_id(
        &svc.create_quest(&json!({
            "name": "Skill Run", "reward_ped": 5.0, "reward_is_skill": true,
            "expected_reward_markup_percent": 120.0,
        }))
        .await
        .unwrap(),
    );
    pin_ts(&db, "quests", q3, 1002.0).await;
    assert_eq!((q1, q2, q3), (1, 2, 3));

    // The minimal quest: planet defaults, everything else null,
    // the skill flag stored as integer 0.
    let q1_fresh = svc.get_quest(q1).await.unwrap().unwrap();
    assert_eq!(
        q1_fresh,
        json!({
            "id": 1, "name": "Iron Challenge", "planet": "Calypso", "waypoint": null,
            "cooldown_hours": null, "reward_ped": null, "reward_is_skill": 0,
            "expected_reward_markup_percent": null, "notes": null, "chain_name": null,
            "chain_position": null, "chain_total": null, "started_at": null,
            "is_active": 1, "created_at": 1000.0, "category": null,
            "reward_description": null, "updated_at": 1000.0, "signal_loot_item": null,
            "family_id": null, "cooldown_anchor": "completion", "last_started_at": null,
            "family_name": null, "family_cooldown_hours": null,
            "family_cooldown_anchor": null, "last_completed_at": null,
            "cooldown_expires_at": null, "family_cooldown_expires_at": null,
            "mobs": [], "playlist_ids": [],
        })
    );

    // The full quest: mobs strip, drop empties, dedupe, and read
    // back sorted; the integer cooldown stores as REAL; a liquid
    // positive reward keeps its markup.
    let q2_fresh = svc.get_quest(q2).await.unwrap().unwrap();
    assert_eq!(q2_fresh["planet"], "Foma");
    assert_eq!(q2_fresh["cooldown_hours"], json!(24.0));
    assert_eq!(q2_fresh["expected_reward_markup_percent"], json!(150.0));
    assert_eq!(q2_fresh["mobs"], json!(["Atrax", "Atrox"]));
    assert_eq!(q2_fresh["reward_is_skill"], json!(0));
    assert_eq!(q2_fresh["chain_position"], json!(1));

    // A skill reward normalises its markup away at creation.
    let q3_fresh = svc.get_quest(q3).await.unwrap().unwrap();
    assert_eq!(q3_fresh["reward_is_skill"], json!(1));
    assert_eq!(q3_fresh["expected_reward_markup_percent"], Value::Null);
}

#[tokio::test]
async fn cooldown_derives_from_the_latest_completion() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());

    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES ('sess-1', ?1, 1772366400.0)",
            params![q2],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let quest = svc.get_quest(q2).await.unwrap().unwrap();
    assert_eq!(quest["last_completed_at"], json!(1772366400.0));
    assert_eq!(
        quest["cooldown_expires_at"],
        json!("2026-03-02T12:00:00+00:00"),
        "completion instant plus 24 hours, rendered as a UTC ISO instant"
    );
}

#[tokio::test]
async fn updates_merge_and_renormalise_the_markup() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );

    // Setting a positive liquid reward with a markup keeps it.
    let updated = svc
        .update_quest(
            q1,
            &json!({"reward_ped": 10.0, "expected_reward_markup_percent": 130.0}),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["reward_ped"], json!(10.0));
    assert_eq!(updated["expected_reward_markup_percent"], json!(130.0));

    // Flipping to a skill reward re-normalises the merged picture:
    // the stored markup clears even though the update names only
    // the flag.
    let updated = svc
        .update_quest(q1, &json!({"reward_is_skill": true}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["reward_is_skill"], json!(1));
    assert_eq!(updated["expected_reward_markup_percent"], Value::Null);

    assert_eq!(
        svc.update_quest(9999, &json!({"name": "x"})).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn deletes_are_soft_and_detach_playlist_items() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", q1, 1000.0).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());
    pin_ts(&db, "quests", q2, 1001.0).await;
    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Morning Run", "quest_ids": [q1, q2]}))
            .await
            .unwrap(),
    );

    assert!(svc.delete_quest(q2).await.unwrap());
    assert!(!svc.delete_quest(q2).await.unwrap(), "already inactive");

    let active: Vec<i64> = svc
        .get_quests(true)
        .await
        .unwrap()
        .iter()
        .map(quest_id)
        .collect();
    assert_eq!(active, [q1]);
    let all: Vec<i64> = svc
        .get_quests(false)
        .await
        .unwrap()
        .iter()
        .map(quest_id)
        .collect();
    assert_eq!(all, [q1, q2]);

    // The deleted quest left the playlist.
    let playlist = svc.get_playlist(p1).await.unwrap().unwrap();
    assert_eq!(playlist["quest_ids"], json!([q1]));

    // Mob autocomplete reads active quests only.
    assert_eq!(svc.get_all_mob_names().await.unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn playlists_classify_items_and_split_groups() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());

    // A bare id list classifies everything immediate, with the
    // planet and duration defaults.
    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Morning Run", "quest_ids": [q1, q2]}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quest_playlists", p1, 2000.0).await;
    let p1_fresh = svc.get_playlist(p1).await.unwrap().unwrap();
    assert_eq!(
        p1_fresh,
        json!({
            "id": 1, "name": "Morning Run", "planet": "Calypso",
            "estimated_minutes": 30, "is_active": 1, "created_at": 2000.0,
            "updated_at": 2000.0, "quest_ids": [1, 2],
            "immediate_quest_ids": [1, 2], "long_horizon_quest_ids": [],
            "items": [
                {"quest_id": 1, "description": null, "group_type": "immediate"},
                {"quest_id": 2, "description": null, "group_type": "immediate"},
            ],
        })
    );

    // Classified items keep their groups; immediate items list
    // ahead of long-horizon ones regardless of insertion order.
    let p2 = quest_id(
        &svc.create_playlist(&json!({
            "name": "Big Loop", "planet": "Foma", "estimated_minutes": 90,
            "items": [
                {"quest_id": q2, "description": "warmup", "group_type": "immediate"},
                {"quest_id": q1, "group_type": "long_horizon"},
            ],
        }))
        .await
        .unwrap(),
    );
    let p2_fresh = svc.get_playlist(p2).await.unwrap().unwrap();
    assert_eq!(p2_fresh["quest_ids"], json!([q2, q1]));
    assert_eq!(p2_fresh["immediate_quest_ids"], json!([q2]));
    assert_eq!(p2_fresh["long_horizon_quest_ids"], json!([q1]));
    assert_eq!(
        p2_fresh["items"],
        json!([
            {"quest_id": q2, "description": "warmup", "group_type": "immediate"},
            {"quest_id": q1, "description": null, "group_type": "long_horizon"},
        ])
    );

    // Updates rewrite items from either payload shape, and soft
    // deletes clear them.
    let updated = svc
        .update_playlist(p1, &json!({"name": "Dawn Run", "quest_ids": [q2]}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["name"], "Dawn Run");
    assert_eq!(updated["quest_ids"], json!([q2]));
    assert!(svc.delete_playlist(p2).await.unwrap());
    assert!(!svc.delete_playlist(p2).await.unwrap());
    assert_eq!(svc.get_playlists(true).await.unwrap().len(), 1);
    assert_eq!(svc.get_playlists(false).await.unwrap().len(), 2);
    assert_eq!(svc.get_playlist(9999).await.unwrap(), None);
    assert_eq!(
        svc.update_playlist(9999, &json!({"name": "x"}))
            .await
            .unwrap(),
        None
    );

    // The active quest's playlist membership reflects only live
    // playlists.
    let q2_now = svc.get_quest(q2).await.unwrap().unwrap();
    assert_eq!(q2_now["playlist_ids"], json!([p1]));
}

#[tokio::test]
async fn invalid_groups_reject_verbatim_and_leave_no_trace() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Morning Run", "quest_ids": [q1]}))
            .await
            .unwrap(),
    );

    let error = svc
        .create_playlist(&json!({
            "name": "Bad",
            "items": [{"quest_id": q1, "group_type": "weekly"}],
        }))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Invalid playlist group type: weekly");

    // A present-but-null group is rendered the way the original's
    // message renders None.
    let error = svc
        .update_playlist(
            p1,
            &json!({"items": [{"quest_id": q1, "group_type": null}]}),
        )
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Invalid playlist group type: None");

    // The failed writes roll back whole: no phantom playlist, and
    // the failed item rewrite keeps the prior items. (The original
    // leaves these partial writes pending on its shared connection
    // for a later commit to ratify; the pooled port repairs that
    // by construction, per the migration's settled architecture.)
    let playlists = svc.get_playlists(true).await.unwrap();
    assert_eq!(playlists.len(), 1);
    assert_eq!(playlists[0]["name"], "Morning Run");
    assert_eq!(playlists[0]["quest_ids"], json!([q1]));
}

#[tokio::test]
async fn present_null_lists_refuse_and_leave_state_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge", "mobs": ["Atrox"]}))
            .await
            .unwrap(),
    );
    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Morning Run", "quest_ids": [q1]}))
            .await
            .unwrap(),
    );

    // An explicit-null quest_ids update refuses (the original
    // crashes iterating it, with no surviving write) instead of
    // clearing the playlist.
    let error = svc
        .update_playlist(p1, &json!({"quest_ids": null}))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "'quest_ids' must be a list of quest ids");
    let playlist = svc.get_playlist(p1).await.unwrap().unwrap();
    assert_eq!(playlist["quest_ids"], json!([q1]));

    // An explicit-null mobs update refuses likewise; the mob rows
    // survive (the original's crash leaves its mob delete pending
    // for the next commit to ratify silently; the typed refusal
    // plus rollback is the sanctioned repair shape).
    let error = svc
        .update_quest(q1, &json!({"mobs": null}))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "'mobs' must be a list of mob names");
    let quest = svc.get_quest(q1).await.unwrap().unwrap();
    assert_eq!(quest["mobs"], json!(["Atrox"]));
}

#[tokio::test]
async fn create_quest_ignores_a_falsy_mobs_payload_without_dereferencing_it() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    // A present-but-null mobs value is falsy on create: it writes no mob
    // rows and must never be dereferenced as a list.
    let created = svc
        .create_quest(&json!({"name": "Bounty", "mobs": null}))
        .await
        .unwrap();
    let id = quest_id(&created);
    let stored = svc.get_quest(id).await.unwrap().unwrap();
    assert_eq!(stored["name"], "Bounty");
    assert_eq!(stored["mobs"], json!([]), "no mob rows written");
}

#[tokio::test]
async fn soft_deleting_a_quest_keeps_its_mob_rows() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge", "mobs": ["Atrox"]}))
            .await
            .unwrap(),
    );

    assert!(svc.delete_quest(q1).await.unwrap());
    // The soft delete detaches playlist items only; the mob rows
    // stay (the autocomplete reader filters by active quests, so
    // they vanish from that surface without being destroyed).
    let mobs: i64 = db
        .with_reader(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM quest_mobs WHERE quest_id = ?1",
                params![q1],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .await
        .unwrap();
    assert_eq!(mobs, 1);
    assert_eq!(svc.get_all_mob_names().await.unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn mob_autocomplete_lists_active_quest_mobs_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    svc.create_quest(&full_quest_payload()).await.unwrap();
    svc.create_quest(&json!({"name": "Side Hunt", "mobs": ["Snablesnot"]}))
        .await
        .unwrap();
    assert_eq!(
        svc.get_all_mob_names().await.unwrap(),
        ["Atrax", "Atrox", "Snablesnot"]
    );
}

#[tokio::test]
async fn a_zero_hour_cooldown_never_produces_an_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let mut payload = full_quest_payload();
    payload["cooldown_hours"] = json!(0);
    let q = quest_id(&svc.create_quest(&payload).await.unwrap());
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES ('sess-1', ?1, 1772366400.0)",
            params![q],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let quest = svc.get_quest(q).await.unwrap().unwrap();
    assert_eq!(quest["last_completed_at"], json!(1772366400.0));
    assert_eq!(
        quest["cooldown_expires_at"],
        Value::Null,
        "the expiry derives only from a strictly positive cooldown"
    );
}

#[tokio::test]
async fn a_null_items_payload_clears_the_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Morning Run", "quest_ids": [q1]}))
            .await
            .unwrap(),
    );

    // The original's is-not-None test routes a present-null items
    // payload to the quest_ids leg, which is absent, so the
    // rewrite clears every item; null items is the documented
    // clear-all shape, unlike null quest_ids which refuses.
    let updated = svc
        .update_playlist(p1, &json!({"items": null}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["quest_ids"], json!([]));
    assert_eq!(updated["items"], json!([]));
}

/// One walk through the lifecycle, mirroring the original's run
/// over identical payloads, clock advances, and identifier
/// streams; every expected value below is the original's output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lifecycle_walkthrough_matches_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db, clock, bus) = service_with_clock(dir.path()).await;

    let qa = quest_id(
        &svc.create_quest(
            &json!({"name": "Iron Challenge", "reward_ped": 2.5, "cooldown_hours": 24}),
        )
        .await
        .unwrap(),
    );
    pin_ts(&db, "quests", qa, 1000.0).await;
    let qb = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunt: Atrox", "reward_ped": 5.0,
                                   "reward_is_skill": true, "cooldown_hours": 1}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qb, 1001.0).await;
    let qc = quest_id(
        &svc.create_quest(&json!({"name": "G\u{e9}ologist Survey"}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qc, 1002.0).await;
    let qe = quest_id(
        &svc.create_quest(&json!({"name": "Zero Bounty", "reward_ped": 0}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qe, 1003.0).await;

    // Start legs.
    assert_eq!(svc.start_quest(9999).await.unwrap(), None);
    let started = svc.start_quest(qa).await.unwrap().unwrap();
    assert_eq!(started["started_at"], json!(1772366400.0));

    // A session-less completion: a ledger row (liquid reward) and
    // a synthetic manual completion key.
    clock.advance(60.0).unwrap();
    let done = svc.complete_quest(qa).await.unwrap().unwrap();
    assert_eq!(done["started_at"], Value::Null);
    assert_eq!(done["last_completed_at"], json!(1772366460.0));
    assert_eq!(
        done["cooldown_expires_at"],
        json!("2026-03-02T12:01:00+00:00")
    );
    let ledger = |sql: &'static str| {
        let db = db.clone();
        async move {
            db.with_reader(move |conn| {
                let mut stmt = conn.prepare(sql)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(json!([
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, String>(4)?,
                        ]))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
            .unwrap()
        }
    };
    assert_eq!(
        ledger("SELECT id, date, description, amount, tag FROM ledger_entries ORDER BY id").await,
        vec![json!([
            "fixed-0001",
            "2026-03-01T12:01:00+00:00",
            "Quest: Iron Challenge",
            2.5,
            "quest_reward"
        ])]
    );
    let completions = |db: Db| async move {
        db.with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT session_id, quest_id, completed_at FROM session_quest_completions ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!([
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, f64>(2)?,
                    ]))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap()
    };
    assert_eq!(
        completions(db.clone()).await,
        vec![json!(["manual-fixed-0002", qa, 1772366460.0])]
    );
    let reward_provenance = db
        .with_reader(|conn| {
            Ok(conn.query_row(
                "SELECT reward_source, reward_ped, ledger_entry_id, quest_claim_id \
                 FROM session_quest_completions WHERE quest_id = ?",
                params![qa],
                |row| {
                    Ok(json!([
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ]))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(
        reward_provenance,
        json!(["ledger", 2.5, "fixed-0001", null])
    );

    // The bus feeds the active session; a session-scoped skill
    // completion writes a claim, and a repeat in the same session is
    // idempotent across both the completion and its linked reward.
    let attribution_db = db.clone();
    attribution_db
        .with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id, started_at, is_active) \
                 VALUES('sess-abc', 1772366400.0, 1)",
                [],
            )?;
            conn.execute(
                "INSERT INTO session_intervals(id, session_id, kind, label, ref_id, started_at) \
                 VALUES(901, 'sess-abc', 'quest', 'Daily Hunt: Atrox', ?1, 1772366400.0)",
                params![qb],
            )?;
            conn.execute(
                "INSERT INTO session_contexts(id, session_id, created_at) \
                 VALUES(902, 'sess-abc', 1772366400.0)",
                [],
            )?;
            conn.execute(
                "INSERT INTO session_context_intervals(context_id, interval_id) VALUES(902, 901)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    bus.publish(&BusEvent::SessionStarted(SessionLifecyclePayload {
        session_id: "sess-abc".into(),
    }));
    clock.advance(60.0).unwrap();
    svc.complete_quest(qb).await.unwrap().unwrap();
    clock.advance(60.0).unwrap();
    svc.complete_quest(qb).await.unwrap().unwrap();
    let claims = db
        .with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT quest_id, quest_name, ped_value, claimed_at FROM quest_claims ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!([
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                    ]))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    assert_eq!(
        claims,
        vec![json!([qb, "Daily Hunt: Atrox", 5.0, 1772366520.0])]
    );
    assert_eq!(
        completions(db.clone()).await,
        vec![
            json!(["manual-fixed-0002", qa, 1772366460.0]),
            json!(["sess-abc", qb, 1772366520.0]),
        ]
    );
    let captured_activity = db
        .with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT activity_context_id, activity_interval_id, reward_source, quest_claim_id \
                 FROM session_quest_completions WHERE session_id = 'sess-abc'",
                [],
                |row| {
                    Ok(json!([
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ]))
                },
            )?)
        })
        .await
        .unwrap();
    assert_eq!(captured_activity, json!([902, 901, "skill", 1]));

    // Cancel legs: a started quest clears; a quest neither started
    // nor cooling passes through; a cooling quest resets its
    // cooldown and (optionally) undoes the reward.
    svc.start_quest(qc).await.unwrap().unwrap();
    let cancelled = svc.cancel_quest(qc, false).await.unwrap().unwrap();
    assert_eq!(cancelled["started_at"], Value::Null);
    let passthrough = svc.cancel_quest(qc, false).await.unwrap().unwrap();
    assert_eq!(passthrough["id"], json!(qc));
    clock.advance(60.0).unwrap();
    svc.cancel_quest(qb, true).await.unwrap().unwrap();
    let claim_count = count_rows(&db, "SELECT COUNT(*) FROM quest_claims").await;
    assert_eq!(claim_count, 0, "the linked claim is undone exactly");
    svc.cancel_quest(qa, true).await.unwrap().unwrap();
    let ledger_count = count_rows(&db, "SELECT COUNT(*) FROM ledger_entries").await;
    assert_eq!(ledger_count, 0, "the reward ledger entry is undone");

    // The suggestion tree, reason by reason.
    let sugg = |s: Value, t: &str, r: &str| {
        assert_eq!(s["suggestion_type"], t, "type for {r}");
        assert_eq!(s["reason"], r);
        s
    };
    sugg(
        svc.get_session_link_suggestion("sess-none").await.unwrap(),
        "none",
        "no_completions",
    );
    for (session, quest, at) in [
        ("sess-one", qa, 5000.0),
        ("sess-two", qa, 5001.0),
        ("sess-two", qb, 5002.0),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES (?1, ?2, ?3)",
                params![session, quest, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let single = sugg(
        svc.get_session_link_suggestion("sess-one").await.unwrap(),
        "quest",
        "single_quest",
    );
    assert_eq!(single["quest_name"], "Iron Challenge");
    svc.create_playlist(&json!({"name": "Pair Run", "quest_ids": [qa, qb]}))
        .await
        .unwrap();
    let pl = sugg(
        svc.get_session_link_suggestion("sess-two").await.unwrap(),
        "playlist",
        "exact_playlist",
    );
    assert_eq!(pl["playlist_name"], "Pair Run");
    clock.advance(60.0).unwrap();
    // Historical link rows still gate the read: nothing writes the
    // demoted table any more (the recorded quest stretch superseded
    // it), so the already-linked and declined branches are exercised
    // over rows inserted as the legacy data they now are.
    for (session, link_type) in [("sess-two", "playlist"), ("sess-decl", "declined")] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_analytics_links \
                 (session_id, link_type, quest_id, playlist_id, linked_at) \
                 VALUES (?1, ?2, NULL, NULL, 1772366700.0)",
                params![session, link_type],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    sugg(
        svc.get_session_link_suggestion("sess-two").await.unwrap(),
        "none",
        "already_linked",
    );
    sugg(
        svc.get_session_link_suggestion("sess-decl").await.unwrap(),
        "none",
        "declined",
    );
    for (session, quest, at) in [("sess-three", qa, 5003.0), ("sess-three", qc, 5004.0)] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES (?1, ?2, ?3)",
                params![session, quest, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    sugg(
        svc.get_session_link_suggestion("sess-three").await.unwrap(),
        "none",
        "unclean",
    );
    svc.create_playlist(&json!({"name": "Pair Run B", "quest_ids": [qa, qb]}))
        .await
        .unwrap();
    for (session, quest, at) in [("sess-five", qa, 5005.0), ("sess-five", qb, 5006.0)] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES (?1, ?2, ?3)",
                params![session, quest, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    sugg(
        svc.get_session_link_suggestion("sess-five").await.unwrap(),
        "none",
        "ambiguous_playlist",
    );
    // Mission matching: exact (case/space), accent folding,
    // repeatable suffix, containment, fuzzy at the threshold, and
    // a miss below it.
    let match_id = |name: &'static str| {
        let svc = svc.clone();
        async move {
            svc.match_quest_by_mission_name(name, false)
                .await
                .unwrap()
                .map(|quest| quest["id"].as_i64().unwrap())
        }
    };
    assert_eq!(match_id("  IRON CHALLENGE ").await, Some(qa));
    assert_eq!(match_id("Geologist Survey").await, Some(qc));
    assert_eq!(match_id("Iron Challenge (Repeatable)").await, Some(qa));
    assert_eq!(match_id("Mission: Iron Challenge Part II").await, Some(qa));
    assert_eq!(match_id("Iron Chalenge").await, Some(qa));
    assert_eq!(match_id("Totally Different").await, None);

    // Mission auto-start: unknown ignores, a fuzzy match starts
    // once, and an already-started quest skips.
    clock.advance(60.0).unwrap();
    svc.start_quest_from_mission("Unknown Mission")
        .await
        .unwrap();
    svc.start_quest_from_mission("Iron Chalenge").await.unwrap();
    assert!(json_truthy(
        svc.get_quest(qa).await.unwrap().unwrap().get("started_at")
    ));
    svc.start_quest_from_mission("Iron Challenge")
        .await
        .unwrap();

    // The reward filter's five legs, each now a pre-publish
    // suppression answer paired with the post-publish completion
    // check (the tick's loot must reach the consumers before the
    // completion closes anything, so the two are separate calls);
    // the suppression decisions and the overlay trail they leave
    // are the original's. The filter's in-progress gate means each
    // leg starts its quest first, as the mission log would have.
    let complete_tick = |mission: &'static str, loot: Vec<Value>, skills: Vec<Value>| {
        let svc = svc.clone();
        async move {
            svc.mission_complete_check(&[MissionCompletion {
                mission_name: mission.to_string(),
                loot_items: loot,
                skill_gains: skills,
            }])
            .await
            .unwrap();
        }
    };
    clock.advance(60.0).unwrap();
    svc.start_quest(qb).await.unwrap().unwrap();
    let atrox_skills = vec![json!({"skill_name": "Rifle", "amount": 1.0})];
    assert_eq!(
        svc.quest_reward_filter("Daily Hunt: Atrox", &[], &atrox_skills)
            .await
            .unwrap(),
        Some(json!({"suppress_loot_index": null, "suppress_skill_index": 0}))
    );
    complete_tick("Daily Hunt: Atrox", vec![], atrox_skills).await;
    clock.advance(60.0).unwrap();
    svc.start_quest(qa).await.unwrap().unwrap();
    let iron_loot = vec![
        json!({"item_name": "Shrapnel", "quantity": 100, "value": 0.1}),
        json!({"item_name": "Universal Ammo", "quantity": 1, "value": 2.51}),
    ];
    assert_eq!(
        svc.quest_reward_filter("Iron Challenge", &iron_loot, &[])
            .await
            .unwrap(),
        Some(json!({"suppress_loot_index": 1, "suppress_skill_index": null}))
    );
    complete_tick("Iron Challenge", iron_loot, vec![]).await;
    clock.advance(60.0).unwrap();
    svc.start_quest(qa).await.unwrap().unwrap();
    let bare_loot = vec![json!({"item_name": "Shrapnel", "quantity": 100, "value": 0.1})];
    assert_eq!(
        svc.quest_reward_filter("Iron Challenge", &bare_loot, &[])
            .await
            .unwrap(),
        None
    );
    complete_tick("Iron Challenge", bare_loot, vec![]).await;
    clock.advance(60.0).unwrap();
    svc.start_quest(qe).await.unwrap().unwrap();
    let bounty_loot = vec![
        json!({"item_name": "A", "value": 0.5}),
        json!({"item_name": "B", "value": 0.2}),
        json!({"item_name": "C", "value": 0.9}),
    ];
    assert_eq!(
        svc.quest_reward_filter("Zero Bounty", &bounty_loot, &[])
            .await
            .unwrap(),
        Some(json!({"suppress_loot_index": 1, "suppress_skill_index": null}))
    );
    complete_tick("Zero Bounty", bounty_loot, vec![]).await;
    clock.advance(60.0).unwrap();
    svc.start_quest(qc).await.unwrap().unwrap();
    let survey_loot = vec![json!({"item_name": "A", "value": 0.5})];
    assert_eq!(
        svc.quest_reward_filter("Geologist Survey", &survey_loot, &[])
            .await
            .unwrap(),
        None
    );
    complete_tick("Geologist Survey", survey_loot, vec![]).await;
    // A completion line for a quest the log does not carry as in
    // progress is not ours to act on: no suppression, no completion.
    assert_eq!(
        svc.quest_reward_filter("Iron Challenge", &[], &[])
            .await
            .unwrap(),
        None
    );
    let trail_before = count_rows(&db, "SELECT COUNT(*) FROM notable_events").await;
    complete_tick("Iron Challenge", vec![], vec![]).await;
    let trail_after = count_rows(&db, "SELECT COUNT(*) FROM notable_events").await;
    assert_eq!(trail_after, trail_before, "an idle quest never completes");

    // The overlay trail, exactly as the original recorded it.
    let events = db
        .with_reader(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT session_id, kill_id, event_type, mob_or_item, value_ped, timestamp \
                 FROM notable_events ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(json!([
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, f64>(5)?,
                    ]))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    assert_eq!(
        events,
        vec![
            json!([
                "sess-abc",
                null,
                "quest_started",
                "Iron Challenge",
                0.0,
                1772366760.0
            ]),
            json!([
                "sess-abc",
                null,
                "quest_completed_pes",
                "Daily Hunt: Atrox: skill reward suppressed",
                5.0,
                1772366820.0
            ]),
            json!([
                "sess-abc",
                null,
                "quest_completed",
                "Iron Challenge: Universal Ammo (2.50 PED) suppressed",
                2.5,
                1772366880.0
            ]),
            json!([
                "sess-abc",
                null,
                "quest_completed",
                "Iron Challenge",
                2.5,
                1772366940.0
            ]),
            json!([
                "sess-abc",
                null,
                "quest_completed",
                "Zero Bounty: B suppressed",
                0.0,
                1772367000.0
            ]),
            json!([
                "sess-abc",
                null,
                "quest_completed",
                "G\u{e9}ologist Survey",
                0.0,
                1772367060.0
            ]),
        ]
    );

    // The final ledger carries exactly the two liquid completions
    // the filter recorded; the zero-reward completion wrote none.
    let final_ledger: Vec<String> = db
        .with_reader(move |conn| {
            let mut stmt = conn.prepare("SELECT id FROM ledger_entries ORDER BY id")?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
        .unwrap();
    assert_eq!(final_ledger, ["fixed-0003", "fixed-0004"]);

    // A session stop clears the tracked session: notable events
    // stop recording.
    bus.publish(&BusEvent::SessionStopped(SessionLifecyclePayload {
        session_id: "s1".into(),
    }));
    svc.start_quest_from_mission("Geologist Survey")
        .await
        .unwrap();
    let count = count_rows(&db, "SELECT COUNT(*) FROM notable_events").await;
    assert_eq!(count, 6, "no session, no overlay event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_received_mission_event_starts_its_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db, _clock, bus) = service_with_clock(dir.path()).await;
    let q = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );

    bus.publish(&BusEvent::MissionReceived(MissionReceivedPayload {
        kind: MissionReceivedTag,
        timestamp: "2026-01-01T00:00:01".into(),
        mission_name: "Iron Challenge".into(),
    }));
    assert!(json_truthy(
        svc.get_quest(q).await.unwrap().unwrap().get("started_at")
    ));
    // A nameless event is ignored.
    bus.publish(&BusEvent::MissionReceived(MissionReceivedPayload {
        kind: MissionReceivedTag,
        timestamp: "2026-01-01T00:00:01".into(),
        mission_name: "".into(),
    }));
}

#[tokio::test]
async fn starting_an_inactive_quest_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q = quest_id(&svc.create_quest(&json!({"name": "Dead"})).await.unwrap());
    svc.delete_quest(q).await.unwrap();
    assert_eq!(svc.start_quest(q).await.unwrap(), None);
}

#[tokio::test]
async fn equal_fuzzy_scores_keep_the_first_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let first = quest_id(
        &svc.create_quest(&json!({"name": "iron chal a"}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", first, 1000.0).await;
    let second = quest_id(
        &svc.create_quest(&json!({"name": "iron chal b"}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", second, 1001.0).await;

    // Both names score 0.9090909090909091 against the mission (the
    // reference's figure); the strictly-greater comparison keeps
    // the earlier quest.
    let matched = svc
        .match_quest_by_mission_name("iron chal c", false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(matched["id"], json!(first));
}

#[tokio::test]
async fn filter_ties_keep_the_first_item() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let tie = quest_id(
        &svc.create_quest(&json!({"name": "Tie Quest", "reward_ped": 2.5}))
            .await
            .unwrap(),
    );
    let zed = quest_id(
        &svc.create_quest(&json!({"name": "Zed Bounty", "reward_ped": 0}))
            .await
            .unwrap(),
    );
    svc.start_quest(tie).await.unwrap().unwrap();
    svc.start_quest(zed).await.unwrap().unwrap();

    // Equal absolute differences (2.49 and 2.51 against 2.5) keep
    // the first item, as the original's strictly-less tracking does.
    assert_eq!(
        svc.quest_reward_filter(
            "Tie Quest",
            &[
                json!({"item_name": "A", "value": 2.49}),
                json!({"item_name": "B", "value": 2.51}),
            ],
            &[]
        )
        .await
        .unwrap(),
        Some(json!({"suppress_loot_index": 0, "suppress_skill_index": null}))
    );
    // Equal minimum values likewise keep the first item.
    assert_eq!(
        svc.quest_reward_filter(
            "Zed Bounty",
            &[
                json!({"item_name": "A", "value": 0.3}),
                json!({"item_name": "B", "value": 0.3}),
            ],
            &[]
        )
        .await
        .unwrap(),
        Some(json!({"suppress_loot_index": 0, "suppress_skill_index": null}))
    );
}

#[tokio::test]
async fn playlist_matching_requires_completions_within_scope() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let qa = quest_id(&svc.create_quest(&json!({"name": "Alpha"})).await.unwrap());
    let qc = quest_id(&svc.create_quest(&json!({"name": "Gamma"})).await.unwrap());
    svc.create_playlist(&json!({"name": "Solo Run", "quest_ids": [qc]}))
        .await
        .unwrap();
    for (quest, at) in [(qa, 5003.0), (qc, 5004.0)] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES ('s3', ?1, ?2)",
                params![quest, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    // The playlist's immediate set is complete, but the session
    // also completed a quest outside its scope: both subset tests
    // must hold, so the suggestion stays unclean.
    let suggestion = svc.get_session_link_suggestion("s3").await.unwrap();
    assert_eq!(suggestion["reason"], "unclean");
}

#[tokio::test]
async fn cancelling_outside_the_cooldown_window_keeps_completions() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let qa = quest_id(&svc.create_quest(&json!({"name": "Alpha"})).await.unwrap());
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES ('s4', ?1, 1000.0)",
            params![qa],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    // No cooldown configured: never cooling, the completion stays.
    let result = svc.cancel_quest(qa, false).await.unwrap().unwrap();
    assert_eq!(result["last_completed_at"], json!(1000.0));

    // A cooldown that expires exactly at the current instant is no
    // longer cooling (the strict comparison), so the completion
    // stays here too.
    let qe = quest_id(
        &svc.create_quest(&json!({"name": "Edge", "cooldown_hours": 1}))
            .await
            .unwrap(),
    );
    let expires_at = 1772366400.0 - 3600.0;
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES ('s5', ?1, ?2)",
            params![qe, expires_at],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let result = svc.cancel_quest(qe, false).await.unwrap().unwrap();
    assert_eq!(result["last_completed_at"], json!(1772362800.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_session_id_skips_overlay_events() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db, _clock, bus) = service_with_clock(dir.path()).await;
    svc.create_quest(&json!({"name": "Iron Challenge"}))
        .await
        .unwrap();

    // The original's truthiness gate treats an empty session id as
    // no session: the quest starts but no overlay event records.
    bus.publish(&BusEvent::SessionStarted(SessionLifecyclePayload {
        session_id: "".into(),
    }));
    svc.start_quest_from_mission("Iron Challenge")
        .await
        .unwrap();
    let count = count_rows(&db, "SELECT COUNT(*) FROM notable_events").await;
    assert_eq!(count, 0);
}

/// The analytics readers over a seeded economy. The numeric behaviours
/// (engine numeric types preserved: integer zeros from NULL sums, REAL
/// zeros from real columns, active-session exclusion, reward and markup
/// arithmetic) descend from the original implementation; membership is
/// the recorded quest stretch (`session_intervals`), which deliberately
/// superseded the curated link table, so the session sets aggregate by
/// what each session actually ran.
#[tokio::test]
async fn analytics_match_the_original_over_a_seeded_economy() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;

    let qa = quest_id(
        &svc.create_quest(&json!({"name": "Alpha", "reward_ped": 2.5,
                                   "expected_reward_markup_percent": 150.0}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qa, 1000.0).await;
    let qb = quest_id(
        &svc.create_quest(&json!({"name": "Beta", "reward_ped": 5.0,
                                   "reward_is_skill": true}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qb, 1001.0).await;
    let qc = quest_id(&svc.create_quest(&json!({"name": "Gamma"})).await.unwrap());
    pin_ts(&db, "quests", qc, 1002.0).await;
    let qd = quest_id(
        &svc.create_quest(&json!({"name": "Delta", "reward_ped": 1.25}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qd, 1003.0).await;

    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Mixed Run", "items": [
            {"quest_id": qa, "group_type": "immediate"},
            {"quest_id": qb, "group_type": "immediate"},
            {"quest_id": qd, "group_type": "long_horizon"},
        ]}))
        .await
        .unwrap(),
    );
    pin_ts(&db, "quest_playlists", p1, 2000.0).await;
    let p2 = quest_id(
        &svc.create_playlist(&json!({"name": "Bonus Only", "items": [
            {"quest_id": qc, "group_type": "long_horizon"},
        ]}))
        .await
        .unwrap(),
    );
    pin_ts(&db, "quest_playlists", p2, 2001.0).await;

    for (sid, st, en, active, heal, armour) in [
        ("sess-1", 1000.0, Some(4600.0), 0i64, Some(1.5), Some(0.25)),
        ("sess-2", 5000.0, Some(5030.5), 0, None, Some(0.0)),
        ("sess-3", 6000.0, None, 1, Some(2.0), None),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active, heal_cost, armour_cost) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![sid, st, en, active, heal, armour],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    for (kid, sid, mob, ts, enh, loot) in [
        ("k1", "sess-1", "Atrox", 1100.0, 0.5, 12.75),
        ("k2", "sess-1", "Atrox", 1200.0, 0.0, 3.0),
        ("k3", "sess-2", "Snable", 5010.0, 0.1, 0.0),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO kills (id, session_id, mob_name, timestamp, shots_fired, damage_dealt, \
                 damage_taken, critical_hits, cost_ped, enhancer_cost, loot_total_ped) \
                 VALUES (?1, ?2, ?3, ?4, 10, 100.0, 5.0, 1, 0.3, ?5, ?6)",
                params![kid, sid, mob, ts, enh, loot],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    for (kid, tool, shots, cps) in [
        ("k1", "LR-32", 40i64, 0.05),
        ("k1", "Fap-90", 5, 0.02),
        ("k3", "LR-32", 12, 0.05),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, damage_dealt, \
                 critical_hits, cost_per_shot) VALUES (?1, ?2, ?3, 50.0, 0, ?4)",
                params![kid, tool, shots, cps],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    for (sid, skill, ped) in [("sess-1", "Rifle", 0.8), ("sess-2", "Anatomy", 0.2)] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
                 VALUES (?1, 1100.0, ?2, 1.0, ?3)",
                params![sid, skill, ped],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    for (sid, qid, at) in [
        ("sess-1", qa, 1500.0),
        ("sess-1", qb, 1600.0),
        ("sess-1", qd, 1700.0),
        ("sess-2", qa, 5020.0),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES (?1, ?2, ?3)",
                params![sid, qid, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let qn = quest_id(&svc.create_quest(&json!({"name": "Nul"})).await.unwrap());
    pin_ts(&db, "quests", qn, 1004.0).await;
    let qz = quest_id(
        &svc.create_quest(&json!({"name": "Zed", "reward_ped": 0,
                                   "expected_reward_markup_percent": 120.0}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qz, 1005.0).await;
    let qe2 = quest_id(
        &svc.create_quest(&json!({"name": "Echo", "reward_ped": 3.0}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quests", qe2, 1006.0).await;
    for (sid, st, en, active, heal) in [
        ("sess-n", 7000.0, Some(7050.0), 0i64, Some(0.0)),
        ("sess-z", 7100.0, Some(7160.0), 0, Some(0.0)),
        ("sess-act", 8000.0, None, 1, None),
        ("sess-solo", 8100.0, Some(8200.0), 0, Some(0.5)),
    ] {
        let armour = heal.map(|_| 0.0);
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active, heal_cost, armour_cost) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![sid, st, en, active, heal, armour],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    for (sid, qid, at) in [("sess-n", qn, 7040.0), ("sess-z", qz, 7150.0)] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
                 VALUES (?1, ?2, ?3)",
                params![sid, qid, at],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let p3 = quest_id(
        &svc.create_playlist(&json!({"name": "Solo Immediate", "quest_ids": [qc]}))
            .await
            .unwrap(),
    );
    pin_ts(&db, "quest_playlists", p3, 2002.0).await;
    // Membership is the recorded quest stretch: intervals as the
    // lifecycle would have auto-recorded them, one per quest a session
    // actually ran. An active session's stretch is still open (NULL
    // end); sess-solo STARTED Gamma without completing it, which is the
    // real-session-stats-beside-zero-rewards case the curated model
    // used to reach with a completion-less playlist link.
    for (sid, qid, start, end) in [
        ("sess-1", qa, 1400.0, Some(1550.0)),
        ("sess-1", qb, 1450.0, Some(1650.0)),
        ("sess-1", qd, 1500.0, Some(1750.0)),
        ("sess-2", qa, 5005.0, Some(5025.0)),
        ("sess-3", qa, 6010.0, None),
        ("sess-n", qn, 7010.0, Some(7045.0)),
        ("sess-z", qz, 7110.0, Some(7155.0)),
        ("sess-act", qe2, 8010.0, None),
        ("sess-solo", qc, 8110.0, Some(8190.0)),
    ] {
        db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO session_intervals \
                 (session_id, kind, label, ref_id, started_at, ended_at) \
                 VALUES (?1, 'quest', 'Quest', ?2, ?3, ?4)",
                params![sid, qid, start, end],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }

    // Per-quest, name-ordered, over recorded-stretch membership: every
    // quest a session ran shares that session's economics, so Alpha,
    // Beta and Delta all carry sess-1's aggregates (Alpha adds sess-2's;
    // its still-active sess-3 stretch is excluded from the completed
    // stats), Gamma carries the started-but-never-completed sess-solo
    // (real session stats beside integer-zero rewards), and the
    // NULL-reward and zero-reward quests keep their INTEGER zeros on
    // the wire; Echo (recorded only by an active session) is excluded
    // entirely.
    assert_eq!(
        svc.get_quest_analytics().await.unwrap(),
        vec![
            json!({
                "quest_id": qa, "quest_name": "Alpha", "planet": "Calypso",
                "category": null, "reward_ped": 2.5, "reward_is_skill": false,
                "expected_reward_markup_percent": 150.0,
                "total_expected_reward_ped": 7.5,
                "linked_sessions": 2, "total_duration": 3630.5,
                "weapon_cost": 2.7, "heal_cost": 1.5,
                "enhancer_cost": 0.6, "armour_cost": 0.25, "loot_tt": 15.75,
                "skill_tt": 1.0,
            }),
            json!({
                "quest_id": qb, "quest_name": "Beta", "planet": "Calypso",
                "category": null, "reward_ped": 5.0, "reward_is_skill": true,
                "expected_reward_markup_percent": null,
                "total_expected_reward_ped": 5.0,
                "linked_sessions": 1, "total_duration": 3600.0,
                "weapon_cost": 2.1, "heal_cost": 1.5,
                "enhancer_cost": 0.5, "armour_cost": 0.25, "loot_tt": 15.75,
                "skill_tt": 0.8,
            }),
            json!({
                "quest_id": qd, "quest_name": "Delta", "planet": "Calypso",
                "category": null, "reward_ped": 1.25, "reward_is_skill": false,
                "expected_reward_markup_percent": null,
                "total_expected_reward_ped": 1.25,
                "linked_sessions": 1, "total_duration": 3600.0,
                "weapon_cost": 2.1, "heal_cost": 1.5,
                "enhancer_cost": 0.5, "armour_cost": 0.25, "loot_tt": 15.75,
                "skill_tt": 0.8,
            }),
            json!({
                "quest_id": qc, "quest_name": "Gamma", "planet": "Calypso",
                "category": null, "reward_ped": 0, "reward_is_skill": false,
                "expected_reward_markup_percent": null,
                "total_expected_reward_ped": 0,
                "linked_sessions": 1, "total_duration": 100.0,
                "weapon_cost": 0, "heal_cost": 0.5,
                "enhancer_cost": 0, "armour_cost": 0.0, "loot_tt": 0,
                "skill_tt": 0,
            }),
            json!({
                "quest_id": qn, "quest_name": "Nul", "planet": "Calypso",
                "category": null, "reward_ped": 0, "reward_is_skill": false,
                "expected_reward_markup_percent": null,
                "total_expected_reward_ped": 0,
                "linked_sessions": 1, "total_duration": 50.0,
                "weapon_cost": 0, "heal_cost": 0.0,
                "enhancer_cost": 0, "armour_cost": 0.0, "loot_tt": 0,
                "skill_tt": 0,
            }),
            json!({
                "quest_id": qz, "quest_name": "Zed", "planet": "Calypso",
                "category": null, "reward_ped": 0, "reward_is_skill": false,
                // The zero reward normalised its markup away at
                // creation, exactly as the original stores it.
                "expected_reward_markup_percent": null,
                "total_expected_reward_ped": 0,
                "linked_sessions": 1, "total_duration": 60.0,
                "weapon_cost": 0, "heal_cost": 0.0,
                "enhancer_cost": 0, "armour_cost": 0.0, "loot_tt": 0,
                "skill_tt": 0,
            }),
        ]
    );

    // An immediate-only playlist with a linked session that
    // completed nothing in scope: real session stats beside
    // integer-zero reward sums (the empty long-horizon set
    // short-circuits without touching SQL).
    assert_eq!(
        svc.get_playlist_analytics(p3).await.unwrap().unwrap(),
        json!({
            "playlist_id": p3, "playlist_name": "Solo Immediate", "quest_count": 1,
            "long_horizon_quest_count": 0,
            "total_reward_ped": 0, "total_immediate_reward_ped": 0,
            "total_bonus_reward_ped": 0, "total_skill_reward_ped": 0,
            "total_immediate_skill_reward_ped": 0, "total_bonus_skill_reward_ped": 0,
            "total_expected_reward_ped": 0, "total_expected_immediate_reward_ped": 0,
            "total_expected_bonus_reward_ped": 0,
            "matched_sessions": 1, "linked_sessions": 1, "total_duration": 100.0,
            "weapon_cost": 0, "heal_cost": 0.5, "enhancer_cost": 0,
            "armour_cost": 0.0, "loot_tt": 0, "skill_tt": 0,
        })
    );

    let p1_stats = svc.get_playlist_analytics(p1).await.unwrap().unwrap();
    assert_eq!(
        p1_stats,
        json!({
            "playlist_id": p1, "playlist_name": "Mixed Run", "quest_count": 2,
            "long_horizon_quest_count": 1,
            "total_reward_ped": 11.25, "total_immediate_reward_ped": 10.0,
            "total_bonus_reward_ped": 1.25, "total_skill_reward_ped": 5.0,
            "total_immediate_skill_reward_ped": 5.0, "total_bonus_skill_reward_ped": 0,
            "total_expected_reward_ped": 13.75,
            "total_expected_immediate_reward_ped": 12.5,
            "total_expected_bonus_reward_ped": 1.25,
            "matched_sessions": 2, "linked_sessions": 2, "total_duration": 3630.5,
            "weapon_cost": 2.7, "heal_cost": 1.5, "enhancer_cost": 0.6,
            "armour_cost": 0.25, "loot_tt": 15.75, "skill_tt": 1.0,
        })
    );

    // An empty immediate set is the zeroed early-return shape
    // (which carries matched_sessions but no linked_sessions).
    let p2_stats = svc.get_playlist_analytics(p2).await.unwrap().unwrap();
    assert_eq!(
        p2_stats,
        json!({
            "playlist_id": p2, "playlist_name": "Bonus Only", "quest_count": 0,
            "long_horizon_quest_count": 1, "matched_sessions": 0,
            "total_reward_ped": 0, "total_immediate_reward_ped": 0,
            "total_bonus_reward_ped": 0, "total_skill_reward_ped": 0,
            "total_immediate_skill_reward_ped": 0, "total_bonus_skill_reward_ped": 0,
            "total_expected_reward_ped": 0, "total_expected_immediate_reward_ped": 0,
            "total_expected_bonus_reward_ped": 0, "total_duration": 0,
            "weapon_cost": 0, "heal_cost": 0, "enhancer_cost": 0,
            "armour_cost": 0, "loot_tt": 0, "skill_tt": 0,
        })
    );

    let p3_stats = svc.get_playlist_analytics(p3).await.unwrap().unwrap();
    assert_eq!(
        svc.get_all_playlist_analytics().await.unwrap(),
        vec![p1_stats, p2_stats, p3_stats]
    );
    assert_eq!(svc.get_playlist_analytics(9999).await.unwrap(), None);
}

#[test]
fn truthiness_matches_python_bool() {
    // The expectation table is Python's bool() over each shape.
    assert!(!json_truthy(None));
    assert!(!json_truthy(Some(&json!(null))));
    assert!(!json_truthy(Some(&json!(false))));
    assert!(json_truthy(Some(&json!(true))));
    assert!(!json_truthy(Some(&json!(0))));
    assert!(!json_truthy(Some(&json!(0.0))));
    assert!(json_truthy(Some(&json!(2))));
    assert!(!json_truthy(Some(&json!(""))));
    assert!(json_truthy(Some(&json!("no"))));
    assert!(!json_truthy(Some(&json!([]))));
    assert!(json_truthy(Some(&json!(["x"]))));
    assert!(!json_truthy(Some(&json!({}))));
    assert!(json_truthy(Some(&json!({"k": 1}))));
}

/// A completion reports the quest to the interval layer so a declared
/// stretch of it closes at the completion moment; a start reports
/// NOTHING, because the mission log only witnesses pickup (bulk pickup
/// separates it from the play that advances the quest), and which
/// stretch of play is toward a quest is the user's own declaration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_completion_reports_to_the_interval_layer() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;

    let reported = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = reported.clone();
    svc.set_stretch_closer(Arc::new(move |quest_id| {
        sink.lock().unwrap().push(quest_id);
        Box::pin(async {})
    }));

    let quest = quest_id(
        &svc.create_quest(&json!({"name": "Daily: Carabok"}))
            .await
            .unwrap(),
    );

    svc.start_quest(quest).await.unwrap();
    assert!(
        reported.lock().unwrap().is_empty(),
        "a start opens no stretch: the stretch is the user's declaration"
    );

    svc.complete_quest(quest).await.unwrap();
    assert_eq!(*reported.lock().unwrap(), vec![quest]);
}

// ── Signal-completed quests ─────────────────────────────────────────

/// A signal-probe loot line, as the watcher would hand it over.
fn marker(item_name: &str, quantity: i64) -> SignalLoot {
    SignalLoot {
        item_name: item_name.to_string(),
        quantity,
    }
}

/// The signal path end to end at the service: an in-progress signal
/// quest completes when its item arrives (case-insensitively, trimmed),
/// records the completion, reports the stretch close, and clears the
/// in-progress state so the run is over until the next declaration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signal_loot_tick_completes_the_in_progress_signal_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;

    let closed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = closed.clone();
    svc.set_stretch_closer(Arc::new(move |quest_id| {
        sink.lock().unwrap().push(quest_id);
        Box::pin(async {})
    }));

    let boss = quest_id(
        &svc.create_quest(&json!({
            "name": "Hyperion Boss 1",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );
    svc.start_quest(boss).await.unwrap();

    // A mission-less loot tick with the marker (case and padding off on
    // purpose) completes the run; unrelated items complete nothing.
    svc.signal_loot_check(&[
        marker("Shrapnel", 4639),
        marker(" hyperion daily voucher ", 1),
        marker("Hyperium", 2),
    ])
    .await
    .unwrap();

    let quest = svc.get_quest(boss).await.unwrap().unwrap();
    assert!(quest["started_at"].is_null(), "the run ended");
    assert!(!quest["last_completed_at"].is_null(), "completion recorded");
    assert_eq!(*closed.lock().unwrap(), vec![boss]);
    let source = db
        .with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT reward_source, reward_ped FROM session_quest_completions \
                 WHERE quest_id = ?",
                params![boss],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(source, ("tracked_loot".to_string(), None));
}

/// A signal quest that is NOT in progress ignores its marker: an
/// undeclared run stays unrecorded rather than being invented from
/// loot (the same honesty rule the Activities control follows).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signal_without_a_declared_run_completes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;

    let boss = quest_id(
        &svc.create_quest(&json!({
            "name": "Hyperion Boss 1",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );

    svc.signal_loot_check(&[marker("Hyperion Daily Voucher", 1)])
        .await
        .unwrap();

    let quest = svc.get_quest(boss).await.unwrap().unwrap();
    assert!(
        quest["last_completed_at"].is_null(),
        "no declared run, no completion"
    );
}

/// One marker completes one run: two in-progress quests sharing a
/// signal item draw on the tick's occurrence budget oldest-first, so a
/// single marker cannot complete both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_marker_completes_one_of_two_quests_sharing_the_signal() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;

    let first = quest_id(
        &svc.create_quest(&json!({
            "name": "Boss A",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );
    let second = quest_id(
        &svc.create_quest(&json!({
            "name": "Boss B",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );
    svc.start_quest(first).await.unwrap();
    svc.start_quest(second).await.unwrap();
    // Pin distinct start instants so "oldest first" is deterministic.
    db.with_writer(move |conn| {
        conn.execute(
            "UPDATE quests SET started_at = 100.0 WHERE id = ?",
            params![first],
        )?;
        conn.execute(
            "UPDATE quests SET started_at = 200.0 WHERE id = ?",
            params![second],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    svc.signal_loot_check(&[marker("Hyperion Daily Voucher", 1)])
        .await
        .unwrap();

    let first_quest = svc.get_quest(first).await.unwrap().unwrap();
    let second_quest = svc.get_quest(second).await.unwrap().unwrap();
    assert!(
        !first_quest["last_completed_at"].is_null(),
        "the oldest-started run completed"
    );
    assert!(
        second_quest["last_completed_at"].is_null() && !second_quest["started_at"].is_null(),
        "the newer run keeps going"
    );
}

/// A stacked marker line pays for that many runs: the budget counts
/// units, not lines, so one loot line carrying quantity 2 completes
/// two in-progress quests sharing the signal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stacked_marker_line_pays_for_that_many_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;

    let first = quest_id(
        &svc.create_quest(&json!({
            "name": "Boss A",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );
    let second = quest_id(
        &svc.create_quest(&json!({
            "name": "Boss B",
            "signal_loot_item": "Hyperion Daily Voucher",
        }))
        .await
        .unwrap(),
    );
    svc.start_quest(first).await.unwrap();
    svc.start_quest(second).await.unwrap();
    db.with_writer(move |conn| {
        conn.execute(
            "UPDATE quests SET started_at = 100.0 WHERE id = ?",
            params![first],
        )?;
        conn.execute(
            "UPDATE quests SET started_at = 200.0 WHERE id = ?",
            params![second],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    svc.signal_loot_check(&[marker("Hyperion Daily Voucher", 2)])
        .await
        .unwrap();

    for quest in [first, second] {
        let row = svc.get_quest(quest).await.unwrap().unwrap();
        assert!(
            !row["last_completed_at"].is_null(),
            "both stacked-paid runs completed"
        );
    }
}

/// The colon-variant discipline: a variant-family quest is matched
/// only by a mission line carrying the same family and a variant that
/// clears the fuzzy bar on its own, so the bare umbrella line a
/// pickup emits (which crosses the whole-string fuzzy bar) and a
/// sibling variant's line can never cross-match it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn variant_family_quests_refuse_umbrella_and_sibling_lines() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let variant = quest_id(
        &svc.create_quest(&json!({"name": "ARIS - Daily Hunting 3: Verderons"}))
            .await
            .unwrap(),
    );
    svc.start_quest(variant).await.unwrap();

    // The umbrella chooser line is a DIFFERENT mission: whole-string
    // scoring puts it exactly at the 0.8 bar, but the structural rule
    // refuses a family-only line against a variant quest.
    assert!(svc
        .match_quest_by_mission_name("ARIS - Daily Hunting 3", true)
        .await
        .unwrap()
        .is_none());
    // A sibling variant shares the long family prefix but is its own
    // mission: variants are compared on their own, and "Fieroids"
    // against "Verderons" is nowhere near the bar.
    assert!(svc
        .match_quest_by_mission_name("ARIS - Daily Hunting 3: Fieroids", true)
        .await
        .unwrap()
        .is_none());
    // The quest's own line still matches through the decorations...
    let matched = svc
        .match_quest_by_mission_name("ARIS - Daily Hunting 3: VERDERONS (Repeatable)", true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(matched["id"], json!(variant));
    // ...and a near-identical variant spelling fuzzy-matches within
    // the family.
    let matched = svc
        .match_quest_by_mission_name("ARIS - Daily Hunting 3: Verderon", true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(matched["id"], json!(variant));
}

/// The completion gate composes with the discipline: an umbrella line
/// through the completion check completes nothing even while the
/// variant quest is in progress.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_umbrella_line_never_completes_a_variant_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let variant = quest_id(
        &svc.create_quest(&json!({"name": "ARIS - Daily Hunting 1: Faint Fieroids"}))
            .await
            .unwrap(),
    );
    svc.start_quest(variant).await.unwrap();

    svc.mission_complete_check(&[MissionCompletion {
        mission_name: "ARIS - Daily Hunting 1".to_string(),
        loot_items: vec![],
        skill_gains: vec![],
    }])
    .await
    .unwrap();

    let quest = svc.get_quest(variant).await.unwrap().unwrap();
    assert!(
        quest["last_completed_at"].is_null() && !quest["started_at"].is_null(),
        "the variant keeps running through its umbrella's line"
    );
}

/// The signal/reward exclusion: a signal quest cannot carry a fixed
/// positive reward (its reward is the tracked loot itself), on create
/// and on the merged update picture alike. A blank signal normalises
/// to none and lifts the exclusion.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_signal_quest_refuses_a_fixed_reward() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;

    let refused = svc
        .create_quest(&json!({
            "name": "Boss",
            "signal_loot_item": "Hyperion Daily Voucher",
            "reward_ped": 2.0,
        }))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("signal-completed"));

    // Blank signal is no signal: the reward is fine.
    let plain = quest_id(
        &svc.create_quest(&json!({
            "name": "Daily",
            "signal_loot_item": "  ",
            "reward_ped": 2.0,
        }))
        .await
        .unwrap(),
    );

    // Adding a signal to a rewarded quest is refused over the merge...
    let refused = svc
        .update_quest(plain, &json!({"signal_loot_item": "Voucher"}))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("signal-completed"));

    // ...and adding a reward to a signal quest likewise.
    let boss = quest_id(
        &svc.create_quest(&json!({
            "name": "Boss",
            "signal_loot_item": "Voucher",
        }))
        .await
        .unwrap(),
    );
    let refused = svc
        .update_quest(boss, &json!({"reward_ped": 2.0}))
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("signal-completed"));

    // Clearing the signal lifts the exclusion in the same patch.
    let updated = svc
        .update_quest(boss, &json!({"signal_loot_item": null, "reward_ped": 2.0}))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["reward_ped"], json!(2.0));
    assert!(updated["signal_loot_item"].is_null());
}

// ── Quest families: shared, anchor-aware cooldowns ──────────────────

#[tokio::test]
async fn family_crud_round_trips_and_validates() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;

    // Defaults: planet Calypso, pickup anchor, no gate, no members.
    let family = svc
        .create_family(&json!({"name": "  Daily Hunting 1  "}))
        .await
        .unwrap();
    assert_eq!(family["name"], json!("Daily Hunting 1"), "name trims");
    assert_eq!(family["planet"], json!("Calypso"));
    assert_eq!(family["cooldown_anchor"], json!("pickup"));
    assert_eq!(family["cooldown_hours"], Value::Null);
    assert_eq!(family["member_count"], json!(0));
    assert_eq!(family["cooldown_expires_at"], Value::Null);

    // Refusals: blank name, bad anchor, non-positive hours.
    assert!(svc.create_family(&json!({"name": "  "})).await.is_err());
    assert!(svc
        .create_family(&json!({"name": "X", "cooldown_anchor": "sometimes"}))
        .await
        .is_err());
    assert!(svc
        .create_family(&json!({"name": "X", "cooldown_hours": 0}))
        .await
        .is_err());

    // Update binds present keys; absent keys keep.
    let fid = family["id"].as_i64().unwrap();
    let updated = svc
        .update_family(
            fid,
            &json!({"cooldown_hours": 20.0, "cooldown_anchor": "completion"}),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated["name"], json!("Daily Hunting 1"));
    assert_eq!(updated["cooldown_hours"], json!(20.0));
    assert_eq!(updated["cooldown_anchor"], json!("completion"));

    // Delete soft-deletes off the active list.
    assert!(svc.delete_family(fid).await.unwrap());
    assert!(!svc.delete_family(fid).await.unwrap(), "already inactive");
    assert_eq!(svc.get_families(true).await.unwrap().len(), 0);
    assert_eq!(svc.get_families(false).await.unwrap().len(), 1);
}

#[tokio::test]
async fn creating_a_family_sweeps_matching_unattached_variants() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;

    let a = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Weak Mortirex"}))
            .await
            .unwrap(),
    );
    let b = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Derilect Destroyer"}))
            .await
            .unwrap(),
    );
    let unrelated = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    // A variant already claimed by another family is never stolen.
    let other = svc
        .create_family(&json!({"name": "Daily Hunting 2"}))
        .await
        .unwrap();
    let claimed = quest_id(
        &svc.create_quest(&json!({
            "name": "Daily Hunting 1: Poached",
            "family_id": other["id"].as_i64().unwrap(),
        }))
        .await
        .unwrap(),
    );

    let family = svc
        .create_family(&json!({"name": "daily hunting 1", "cooldown_hours": 20.0}))
        .await
        .unwrap();
    assert_eq!(family["member_count"], json!(2), "case-insensitive sweep");
    let fid = family["id"].as_i64().unwrap();
    for (id, expect) in [
        (a, Some(fid)),
        (b, Some(fid)),
        (unrelated, None),
        (claimed, Some(other["id"].as_i64().unwrap())),
    ] {
        let quest = svc.get_quest(id).await.unwrap().unwrap();
        assert_eq!(quest["family_id"], json!(expect), "quest {id}");
    }
}

#[tokio::test]
async fn creating_a_quest_auto_attaches_by_name_only_when_family_id_is_absent() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let family = svc
        .create_family(&json!({"name": "Daily Hunting 1"}))
        .await
        .unwrap();
    let fid = family["id"].as_i64().unwrap();

    // Absent key: the colon-split name attaches.
    let auto = svc
        .create_quest(&json!({"name": "Daily Hunting 1: Weak Mortirex"}))
        .await
        .unwrap();
    assert_eq!(auto["family_id"], json!(fid));
    assert_eq!(auto["family_name"], json!("Daily Hunting 1"));

    // Present-null key: explicitly standalone, no auto-attach.
    let standalone = svc
        .create_quest(&json!({"name": "Daily Hunting 1: Loner", "family_id": null}))
        .await
        .unwrap();
    assert_eq!(standalone["family_id"], Value::Null);

    // A dangling id refuses; an update never re-attaches implicitly.
    assert!(svc
        .create_quest(&json!({"name": "X", "family_id": 999}))
        .await
        .is_err());
    let renamed = svc
        .update_quest(
            quest_id(&standalone),
            &json!({"name": "Daily Hunting 1: Renamed Loner"}),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(renamed["family_id"], Value::Null, "detached stays detached");
}

#[tokio::test]
async fn family_cooldown_derives_from_member_instants_per_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db, clock, _bus) = service_with_clock(dir.path()).await;
    let family = svc
        .create_family(&json!({"name": "Daily Hunting 1", "cooldown_hours": 20.0}))
        .await
        .unwrap();
    let fid = family["id"].as_i64().unwrap();
    let a = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Weak Mortirex"}))
            .await
            .unwrap(),
    );
    let b = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Derilect Destroyer"}))
            .await
            .unwrap(),
    );

    // Pickup anchor: starting ANY member opens the family window on
    // EVERY member row, from the latest member start.
    svc.start_quest(a).await.unwrap();
    let a_row = svc.get_quest(a).await.unwrap().unwrap();
    let start_epoch = a_row["last_started_at"].as_f64().unwrap();
    let expected = crate::time::to_iso_utc(start_epoch + 20.0 * 3600.0);
    for id in [a, b] {
        let row = svc.get_quest(id).await.unwrap().unwrap();
        assert_eq!(row["family_id"], json!(fid));
        assert_eq!(row["family_cooldown_anchor"], json!("pickup"));
        assert_eq!(row["family_cooldown_expires_at"], json!(expected.clone()));
        assert_eq!(row["cooldown_expires_at"], Value::Null, "no own gate");
    }

    // Completing the run later leaves the pickup-anchored window where
    // the start put it; flipping the family to completion re-anchors
    // the same window on the completion instant.
    clock.advance(3600.0).unwrap();
    svc.complete_quest(a).await.unwrap();
    let row = svc.get_quest(b).await.unwrap().unwrap();
    assert_eq!(row["family_cooldown_expires_at"], json!(expected.clone()));
    let completion_epoch = start_epoch + 3600.0;
    svc.update_family(fid, &json!({"cooldown_anchor": "completion"}))
        .await
        .unwrap()
        .unwrap();
    let row = svc.get_quest(b).await.unwrap().unwrap();
    assert_eq!(
        row["family_cooldown_expires_at"],
        json!(crate::time::to_iso_utc(completion_epoch + 20.0 * 3600.0))
    );
}

#[tokio::test]
async fn start_stamps_a_durable_last_started_at_surviving_completion_and_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db, clock, _bus) = service_with_clock(dir.path()).await;
    let q = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );

    svc.start_quest(q).await.unwrap();
    let first_start = svc.get_quest(q).await.unwrap().unwrap()["last_started_at"]
        .as_f64()
        .unwrap();
    svc.complete_quest(q).await.unwrap();
    let row = svc.get_quest(q).await.unwrap().unwrap();
    assert_eq!(row["started_at"], Value::Null);
    assert_eq!(
        row["last_started_at"],
        json!(first_start),
        "survives completion"
    );

    clock.advance(60.0).unwrap();
    svc.start_quest(q).await.unwrap();
    svc.cancel_quest(q, false).await.unwrap();
    let row = svc.get_quest(q).await.unwrap().unwrap();
    assert_eq!(row["started_at"], Value::Null);
    assert_eq!(
        row["last_started_at"],
        json!(first_start + 60.0),
        "an abandon keeps the durable stamp (the giver's timer keeps running)"
    );
}

#[tokio::test]
async fn pickup_anchored_own_cooldown_cools_from_start_and_double_cancel_resets() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let q = quest_id(
        &svc.create_quest(&json!({
            "name": "Standing Contract",
            "cooldown_hours": 20.0,
            "cooldown_anchor": "pickup",
        }))
        .await
        .unwrap(),
    );

    svc.start_quest(q).await.unwrap();
    let row = svc.get_quest(q).await.unwrap().unwrap();
    let start_epoch = row["last_started_at"].as_f64().unwrap();
    assert_eq!(
        row["cooldown_expires_at"],
        json!(crate::time::to_iso_utc(start_epoch + 20.0 * 3600.0)),
        "a pickup-anchored own window opens at the start itself"
    );

    // First cancel un-starts and honours the timer; the second cancel is
    // the explicit reset, clearing the durable stamp (the pickup-anchor
    // parallel of deleting the latest completion).
    svc.cancel_quest(q, false).await.unwrap();
    let row = svc.get_quest(q).await.unwrap().unwrap();
    assert_eq!(row["started_at"], Value::Null);
    assert!(!row["cooldown_expires_at"].is_null(), "still cooling");
    svc.cancel_quest(q, false).await.unwrap();
    let row = svc.get_quest(q).await.unwrap().unwrap();
    assert_eq!(row["last_started_at"], Value::Null);
    assert_eq!(row["cooldown_expires_at"], Value::Null, "reset to ready");
}

#[tokio::test]
async fn a_member_cancel_never_clears_the_family_window() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let family = svc
        .create_family(&json!({"name": "Daily Hunting 1", "cooldown_hours": 20.0}))
        .await
        .unwrap();
    let a = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Weak Mortirex"}))
            .await
            .unwrap(),
    );

    svc.start_quest(a).await.unwrap();
    // Two cancels: the first un-starts; the second finds the member
    // neither started nor own-cooling (its own anchor is 'completion'
    // and it has no own gate), so it returns as-is. FAMILY cooling
    // alone never opens the reset branch, so the family-wide stamp
    // survives. (A member with its OWN pickup gate can still disavow
    // its start via the reset, and the family window derived from that
    // fact moves with it; that is the correction working, not a leak.)
    svc.cancel_quest(a, false).await.unwrap();
    svc.cancel_quest(a, false).await.unwrap();
    let row = svc.get_quest(a).await.unwrap().unwrap();
    assert!(!row["last_started_at"].is_null());
    assert!(
        !row["family_cooldown_expires_at"].is_null(),
        "family still cooling"
    );
    let families = svc.get_families(true).await.unwrap();
    assert_eq!(families[0]["id"], family["id"]);
    assert!(!families[0]["cooldown_expires_at"].is_null());
}

#[tokio::test]
async fn an_unknown_variant_of_a_known_family_auto_creates_and_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, db) = service(dir.path()).await;
    let family = svc
        .create_family(
            &json!({"name": "ARIS - Daily Hunting 1", "planet": "ARIS", "cooldown_hours": 20.0}),
        )
        .await
        .unwrap();

    svc.start_quest_from_mission("ARIS - Daily Hunting 1: Weak Mortirex (Repeatable)")
        .await
        .unwrap();
    let quests = svc.get_quests(true).await.unwrap();
    assert_eq!(quests.len(), 1);
    let created = &quests[0];
    assert_eq!(
        created["name"],
        json!("ARIS - Daily Hunting 1: Weak Mortirex"),
        "named as the line reads, repeatable suffix stripped"
    );
    assert_eq!(
        created["planet"],
        json!("ARIS"),
        "inherits the family planet"
    );
    assert_eq!(created["family_id"], family["id"]);
    assert!(
        json_truthy(created.get("started_at")),
        "starts in the same motion"
    );

    // The second encounter is an exact match: no duplicate row.
    svc.start_quest_from_mission("ARIS - Daily Hunting 1: Weak Mortirex")
        .await
        .unwrap();
    assert_eq!(svc.get_quests(true).await.unwrap().len(), 1);

    // A line matching no quest and no family stays ignored, and the
    // bare umbrella line (no variant part) never creates a quest.
    svc.start_quest_from_mission("Some Other Mission")
        .await
        .unwrap();
    svc.start_quest_from_mission("ARIS - Daily Hunting 1: ")
        .await
        .unwrap();
    assert_eq!(svc.get_quests(true).await.unwrap().len(), 1);
    let quest_rows = count_rows(&db, "SELECT COUNT(*) FROM quests").await;
    assert_eq!(quest_rows, 1);
}

#[tokio::test]
async fn updating_a_soft_deleted_family_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _db) = service(dir.path()).await;
    let family = svc
        .create_family(&json!({"name": "Daily Hunting 1", "cooldown_hours": 20.0}))
        .await
        .unwrap();
    let fid = family["id"].as_i64().unwrap();
    let member = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunting 1: Weak Mortirex"}))
            .await
            .unwrap(),
    );

    assert!(svc.delete_family(fid).await.unwrap());
    // A soft-deleted family reads as absent: no mutation, and above all
    // no rename sweep re-pointing active quests at a dead family (the
    // delete detached them precisely to prevent that).
    assert_eq!(
        svc.update_family(fid, &json!({"name": "Daily Hunting 1"}))
            .await
            .unwrap(),
        None
    );
    let row = svc.get_quest(member).await.unwrap().unwrap();
    assert_eq!(row["family_id"], Value::Null, "the detach stands");
}
