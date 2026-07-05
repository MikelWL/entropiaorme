// Expected values in these tests are the original implementation's
// outputs, computed by running the original Python implementation
// over byte-identical payloads and database seeds (created_at and
// updated_at pinned by direct UPDATE on both sides, since the schema
// stamps them from the wall clock).

use std::sync::Arc;

use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use tokio::runtime::Handle;

use crate::bus_events::{
    BusEvent, MissionReceivedPayload, MissionReceivedTag, SessionLifecyclePayload,
};
use crate::db::Db;

use super::lifecycle::{delete_latest_quest_claim, delete_latest_quest_reward_entry};
use super::payload::json_truthy;
use super::QuestService;
use crate::ped::Ped;

type ServiceRig = (
    Arc<QuestService>,
    SqlitePool,
    Arc<crate::clock::MockClock>,
    Arc<crate::event_bus::EventBus>,
);

async fn service_with_clock(dir: &std::path::Path) -> ServiceRig {
    let db = Db::open(&dir.join("entropia_orme.db")).await.unwrap();
    // Tests drive direct SQL through the writer pool (single connection),
    // reproducing the original pool-of-one semantics.
    let pool = db.write().clone();
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
        Db::from_pool(pool.clone()),
        clock.clone(),
        Handle::current(),
        Arc::new(move || {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            format!("fixed-{n:04}")
        }),
    );
    (svc, pool, clock, bus)
}

async fn service(dir: &std::path::Path) -> (Arc<QuestService>, SqlitePool) {
    let (svc, pool, _clock, _bus) = service_with_clock(dir).await;
    (svc, pool)
}

async fn pin_ts(pool: &SqlitePool, table: &str, id: i64, ts: f64) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET created_at = ?, updated_at = ? WHERE id = ?"
    )))
    .bind(ts)
    .bind(ts)
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
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
    let (_svc, pool) = service(dir.path()).await;

    // A historical skill-reward claim and a liquid-reward ledger
    // entry, both two days behind the heal watermark.
    let claimed_at = 999_700_000.0; // inside 2001-09-05 UTC
    sqlx::query(
        "INSERT INTO quest_claims (quest_id, quest_name, ped_value, claimed_at) \
         VALUES (7, 'Iron Atrox', 2.5, ?)",
    )
    .bind(claimed_at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ledger_entries (id, date, type, description, amount, tag) \
         VALUES ('q1', '2001-09-05', 'markup', 'Quest: Daily Feffoid', 4.0, 'quest_reward')",
    )
    .execute(&pool)
    .await
    .unwrap();
    crate::daily_rollup::heal_rollups(&pool, claimed_at + 3.0 * 86_400.0)
        .await
        .unwrap();
    let day = crate::daily_rollup::epoch_day(claimed_at);
    let quest_pes: Option<f64> =
        sqlx::query_scalar("SELECT quest_pes FROM daily_rollups WHERE day = ?")
            .bind(&day)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(quest_pes, Some(2.5));

    // Both undo paths reland their day inside the caller's commit
    // semantics.
    let mut conn = pool.acquire().await.unwrap();
    assert!(delete_latest_quest_claim(&mut conn, 7).await.unwrap());
    assert!(
        delete_latest_quest_reward_entry(&mut conn, "Daily Feffoid", Ped(4.0))
            .await
            .unwrap()
    );
    drop(conn);
    let quest_pes: Option<f64> =
        sqlx::query_scalar("SELECT quest_pes FROM daily_rollups WHERE day = ?")
            .bind(&day)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(quest_pes, None, "the undone claim left the day");
    let ledger_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM daily_ledger_rollups WHERE day = ? AND tag = 'quest_reward'",
    )
    .bind(&day)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ledger_rows, 0, "the undone reward left the day");
}

#[tokio::test]
async fn creates_apply_defaults_normalisation_and_mob_rules() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, pool) = service(dir.path()).await;

    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", q1, 1000.0).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());
    pin_ts(&pool, "quests", q2, 1001.0).await;
    let q3 = quest_id(
        &svc.create_quest(&json!({
            "name": "Skill Run", "reward_ped": 5.0, "reward_is_skill": true,
            "expected_reward_markup_percent": 120.0,
        }))
        .await
        .unwrap(),
    );
    pin_ts(&pool, "quests", q3, 1002.0).await;
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
            "reward_description": null, "updated_at": 1000.0, "last_completed_at": null,
            "cooldown_expires_at": null, "mobs": [], "playlist_ids": [],
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
    let (svc, pool) = service(dir.path()).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());

    sqlx::query(
        "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
         VALUES ('sess-1', ?, 1772366400.0)",
    )
    .bind(q2)
    .execute(&pool)
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
    let (svc, _pool) = service(dir.path()).await;
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
    let (svc, pool) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge"}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", q1, 1000.0).await;
    let q2 = quest_id(&svc.create_quest(&full_quest_payload()).await.unwrap());
    pin_ts(&pool, "quests", q2, 1001.0).await;
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
    let (svc, pool) = service(dir.path()).await;
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
    pin_ts(&pool, "quest_playlists", p1, 2000.0).await;
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
    let (svc, _pool) = service(dir.path()).await;
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
    let (svc, _pool) = service(dir.path()).await;
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
async fn soft_deleting_a_quest_keeps_its_mob_rows() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, pool) = service(dir.path()).await;
    let q1 = quest_id(
        &svc.create_quest(&json!({"name": "Iron Challenge", "mobs": ["Atrox"]}))
            .await
            .unwrap(),
    );

    assert!(svc.delete_quest(q1).await.unwrap());
    // The soft delete detaches playlist items only; the mob rows
    // stay (the autocomplete reader filters by active quests, so
    // they vanish from that surface without being destroyed).
    let mobs: i64 = sqlx::query("SELECT COUNT(*) FROM quest_mobs WHERE quest_id = ?")
        .bind(q1)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(mobs, 1);
    assert_eq!(svc.get_all_mob_names().await.unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn mob_autocomplete_lists_active_quest_mobs_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _pool) = service(dir.path()).await;
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
    let (svc, pool) = service(dir.path()).await;
    let mut payload = full_quest_payload();
    payload["cooldown_hours"] = json!(0);
    let q = quest_id(&svc.create_quest(&payload).await.unwrap());
    sqlx::query(
        "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
         VALUES ('sess-1', ?, 1772366400.0)",
    )
    .bind(q)
    .execute(&pool)
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
    let (svc, _pool) = service(dir.path()).await;
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
    let (svc, pool, clock, bus) = service_with_clock(dir.path()).await;

    let qa = quest_id(
        &svc.create_quest(
            &json!({"name": "Iron Challenge", "reward_ped": 2.5, "cooldown_hours": 24}),
        )
        .await
        .unwrap(),
    );
    pin_ts(&pool, "quests", qa, 1000.0).await;
    let qb = quest_id(
        &svc.create_quest(&json!({"name": "Daily Hunt: Atrox", "reward_ped": 5.0,
                                   "reward_is_skill": true, "cooldown_hours": 1}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qb, 1001.0).await;
    let qc = quest_id(
        &svc.create_quest(&json!({"name": "G\u{e9}ologist Survey"}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qc, 1002.0).await;
    let qe = quest_id(
        &svc.create_quest(&json!({"name": "Zero Bounty", "reward_ped": 0}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qe, 1003.0).await;

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
        let pool = pool.clone();
        async move {
            sqlx::query(sql)
                .fetch_all(&pool)
                .await
                .unwrap()
                .iter()
                .map(|row| {
                    json!([
                        row.get::<String, _>(0),
                        row.get::<String, _>(1),
                        row.get::<String, _>(2),
                        row.get::<f64, _>(3),
                        row.get::<String, _>(4),
                    ])
                })
                .collect::<Vec<_>>()
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
    let completions = |pool: SqlitePool| async move {
        sqlx::query(
            "SELECT session_id, quest_id, completed_at FROM session_quest_completions ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            json!([
                row.get::<String, _>(0),
                row.get::<i64, _>(1),
                row.get::<f64, _>(2)
            ])
        })
        .collect::<Vec<_>>()
    };
    assert_eq!(
        completions(pool.clone()).await,
        vec![json!(["manual-fixed-0002", qa, 1772366460.0])]
    );

    // The bus feeds the active session; a session-scoped skill
    // completion writes a claim, and a repeat in the same session
    // dedupes the completion while duplicating the claim.
    bus.publish(&BusEvent::SessionStarted(SessionLifecyclePayload {
        session_id: "sess-abc".into(),
    }));
    clock.advance(60.0).unwrap();
    svc.complete_quest(qb).await.unwrap().unwrap();
    clock.advance(60.0).unwrap();
    svc.complete_quest(qb).await.unwrap().unwrap();
    let claims = sqlx::query(
        "SELECT quest_id, quest_name, ped_value, claimed_at FROM quest_claims ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .iter()
    .map(|row| {
        json!([
            row.get::<i64, _>(0),
            row.get::<String, _>(1),
            row.get::<f64, _>(2),
            row.get::<f64, _>(3)
        ])
    })
    .collect::<Vec<_>>();
    assert_eq!(
        claims,
        vec![
            json!([qb, "Daily Hunt: Atrox", 5.0, 1772366520.0]),
            json!([qb, "Daily Hunt: Atrox", 5.0, 1772366580.0]),
        ]
    );
    assert_eq!(
        completions(pool.clone()).await,
        vec![
            json!(["manual-fixed-0002", qa, 1772366460.0]),
            json!(["sess-abc", qb, 1772366520.0]),
        ]
    );

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
    let claim_count: i64 = sqlx::query("SELECT COUNT(*) FROM quest_claims")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(claim_count, 1, "the newest claim is undone");
    svc.cancel_quest(qa, true).await.unwrap().unwrap();
    let ledger_count: i64 = sqlx::query("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
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
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES (?, ?, ?)",
        )
        .bind(session)
        .bind(quest)
        .bind(at)
        .execute(&pool)
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
    svc.accept_session_link_suggestion("sess-two")
        .await
        .unwrap();
    sugg(
        svc.get_session_link_suggestion("sess-two").await.unwrap(),
        "none",
        "already_linked",
    );
    svc.decline_session_link("sess-decl").await.unwrap();
    sugg(
        svc.get_session_link_suggestion("sess-decl").await.unwrap(),
        "none",
        "declined",
    );
    let error = svc
        .accept_session_link_suggestion("sess-none")
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "No linkable suggestion for session sess-none: no_completions"
    );
    svc.accept_session_link_suggestion("sess-one")
        .await
        .unwrap();
    for (session, quest, at) in [("sess-three", qa, 5003.0), ("sess-three", qc, 5004.0)] {
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES (?, ?, ?)",
        )
        .bind(session)
        .bind(quest)
        .bind(at)
        .execute(&pool)
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
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES (?, ?, ?)",
        )
        .bind(session)
        .bind(quest)
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    }
    sugg(
        svc.get_session_link_suggestion("sess-five").await.unwrap(),
        "none",
        "ambiguous_playlist",
    );
    let links = sqlx::query(
        "SELECT session_id, link_type, quest_id, playlist_id, linked_at \
         FROM session_quest_analytics_links ORDER BY session_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .iter()
    .map(|row| {
        json!([
            row.get::<String, _>(0),
            row.get::<String, _>(1),
            row.get::<Option<i64>, _>(2),
            row.get::<Option<i64>, _>(3),
            row.get::<f64, _>(4)
        ])
    })
    .collect::<Vec<_>>();
    assert_eq!(
        links,
        vec![
            json!(["sess-decl", "declined", null, null, 1772366700.0]),
            json!(["sess-one", "quest", qa, null, 1772366700.0]),
            json!(["sess-two", "playlist", null, 1, 1772366700.0]),
        ]
    );

    // Mission matching: exact (case/space), accent folding,
    // repeatable suffix, containment, fuzzy at the threshold, and
    // a miss below it.
    let match_id = |name: &'static str| {
        let svc = svc.clone();
        async move {
            svc.match_quest_by_mission_name(name)
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

    // The reward filter's five legs.
    clock.advance(60.0).unwrap();
    assert_eq!(
        svc.quest_reward_filter(
            "Daily Hunt: Atrox",
            &[],
            &[json!({"skill_name": "Rifle", "amount": 1.0})]
        )
        .await
        .unwrap(),
        Some(json!({"suppress_loot_index": null, "suppress_skill_index": 0}))
    );
    clock.advance(60.0).unwrap();
    assert_eq!(
        svc.quest_reward_filter(
            "Iron Challenge",
            &[
                json!({"item_name": "Shrapnel", "quantity": 100, "value": 0.1}),
                json!({"item_name": "Universal Ammo", "quantity": 1, "value": 2.51}),
            ],
            &[]
        )
        .await
        .unwrap(),
        Some(json!({"suppress_loot_index": 1, "suppress_skill_index": null}))
    );
    clock.advance(60.0).unwrap();
    assert_eq!(
        svc.quest_reward_filter(
            "Iron Challenge",
            &[json!({"item_name": "Shrapnel", "quantity": 100, "value": 0.1})],
            &[]
        )
        .await
        .unwrap(),
        None
    );
    clock.advance(60.0).unwrap();
    assert_eq!(
        svc.quest_reward_filter(
            "Zero Bounty",
            &[
                json!({"item_name": "A", "value": 0.5}),
                json!({"item_name": "B", "value": 0.2}),
                json!({"item_name": "C", "value": 0.9}),
            ],
            &[]
        )
        .await
        .unwrap(),
        Some(json!({"suppress_loot_index": 1, "suppress_skill_index": null}))
    );
    clock.advance(60.0).unwrap();
    assert_eq!(
        svc.quest_reward_filter(
            "Geologist Survey",
            &[json!({"item_name": "A", "value": 0.5})],
            &[]
        )
        .await
        .unwrap(),
        None
    );

    // The overlay trail, exactly as the original recorded it.
    let events = sqlx::query(
        "SELECT session_id, kill_id, event_type, mob_or_item, value_ped, timestamp \
         FROM notable_events ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .iter()
    .map(|row| {
        json!([
            row.get::<String, _>(0),
            row.get::<Option<String>, _>(1),
            row.get::<String, _>(2),
            row.get::<String, _>(3),
            row.get::<f64, _>(4),
            row.get::<f64, _>(5)
        ])
    })
    .collect::<Vec<_>>();
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
    let final_ledger: Vec<String> = sqlx::query("SELECT id FROM ledger_entries ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(final_ledger, ["fixed-0003", "fixed-0004"]);

    // A session stop clears the tracked session: notable events
    // stop recording.
    bus.publish(&BusEvent::SessionStopped(SessionLifecyclePayload {
        session_id: "s1".into(),
    }));
    svc.start_quest_from_mission("Geologist Survey")
        .await
        .unwrap();
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM notable_events")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 6, "no session, no overlay event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_received_mission_event_starts_its_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _pool, _clock, bus) = service_with_clock(dir.path()).await;
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
    let (svc, _pool) = service(dir.path()).await;
    let q = quest_id(&svc.create_quest(&json!({"name": "Dead"})).await.unwrap());
    svc.delete_quest(q).await.unwrap();
    assert_eq!(svc.start_quest(q).await.unwrap(), None);
}

#[tokio::test]
async fn equal_fuzzy_scores_keep_the_first_quest() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, pool) = service(dir.path()).await;
    let first = quest_id(
        &svc.create_quest(&json!({"name": "iron chal a"}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", first, 1000.0).await;
    let second = quest_id(
        &svc.create_quest(&json!({"name": "iron chal b"}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", second, 1001.0).await;

    // Both names score 0.9090909090909091 against the mission (the
    // reference's figure); the strictly-greater comparison keeps
    // the earlier quest.
    let matched = svc
        .match_quest_by_mission_name("iron chal c")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(matched["id"], json!(first));
}

#[tokio::test]
async fn filter_ties_keep_the_first_item() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, _pool) = service(dir.path()).await;
    svc.create_quest(&json!({"name": "Tie Quest", "reward_ped": 2.5}))
        .await
        .unwrap();
    svc.create_quest(&json!({"name": "Zed Bounty", "reward_ped": 0}))
        .await
        .unwrap();

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
    let (svc, pool) = service(dir.path()).await;
    let qa = quest_id(&svc.create_quest(&json!({"name": "Alpha"})).await.unwrap());
    let qc = quest_id(&svc.create_quest(&json!({"name": "Gamma"})).await.unwrap());
    svc.create_playlist(&json!({"name": "Solo Run", "quest_ids": [qc]}))
        .await
        .unwrap();
    for (quest, at) in [(qa, 5003.0), (qc, 5004.0)] {
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES ('s3', ?, ?)",
        )
        .bind(quest)
        .bind(at)
        .execute(&pool)
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
    let (svc, pool) = service(dir.path()).await;
    let qa = quest_id(&svc.create_quest(&json!({"name": "Alpha"})).await.unwrap());
    sqlx::query(
        "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
         VALUES ('s4', ?, 1000.0)",
    )
    .bind(qa)
    .execute(&pool)
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
    sqlx::query(
        "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
         VALUES ('s5', ?, ?)",
    )
    .bind(qe)
    .bind(1772366400.0 - 3600.0)
    .execute(&pool)
    .await
    .unwrap();
    let result = svc.cancel_quest(qe, false).await.unwrap().unwrap();
    assert_eq!(result["last_completed_at"], json!(1772362800.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_session_id_skips_overlay_events() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, pool, _clock, bus) = service_with_clock(dir.path()).await;
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
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM notable_events")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
}

/// The analytics readers over a seeded economy, with every
/// expected object computed by the original implementation over
/// byte-identical seeds (engine numeric types preserved: integer
/// zeros from NULL sums, REAL zeros from real columns, and the
/// raw float artefacts of the engine's arithmetic).
#[tokio::test]
async fn analytics_match_the_original_over_a_seeded_economy() {
    let dir = tempfile::tempdir().unwrap();
    let (svc, pool) = service(dir.path()).await;

    let qa = quest_id(
        &svc.create_quest(&json!({"name": "Alpha", "reward_ped": 2.5,
                                   "expected_reward_markup_percent": 150.0}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qa, 1000.0).await;
    let qb = quest_id(
        &svc.create_quest(&json!({"name": "Beta", "reward_ped": 5.0,
                                   "reward_is_skill": true}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qb, 1001.0).await;
    let qc = quest_id(&svc.create_quest(&json!({"name": "Gamma"})).await.unwrap());
    pin_ts(&pool, "quests", qc, 1002.0).await;
    let qd = quest_id(
        &svc.create_quest(&json!({"name": "Delta", "reward_ped": 1.25}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qd, 1003.0).await;

    let p1 = quest_id(
        &svc.create_playlist(&json!({"name": "Mixed Run", "items": [
            {"quest_id": qa, "group_type": "immediate"},
            {"quest_id": qb, "group_type": "immediate"},
            {"quest_id": qd, "group_type": "long_horizon"},
        ]}))
        .await
        .unwrap(),
    );
    pin_ts(&pool, "quest_playlists", p1, 2000.0).await;
    let p2 = quest_id(
        &svc.create_playlist(&json!({"name": "Bonus Only", "items": [
            {"quest_id": qc, "group_type": "long_horizon"},
        ]}))
        .await
        .unwrap(),
    );
    pin_ts(&pool, "quest_playlists", p2, 2001.0).await;

    for (sid, st, en, active, heal, armour) in [
        ("sess-1", 1000.0, Some(4600.0), 0i64, Some(1.5), Some(0.25)),
        ("sess-2", 5000.0, Some(5030.5), 0, None, Some(0.0)),
        ("sess-3", 6000.0, None, 1, Some(2.0), None),
    ] {
        sqlx::query(
            "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active, heal_cost, armour_cost) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(sid)
        .bind(st)
        .bind(en)
        .bind(active)
        .bind(heal)
        .bind(armour)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (kid, sid, mob, ts, enh, loot) in [
        ("k1", "sess-1", "Atrox", 1100.0, 0.5, 12.75),
        ("k2", "sess-1", "Atrox", 1200.0, 0.0, 3.0),
        ("k3", "sess-2", "Snable", 5010.0, 0.1, 0.0),
    ] {
        sqlx::query(
            "INSERT INTO kills (id, session_id, mob_name, timestamp, shots_fired, damage_dealt, \
             damage_taken, critical_hits, cost_ped, enhancer_cost, loot_total_ped) \
             VALUES (?, ?, ?, ?, 10, 100.0, 5.0, 1, 0.3, ?, ?)",
        )
        .bind(kid)
        .bind(sid)
        .bind(mob)
        .bind(ts)
        .bind(enh)
        .bind(loot)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (kid, tool, shots, cps) in [
        ("k1", "LR-32", 40i64, 0.05),
        ("k1", "Fap-90", 5, 0.02),
        ("k3", "LR-32", 12, 0.05),
    ] {
        sqlx::query(
            "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, damage_dealt, \
             critical_hits, cost_per_shot) VALUES (?, ?, ?, 50.0, 0, ?)",
        )
        .bind(kid)
        .bind(tool)
        .bind(shots)
        .bind(cps)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (sid, skill, ped) in [("sess-1", "Rifle", 0.8), ("sess-2", "Anatomy", 0.2)] {
        sqlx::query(
            "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
             VALUES (?, 1100.0, ?, 1.0, ?)",
        )
        .bind(sid)
        .bind(skill)
        .bind(ped)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (sid, qid, at) in [
        ("sess-1", qa, 1500.0),
        ("sess-1", qb, 1600.0),
        ("sess-1", qd, 1700.0),
        ("sess-2", qa, 5020.0),
    ] {
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES (?, ?, ?)",
        )
        .bind(sid)
        .bind(qid)
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    }
    let qn = quest_id(&svc.create_quest(&json!({"name": "Nul"})).await.unwrap());
    pin_ts(&pool, "quests", qn, 1004.0).await;
    let qz = quest_id(
        &svc.create_quest(&json!({"name": "Zed", "reward_ped": 0,
                                   "expected_reward_markup_percent": 120.0}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qz, 1005.0).await;
    let qe2 = quest_id(
        &svc.create_quest(&json!({"name": "Echo", "reward_ped": 3.0}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quests", qe2, 1006.0).await;
    for (sid, st, en, active, heal) in [
        ("sess-n", 7000.0, Some(7050.0), 0i64, Some(0.0)),
        ("sess-z", 7100.0, Some(7160.0), 0, Some(0.0)),
        ("sess-act", 8000.0, None, 1, None),
        ("sess-solo", 8100.0, Some(8200.0), 0, Some(0.5)),
    ] {
        sqlx::query(
            "INSERT INTO tracking_sessions (id, started_at, ended_at, is_active, heal_cost, armour_cost) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(sid)
        .bind(st)
        .bind(en)
        .bind(active)
        .bind(heal)
        .bind(heal.map(|_| 0.0))
        .execute(&pool)
        .await
        .unwrap();
    }
    for (sid, qid, at) in [("sess-n", qn, 7040.0), ("sess-z", qz, 7150.0)] {
        sqlx::query(
            "INSERT INTO session_quest_completions (session_id, quest_id, completed_at) \
             VALUES (?, ?, ?)",
        )
        .bind(sid)
        .bind(qid)
        .bind(at)
        .execute(&pool)
        .await
        .unwrap();
    }
    let p3 = quest_id(
        &svc.create_playlist(&json!({"name": "Solo Immediate", "quest_ids": [qa]}))
            .await
            .unwrap(),
    );
    pin_ts(&pool, "quest_playlists", p3, 2002.0).await;
    for (sid, lt, qid, plid) in [
        ("sess-1", "playlist", None::<i64>, Some(p1)),
        ("sess-2", "quest", Some(qa), None),
        ("sess-3", "quest", Some(qa), None),
        ("sess-n", "quest", Some(qn), None),
        ("sess-z", "quest", Some(qz), None),
        ("sess-act", "quest", Some(qe2), None),
        ("sess-solo", "playlist", None, Some(p3)),
    ] {
        sqlx::query(
            "INSERT INTO session_quest_analytics_links \
             (session_id, link_type, quest_id, playlist_id, linked_at) \
             VALUES (?, ?, ?, ?, 9000.0)",
        )
        .bind(sid)
        .bind(lt)
        .bind(qid)
        .bind(plid)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Per-quest, name-ordered: Alpha (the still-active linked
    // session is excluded from the completed count but rides in
    // the id set), then the NULL-reward and zero-reward quests
    // whose collapsed rewards and expected totals stay INTEGER
    // zeros on the wire; Echo (linked only by an active session)
    // is excluded entirely.
    assert_eq!(
        svc.get_quest_analytics().await.unwrap(),
        vec![
            json!({
                "quest_id": qa, "quest_name": "Alpha", "planet": "Calypso",
                "category": null, "reward_ped": 2.5, "reward_is_skill": false,
                "expected_reward_markup_percent": 150.0,
                "total_expected_reward_ped": 3.75,
                "linked_sessions": 1, "total_duration": 30.5,
                "weapon_cost": 0.6000000000000001, "heal_cost": 0,
                "enhancer_cost": 0.1, "armour_cost": 0.0, "loot_tt": 0.0,
                "skill_tt": 0.2,
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
            "total_reward_ped": 8.75, "total_immediate_reward_ped": 7.5,
            "total_bonus_reward_ped": 1.25, "total_skill_reward_ped": 5.0,
            "total_immediate_skill_reward_ped": 5.0, "total_bonus_skill_reward_ped": 0,
            "total_expected_reward_ped": 10.0,
            "total_expected_immediate_reward_ped": 8.75,
            "total_expected_bonus_reward_ped": 1.25,
            "matched_sessions": 1, "linked_sessions": 1, "total_duration": 3600.0,
            "weapon_cost": 2.1, "heal_cost": 1.5, "enhancer_cost": 0.5,
            "armour_cost": 0.25, "loot_tt": 15.75, "skill_tt": 0.8,
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
