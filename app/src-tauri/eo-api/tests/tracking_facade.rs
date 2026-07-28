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
    make_api_db(dir, seed, settings).await.0
}

/// [`make_api`] plus a clone of the underlying database handle, for tests
/// that seed or verify rows directly.
async fn make_api_db(dir: &Path, seed: bool, settings: Option<&str>) -> (Api, Db) {
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
    let verify = db.clone();
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current()).await;
    let api = Api::new(
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
        None,
        None,
        None,
    );
    (api, verify)
}

/// One ended session (`ended`) with two `Atrox` kills carrying loot.
async fn seed_ended(db: &Db) {
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,heal_cost,\
             dangling_cost,mob_tracking_mode,updated_at) \
             VALUES('ended',1000.0,4600.0,0,0,0,0,'mob',4600.0)",
            [],
        )?;
        for (id, loot) in [("k1", 10.0), ("k2", 20.0)] {
            conn.execute(
                "INSERT INTO kills(id,session_id,mob_name,timestamp,loot_total_ped) VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![id, "ended", "Atrox", 1001.0, loot],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_empty_session_list_serialises_to_the_empty_page() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let page = api.tracking_sessions(None, None).await.unwrap();
    assert_eq!(
        serde_json::to_string(&page).unwrap(),
        "{\"sessions\":[],\"nextCursor\":null,\"total\":0}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_session_cursor_is_a_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let error = api
        .tracking_sessions(Some("not-a-cursor".to_string()), None)
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&error).unwrap()["kind"], "badRequest");
}

/// Seed `count` ended one-kill sessions at distinct start times (newest
/// last inserted), for the pagination walks.
async fn seed_many_sessions(db: &Db, count: usize) {
    db.with_writer(move |conn| {
        for i in 0..count {
            let sid = format!("s{i:03}");
            let start = 1000.0 + (i as f64) * 10_000.0;
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,\
                 heal_cost,dangling_cost,mob_tracking_mode,updated_at) \
                 VALUES(?1,?2,?3,0,0,0,0,'mob',?3)",
                rusqlite::params![sid, start, start + 600.0],
            )?;
            conn.execute(
                "INSERT INTO kills(id,session_id,mob_name,timestamp,loot_total_ped) \
                 VALUES(?1,?2,'Atrox',?3,5.0)",
                rusqlite::params![format!("{sid}-k"), sid, start + 1.0],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_harvest_session_nets_the_same_on_the_list_and_the_detail() {
    let dir = tempfile::tempdir().unwrap();
    let (api, db) = make_api_db(dir.path(), false, None).await;
    // A harvest-only ended session: swings cost 26.43 in decay, loot
    // returned 16.45. No kills, so every non-harvest cost family is zero.
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,\
             heal_cost,dangling_cost,mob_tracking_mode,updated_at) \
             VALUES('woods',1000.0,4600.0,0,0,0,0,'mob',4600.0)",
            [],
        )?;
        for (id, cost, loot) in [("h1", 13.21, 16.45), ("h2", 13.22, 0.0)] {
            conn.execute(
                "INSERT INTO harvest_events(id,session_id,timestamp,success,tool_name,cost_ped,\
                 loot_total_ped) VALUES(?1,'woods',1001.0,1,'Axe',?2,?3)",
                rusqlite::params![id, cost, loot],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();

    // The list row (served from the healed summary) and the detail read
    // must agree: net = harvest loot - harvest swing decay.
    let page = api.tracking_sessions(None, None).await.unwrap();
    assert_eq!(page.sessions.len(), 1);
    let row = &page.sessions[0];
    let detail = api
        .tracking_session_detail("woods".to_string())
        .await
        .unwrap();
    assert_eq!(row.cost, 26.43);
    assert_eq!(row.returns, 16.45);
    assert_eq!(row.net, -9.98);
    assert_eq!(row.net, detail.summary.net);
    assert_eq!(row.cost, detail.summary.cost);
    assert_eq!(row.returns, detail.summary.returns);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_session_keyset_walk_serves_every_session_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let (api, db) = make_api_db(dir.path(), false, None).await;
    seed_many_sessions(&db, 25).await;

    // First page: newest first, the whole-table count riding along, a
    // further page signalled by the cursor.
    let first = api.tracking_sessions(None, Some(10)).await.unwrap();
    assert_eq!(first.sessions.len(), 10);
    assert_eq!(first.total, 25);
    assert_eq!(first.sessions[0].id, "s024");
    let cursor = first.next_cursor.0.clone().expect("a further page follows");

    // Walk the cursor to exhaustion: every session appears exactly once.
    let mut seen: Vec<String> = first.sessions.iter().map(|s| s.id.clone()).collect();
    let mut token = Some(cursor);
    while let Some(current) = token {
        let page = api
            .tracking_sessions(Some(current), Some(10))
            .await
            .unwrap();
        seen.extend(page.sessions.iter().map(|s| s.id.clone()));
        token = page.next_cursor.0.clone();
    }
    assert_eq!(seen.len(), 25, "every session is served");
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 25, "no session is served twice");
    // The walk is newest-first end to end.
    assert_eq!(seen.first().map(String::as_str), Some("s024"));
    assert_eq!(seen.last().map(String::as_str), Some("s000"));
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
         \"trifectaAttribution\":{\"activePresetId\":\"default\",\
         \"presetName\":\"Default\",\"presets\":[{\"id\":\"default\",\"name\":\"Default\"}],\
         \"smallWeapon\":null,\"bigWeapon\":null,\"healTool\":null},\"recentEvents\":[]}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_session_name_suggestions_serialise_to_the_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;
    let suggestions = api
        .tracking_session_name_suggestions(String::new(), None)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deleting_a_session_cascades_and_guards_active_and_missing() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = dir.path().join("snapshot");
    std::fs::create_dir_all(&snapshot).unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Db::open(&data_dir.join("entropia_orme.db"))
        .await
        .expect("migrated database");

    // One ended session (`ended`) whose kill `k1` fans out into every child
    // table, plus an active session (`live`) that must resist deletion.
    seed_ended(&db).await;
    db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO kill_tool_stats(kill_id,tool_name) VALUES('k1','Weapon')",
            [],
        )?;
        conn.execute(
            "INSERT INTO kill_loot_items(kill_id,item_name,value_ped) VALUES('k1','Shrapnel',5.0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO skill_gains(session_id,timestamp,skill_name,amount) \
             VALUES('ended',1001.0,'Rifle',1.0)",
            [],
        )?;
        conn.execute(
            "INSERT INTO notable_events(session_id,event_type,mob_or_item,value_ped,timestamp) \
             VALUES('ended','global','Atrox',60.0,1001.0)",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let verify = db.clone();
    let game_data = Arc::new(GameDataStore::new(&snapshot).expect("empty game-data store"));
    let clock = Arc::new(RealClock::new());
    let handles = common::producer_handles(&db, &data_dir, tokio::runtime::Handle::current()).await;
    let api = Api::new(
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
        None,
        None,
        None,
    );

    // Insert the active session AFTER composition so the tracker's startup
    // orphan-recovery (which ends dangling active sessions) leaves it active.
    verify
        .with_writer(move |conn| {
            conn.execute(
                "INSERT INTO tracking_sessions(id,started_at,is_active,mob_tracking_mode) \
                 VALUES('live',2000.0,1,'mob')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    // An active session is a conflict; an absent one is a not-found.
    let active = api
        .tracking_session_delete("live".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&active).unwrap()["kind"], "conflict");
    let missing = api
        .tracking_session_delete("nope".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&missing).unwrap()["kind"], "notFound");

    // Happy path: the ended session and every row that references it are gone,
    // and the active session is untouched.
    api.tracking_session_delete("ended".to_string())
        .await
        .unwrap();
    for (label, query) in [
        (
            "kills",
            "SELECT COUNT(*) FROM kills WHERE session_id='ended'",
        ),
        (
            "skill_gains",
            "SELECT COUNT(*) FROM skill_gains WHERE session_id='ended'",
        ),
        (
            "notable_events",
            "SELECT COUNT(*) FROM notable_events WHERE session_id='ended'",
        ),
        (
            "tracking_sessions",
            "SELECT COUNT(*) FROM tracking_sessions WHERE id='ended'",
        ),
        (
            "kill_tool_stats",
            "SELECT COUNT(*) FROM kill_tool_stats WHERE kill_id='k1'",
        ),
        (
            "kill_loot_items",
            "SELECT COUNT(*) FROM kill_loot_items WHERE kill_id='k1'",
        ),
    ] {
        let query = query.to_string();
        let count: i64 = verify
            .with_reader(move |conn| Ok(conn.query_row(&query, [], |row| row.get::<_, i64>(0))?))
            .await
            .unwrap();
        assert_eq!(count, 0, "{label} still holds rows for the deleted session");
    }
    let live: i64 = verify
        .with_reader(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM tracking_sessions WHERE id='live'",
                [],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(live, 1, "the active session must survive");

    // Deleting the now-gone session is a not-found.
    let again = api
        .tracking_session_delete("ended".to_string())
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&again).unwrap()["kind"], "notFound");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_quest_link_decline_reply_narrows_to_the_two_set_fields() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), true, None).await;
    // The decline reply shares the idle snapshot's exclude-unset ->
    // exclude-none narrowing: the five accept-only fields are None and
    // skip, leaving exactly `sessionId` / `status` on the wire.
    let decision = api
        .tracking_quest_link("ended".to_string(), "decline".to_string())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_string(&decision).unwrap(),
        "{\"sessionId\":\"ended\",\"status\":\"declined\"}"
    );
}

/// The facet keys must actually reach the wire, with the values that
/// were declared.
///
/// Pinned because nothing else pins it: every other snapshot assertion
/// leaves both facets undeclared, so the exclude-none projection drops
/// the keys and a serialiser that emitted null unconditionally would
/// pass the whole suite. These are newly emitted fields; first
/// generation is exactly when an unpinned field is cheapest to get
/// wrong and hardest to notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_idle_snapshot_carries_the_declared_facets() {
    let dir = tempfile::tempdir().unwrap();
    let api = make_api(dir.path(), false, None).await;

    api.tracking_session_config(Some("ARIS Dailies".into()), Some(50))
        .await
        .unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert_eq!(value["sessionName"], "ARIS Dailies");
    assert_eq!(value["skillBoostPercent"], 50);

    // The declared zero survives the wire as 0, NOT as null and NOT
    // dropped: null is reserved for "nothing declared", and collapsing
    // the two would erase the unboosted baseline the whole
    // boost-measurement question rests on.
    api.tracking_session_config(Some("ARIS Dailies".into()), Some(0))
        .await
        .unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert_eq!(value["skillBoostPercent"], 0);

    // Withdrawn: the key drops out of the projection entirely.
    api.tracking_session_config(None, None).await.unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert!(value.get("skillBoostPercent").is_none());
    assert!(value.get("sessionName").is_none());
}

/// The ACTIVE branch's boost, which has a different source from the
/// idle branch's and so could regress without the idle pin noticing.
///
/// It is read from the running session's interval state, NOT from the
/// session row, because the row's column cannot hold a zero (migration
/// 0018 constrains it to `> 0 OR NULL`). A serialiser that went back to
/// the row would silently lose the declared baseline for every running
/// session; that is the regression this pins.
///
/// Only the boost is asserted here. The session name is seeded onto the
/// session from config at start, and this harness composes its tracker
/// with the inert config providers, so an active session in it never
/// carries one. The name's projection is the same `name_value` helper
/// the idle branch pins above; the boost's is not shared, which is why
/// it needs its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_active_snapshot_carries_the_declared_boost() {
    let dir = tempfile::tempdir().unwrap();
    // Hotbar attribution with one slot bound, so the start satisfies
    // that gate rather than the trifecta loadout one.
    let api = make_api(
        dir.path(),
        false,
        Some("{\"hotbar_hooks_enabled\": true, \"hotbar\": {\"1\": 1}}"),
    )
    .await;
    api.tracking_start().await.unwrap();

    api.tracking_session_config(None, Some(50)).await.unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert_eq!(value["status"], "active");
    assert_eq!(value["skillBoostPercent"], 50);

    // Re-declared mid-session to the unboosted baseline: the running
    // session must report 0, not null and not the opening 50.
    api.tracking_session_config(None, Some(0)).await.unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert_eq!(value["skillBoostPercent"], 0);

    // Withdrawn mid-session: the key leaves the projection.
    api.tracking_session_config(None, None).await.unwrap();
    let value = serde_json::to_value(api.tracking_snapshot().await.unwrap()).unwrap();
    assert!(value.get("skillBoostPercent").is_none());
}
