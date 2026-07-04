//! Hermetic router-level coverage for the native registrations. The test
//! harness composes a temp-database hydration state and drives the router
//! in-memory: each request is dispatched through `build_router(state).oneshot`
//! (the same router core the production binary serves in-process via the IPC
//! command), with no socket and no transport. A registered route answers
//! natively, an unmatched path is the framework 404, and an unported method
//! the framework 405. This pins registration, adapter extraction, the
//! validation envelopes, and the conditional-GET / CORS contracts without a
//! Python toolchain.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use eo_http::cors::CorsConfig;
use eo_http::hydration::HydrationState;
use eo_http::AppState;
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tower::ServiceExt;

async fn serve_substrate() -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Db::open(&dir.path().join("entropia_orme.db"))
        .await
        .expect("temp db opens");
    let game_data = Arc::new(GameDataStore::new(&dir.path().join("empty")).expect("empty store"));
    let hydration = Arc::new(HydrationState::new(
        db,
        game_data,
        Arc::new(RealClock::new()),
        dir.path().to_path_buf(),
    ));
    let state = Arc::new(
        AppState::new(0)
            .with_hydration(hydration)
            .with_cors(CorsConfig::new(5173, None)),
    );
    (state, dir)
}

#[tokio::test]
async fn optimize_on_shutdown_runs_over_a_composed_state_and_no_ops_without_one() {
    // A composed hydration state has a pool to optimise.
    let (state, _dir) = serve_substrate().await;
    assert!(
        state.optimize_on_shutdown().await,
        "PRAGMA optimize runs against the composed hydration pool"
    );
    // A bare substrate with no hydration has nothing to optimise.
    let bare = Arc::new(AppState::new(0));
    assert!(
        !bare.optimize_on_shutdown().await,
        "no hydration state means nothing to optimise"
    );
}

async fn get(state: &Arc<AppState>, path: &str) -> (http::StatusCode, http::HeaderMap, Vec<u8>) {
    request(state, "GET", path, &[]).await
}

async fn request(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> (http::StatusCode, http::HeaderMap, Vec<u8>) {
    send(state, method, path, extra_headers, None).await
}

/// A mutating request: a JSON body plus the allowed origin the guard
/// demands of mutating methods.
async fn send_json(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    body: &str,
) -> (http::StatusCode, http::HeaderMap, Vec<u8>) {
    send(
        state,
        method,
        path,
        &[
            ("origin", "tauri://localhost"),
            ("content-type", "application/json"),
        ],
        Some(body.as_bytes().to_vec()),
    )
    .await
}

/// Dispatch one request through a freshly built router (`oneshot` consumes
/// the router, so each request gets its own). No socket and no Host default:
/// a request without a Host header is admitted exactly as the in-process IPC
/// transport's requests are, so the guard / CORS / observe stack is exercised
/// identically.
async fn send(
    state: &Arc<AppState>,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> (http::StatusCode, http::HeaderMap, Vec<u8>) {
    let mut builder = http::Request::builder().method(method).uri(path);
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(body.map(Body::from).unwrap_or_else(Body::empty))
        .unwrap();
    let response = eo_http::build_router(state.clone())
        .oneshot(request)
        .await
        .expect("router responds");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

fn detail_types(body: &[u8]) -> Vec<String> {
    let parsed: Value = serde_json::from_slice(body).expect("envelope parses");
    parsed["detail"]
        .as_array()
        .expect("detail is a list")
        .iter()
        .map(|issue| issue["type"].as_str().expect("typed issue").to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_registered_route_serves_natively_over_the_composed_state() {
    let (state, _dir) = serve_substrate().await;
    // List routes over a fresh database: empty collections, served
    // natively (a proxy fallback would 502 against the dead upstream).
    for (path, expected_body) in [
        // The tracking session reads (ETag-scoped; empty db -> []).
        ("/api/tracking/sessions", "[]"),
        ("/api/tracking/tag-suggestions?q=a", "[]"),
    ] {
        let (status, headers, body) = get(&state, path).await;
        assert_eq!(status, http::StatusCode::OK, "{path}");
        assert_eq!(body, expected_body.as_bytes(), "{path}");
        assert!(headers.contains_key(http::header::ETAG), "{path}");
        assert_eq!(
            headers
                .get(http::header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-cache"),
            "{path}"
        );
    }
    // A missing tracking session: the handler's 404, no ETag.
    let (status, headers, body) = get(&state, "/api/tracking/session/no-such").await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
    assert_eq!(body, b"{\"detail\":\"Session not found\"}");
    assert!(!headers.contains_key(http::header::ETAG));

    // The conditional-GET leg: the current validator earns a 304 with
    // an empty body; a stale one re-serves the representation.
    let (_, headers, _) = get(&state, "/api/tracking/sessions").await;
    let etag = headers
        .get(http::header::ETAG)
        .expect("etag present")
        .to_str()
        .unwrap()
        .to_string();
    let (status, headers, body) = request(
        &state,
        "GET",
        "/api/tracking/sessions",
        &[("if-none-match", etag.as_str())],
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert_eq!(
        headers.get(http::header::ETAG).unwrap().to_str().unwrap(),
        etag
    );
    let (status, _, _) = request(
        &state,
        "GET",
        "/api/tracking/sessions",
        &[("if-none-match", "\"stale\"")],
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_browser_surface_is_answered_at_the_substrate() {
    let (state, _dir) = serve_substrate().await;
    // A passing preflight short-circuits ahead of routing (no upstream
    // exists, so a forwarded preflight would 502).
    let (status, headers, body) = request(
        &state,
        "OPTIONS",
        "/api/tracking/sessions",
        &[
            ("origin", "tauri://localhost"),
            ("access-control-request-method", "GET"),
        ],
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body, b"OK");
    assert_eq!(
        headers
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "tauri://localhost"
    );
    // A failing preflight names its failure.
    let (status, _, body) = request(
        &state,
        "OPTIONS",
        "/api/tracking/sessions",
        &[
            ("origin", "http://evil.example"),
            ("access-control-request-method", "GET"),
        ],
    )
    .await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(body, b"Disallowed CORS origin");
    // Natively-served responses decorate for an allowed origin.
    let (status, headers, _) = request(
        &state,
        "GET",
        "/api/tracking/sessions",
        &[("origin", "tauri://localhost")],
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(
        headers
            .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "tauri://localhost"
    );
    assert_eq!(headers.get(http::header::VARY).unwrap(), "Origin");
    // Reads reject a present-but-disallowed origin ahead of routing.
    let (status, _, body) = request(
        &state,
        "GET",
        "/api/tracking/sessions",
        &[("origin", "http://evil.example")],
    )
    .await;
    assert_eq!(status, http::StatusCode::FORBIDDEN);
    assert_eq!(body, b"{\"detail\":\"Invalid Origin header\"}");
    // Mutating methods require an allowed origin, enforced before any
    // upstream forward (a forwarded request would 502, not 403). Target a
    // real POST route so this proves a valid mutating route is guarded
    // before dispatch, not merely an unmatched-method path.
    let (status, _, body) = request(&state, "POST", "/api/tracking/start", &[]).await;
    assert_eq!(status, http::StatusCode::FORBIDDEN);
    assert_eq!(body, b"{\"detail\":\"Origin header required\"}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_framework_404s_unmatched_paths_and_405s_unported_methods() {
    let (state, _dir) = serve_substrate().await;
    // An encoded slash stays one raw segment, so it does not decode into the
    // registered `/api/tracking/sessions` path: the framework 404 (nothing
    // forwards it upstream now).
    let (status, _, body) = get(&state, "/api/tracking%2Fsessions").await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
    assert_eq!(body, b"{\"detail\":\"Not Found\"}");
    // An unmatched path under /api is likewise the framework 404.
    let (status, _, _) = get(&state, "/api/no-such-route").await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
    // HEAD on a GET route is the framework 405: the backend hard-405s HEAD on
    // its GET routes, and the native router does not auto-serve HEAD from the
    // GET handler (it carries an explicit 405 method fallback).
    let (status, _, _) = request(&state, "HEAD", "/api/tracking/sessions", &[]).await;
    assert_eq!(status, http::StatusCode::METHOD_NOT_ALLOWED);
    // A present-but-empty Host passes the guard (the backend's falsy check
    // skips it), so the route serves natively.
    let (status, _, _) = request(&state, "GET", "/api/tracking/sessions", &[("host", "")]).await;
    assert_eq!(status, http::StatusCode::OK);
}

// ── Tracking session-edit write adapters, end-to-end and hermetic ──────
//
// The five edit adapters (`native.rs`) and the HydrationState method
// wrappers (`tracking_routes.rs`) were once driven end-to-end only by
// the now-retired cross-language battery, so the hermetic mutation
// campaign never exercised them. This test seeds an ended + an active
// session straight into the substrate's database and drives every edit
// through the public port, asserting the RESPONSE BODY fields (not just
// the status) so an adapter/wrapper degraded to `Default::default()`
// (an empty `Response`) is caught. Activate vs deactivate produce
// distinct results from the same wildcard registration, distinguishing
// the suffix dispatch and the path splitter.

const ENDED_MOB: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const ENDED_LOOT: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const ACTIVE: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

async fn open_pool(path: std::path::PathBuf) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(false)
                .busy_timeout(Duration::from_secs(5)),
        )
        .await
        .expect("open shared database")
}

/// Seed the substrate's own database (the schema was created by
/// `Db::open` in `serve_substrate`) with the fixtures every edit needs:
///   - `ENDED_MOB` (is_active=0): two "Atrox" kills (rename target) and
///     one "Argo" kill whose `original_mob_name` is "Wolf" (restore
///     target);
///   - `ENDED_LOOT` (is_active=0): a kill with an ACTIVE "AnimalOil"
///     loot row (deactivate target) and a kill with a DEACTIVATED "Old
///     Hide" row (activate target), plus an item name carrying a slash;
///   - `ACTIVE` (is_active=1): the 409 case.
async fn seed_edits(pool: &SqlitePool) {
    let base = 1_750_000_000.0_f64;
    for (id, active) in [(ENDED_MOB, 0_i64), (ENDED_LOOT, 0), (ACTIVE, 1)] {
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,heal_cost,dangling_cost,mob_tracking_mode,updated_at) \
             VALUES(?,?,?,?,?,?,?,?,?)",
        )
        .bind(id)
        .bind(base)
        .bind(if active == 0 { Some(base + 3600.0) } else { None })
        .bind(active)
        .bind(1.0_f64)
        .bind(0.0_f64)
        .bind(0.0_f64)
        .bind("mob")
        .bind(base + 3600.0)
        .execute(pool)
        .await
        .expect("seed session");
    }

    // ENDED_MOB: two Atrox kills (rename) + one renamed Argo (restore).
    for i in 0..2 {
        sqlx::query(
            "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,shots_fired,damage_dealt,damage_taken,critical_hits,cost_ped,enhancer_cost,loot_total_ped,is_global,is_hof,original_mob_name) \
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(format!("k-mob-{i}")).bind(ENDED_MOB).bind("Atrox").bind("").bind("")
        .bind(base + i as f64).bind(10_i64).bind(50.0).bind(0.0).bind(0_i64)
        .bind(0.5).bind(0.0).bind(3.0).bind(0_i64).bind(0_i64).bind(Option::<String>::None)
        .execute(pool).await.expect("seed mob kill");
    }
    sqlx::query(
        "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,shots_fired,damage_dealt,damage_taken,critical_hits,cost_ped,enhancer_cost,loot_total_ped,is_global,is_hof,original_mob_name) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("k-mob-renamed").bind(ENDED_MOB).bind("Argo").bind("").bind("")
    .bind(base + 10.0).bind(10_i64).bind(50.0).bind(0.0).bind(0_i64)
    .bind(0.5).bind(0.0).bind(3.0).bind(0_i64).bind(0_i64).bind(Some("Wolf"))
    .execute(pool).await.expect("seed renamed kill");

    // ENDED_LOOT: K_LD carries an ACTIVE "AnimalOil" (deactivate
    // target, value 2.0, parent loot_total 5.0); K_LA carries a
    // DEACTIVATED "OldHide" (activate target, value 3.0, parent
    // loot_total 4.0) plus a slash-bearing item name.
    sqlx::query(
        "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,shots_fired,damage_dealt,damage_taken,critical_hits,cost_ped,enhancer_cost,loot_total_ped,is_global,is_hof,original_mob_name) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("k-ld").bind(ENDED_LOOT).bind("Atrox").bind("").bind("")
    .bind(base).bind(10_i64).bind(50.0).bind(0.0).bind(0_i64)
    .bind(0.5).bind(0.0).bind(5.0).bind(0_i64).bind(0_i64).bind(Option::<String>::None)
    .execute(pool).await.expect("seed ld kill");
    sqlx::query(
        "INSERT INTO kills(id,session_id,mob_name,mob_species,mob_maturity,timestamp,shots_fired,damage_dealt,damage_taken,critical_hits,cost_ped,enhancer_cost,loot_total_ped,is_global,is_hof,original_mob_name) \
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind("k-la").bind(ENDED_LOOT).bind("Atrox").bind("").bind("")
    .bind(base + 1.0).bind(10_i64).bind(50.0).bind(0.0).bind(0_i64)
    .bind(0.5).bind(0.0).bind(4.0).bind(0_i64).bind(0_i64).bind(Option::<String>::None)
    .execute(pool).await.expect("seed la kill");
    sqlx::query("INSERT INTO kill_loot_items(kill_id,item_name,quantity,value_ped,is_enhancer_shrapnel,deactivated_at) VALUES(?,?,?,?,?,?)")
        .bind("k-ld").bind("AnimalOil").bind(1_i64).bind(2.0).bind(0_i64).bind(Option::<f64>::None)
        .execute(pool).await.expect("seed active loot");
    sqlx::query("INSERT INTO kill_loot_items(kill_id,item_name,quantity,value_ped,is_enhancer_shrapnel,deactivated_at) VALUES(?,?,?,?,?,?)")
        .bind("k-la").bind("OldHide").bind(1_i64).bind(3.0).bind(0_i64).bind(Some(base + 50.0))
        .execute(pool).await.expect("seed deactivated loot");
    // A slash-bearing item name: the `{item_name:path}` converter KEEPS
    // the decoded slash rather than 404-ing it (the session id, a single
    // segment, still de-matches a slash).
    sqlx::query("INSERT INTO kill_loot_items(kill_id,item_name,quantity,value_ped,is_enhancer_shrapnel,deactivated_at) VALUES(?,?,?,?,?,?)")
        .bind("k-ld").bind("Metal/Wire").bind(1_i64).bind(1.0).bind(0_i64).bind(Option::<f64>::None)
        .execute(pool).await.expect("seed slash loot");
}

fn body_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).expect("response body is JSON")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracking_session_edits_drive_the_adapters_and_wrappers_end_to_end() {
    let (state, dir) = serve_substrate().await;
    let pool = open_pool(dir.path().join("entropia_orme.db")).await;
    seed_edits(&pool).await;

    // ── rename-mob: success body (sessionId / mobName / killCount) ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_MOB}/rename-mob"),
        "{\"fromMobName\":\"Atrox\",\"toMobName\":\"Daikiba\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], ENDED_MOB);
    assert_eq!(v["mobName"], "Daikiba");
    assert_eq!(v["killCount"], 2);

    // ── restore-mob: the "Argo" kill restores to its "Wolf" original ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_MOB}/restore-mob"),
        "{\"currentMobName\":\"Argo\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], ENDED_MOB);
    assert_eq!(v["mobName"], "Wolf");
    assert_eq!(v["killCount"], 1);

    // ── loot-item deactivate: full body, incl. signed delta + totals ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_LOOT}/loot-item/AnimalOil/deactivate"),
        "",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], ENDED_LOOT);
    assert_eq!(v["itemName"], "AnimalOil");
    assert_eq!(v["affectedRows"], 1);
    assert_eq!(v["totalValueDelta"], -2.0);
    // K_LD 5.0 - 2.0 = 3.0; K_LA still 4.0 -> 7.0.
    assert_eq!(v["sessionTotalReturns"], 7.0);

    // ── loot-item activate on a DEACTIVATED row: the OTHER suffix arm,
    //    distinct result -> kills the wildcard split + suffix dispatch ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_LOOT}/loot-item/OldHide/activate"),
        "",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], ENDED_LOOT);
    assert_eq!(v["itemName"], "OldHide");
    assert_eq!(v["affectedRows"], 1);
    assert_eq!(v["totalValueDelta"], 3.0);
    // K_LD now 3.0; K_LA 4.0 + 3.0 = 7.0 -> 10.0.
    assert_eq!(v["sessionTotalReturns"], 10.0);

    // ── loot-item with a slash in the {item_name:path} segment: the
    //    converter KEEPS the slash, so the item is found and flipped ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_LOOT}/loot-item/Metal/Wire/deactivate"),
        "",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["itemName"], "Metal/Wire");
    assert_eq!(v["affectedRows"], 1);

    // ── armour-cost: echoes round(cost, 2), NOT the new total ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_LOOT}/armour-cost"),
        "{\"cost\":2.5}",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], ENDED_LOOT);
    assert_eq!(v["armourCost"], 2.5);

    // ── 404: a missing session on every guarded edit ──
    let missing = "00000000-0000-4000-8000-000000000000";
    for (path, body) in [
        (
            format!("/api/tracking/session/{missing}/rename-mob"),
            "{\"fromMobName\":\"a\",\"toMobName\":\"b\"}",
        ),
        (
            format!("/api/tracking/session/{missing}/restore-mob"),
            "{\"currentMobName\":\"a\"}",
        ),
        (
            format!("/api/tracking/session/{missing}/loot-item/AnimalOil/deactivate"),
            "",
        ),
        (
            format!("/api/tracking/session/{missing}/armour-cost"),
            "{\"cost\":1.0}",
        ),
    ] {
        let (status, _, _) = send_json(&state, "POST", &path, body).await;
        assert_eq!(
            status,
            http::StatusCode::NOT_FOUND,
            "missing session 404: {path}"
        );
    }

    // ── 409: an ACTIVE session on the three guarded mob/loot edits
    //    (armour-cost deliberately omits the guard) ──
    for (path, body) in [
        (
            format!("/api/tracking/session/{ACTIVE}/rename-mob"),
            "{\"fromMobName\":\"a\",\"toMobName\":\"b\"}",
        ),
        (
            format!("/api/tracking/session/{ACTIVE}/restore-mob"),
            "{\"currentMobName\":\"a\"}",
        ),
        (
            format!("/api/tracking/session/{ACTIVE}/loot-item/AnimalOil/deactivate"),
            "",
        ),
    ] {
        let (status, _, _) = send_json(&state, "POST", &path, body).await;
        assert_eq!(
            status,
            http::StatusCode::CONFLICT,
            "active session 409: {path}"
        );
    }

    // ── 400: a blank mob name (the validated-then-trimmed empty leg) ──
    let (status, _, _) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_MOB}/rename-mob"),
        "{\"fromMobName\":\"   \",\"toMobName\":\"x\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);

    // ── 404: the wildcard tail matches NEITHER suffix ──
    let (status, _, _) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{ENDED_LOOT}/loot-item/Foo/bogus"),
        "",
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
}

// ── Quest-link routes: hermetic body-asserting coverage ──

const QL_QUEST: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"; // single_quest -> accept
const QL_DECLINE: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"; // decline

/// Seed the quest-link fixtures into the substrate's own database: one
/// quest (id 2), a single-quest completion for `QL_QUEST` (the accept
/// target) and a separate session `QL_DECLINE` with one completion (the
/// decline target). No playlists are needed for the single-quest path.
async fn seed_quest_link(pool: &SqlitePool) {
    for id in [QL_QUEST, QL_DECLINE] {
        sqlx::query(
            "INSERT INTO tracking_sessions(id,started_at,ended_at,is_active,armour_cost,heal_cost,dangling_cost,mob_tracking_mode,updated_at) \
             VALUES(?,1000.0,4600.0,0,0,0,0,'mob',4600.0)",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed quest-link session");
    }
    sqlx::query(
        "INSERT INTO quests(id,name,planet,is_active,created_at,category) VALUES(2,'Quest 2','Calypso',1,1000.0,'kill')",
    )
    .execute(pool)
    .await
    .expect("seed quest");
    for id in [QL_QUEST, QL_DECLINE] {
        sqlx::query(
            "INSERT INTO session_quest_completions(session_id,quest_id,completed_at) VALUES(?,2,2000.0)",
        )
        .bind(id)
        .execute(pool)
        .await
        .expect("seed completion");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quest_link_routes_drive_the_adapters_and_handlers_end_to_end() {
    let (state, dir) = serve_substrate().await;
    let pool = open_pool(dir.path().join("entropia_orme.db")).await;
    seed_quest_link(&pool).await;

    // ── GET suggestion for a single_quest session: the 7-field body ──
    let (status, headers, body) = get(
        &state,
        &format!("/api/tracking/session/{QL_QUEST}/quest-link-suggestion"),
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    assert_eq!(v["sessionId"], QL_QUEST);
    assert_eq!(v["suggestionType"], "quest");
    assert_eq!(v["reason"], "single_quest");
    assert_eq!(v["questId"], "2");
    assert_eq!(v["questName"], "Quest 2");
    assert!(v["playlistId"].is_null());
    assert!(v["playlistName"].is_null());
    // ETag-scoped: the read carries a strong ETag for the 304 leg below.
    let etag = headers
        .get(http::header::ETAG)
        .expect("suggestion etag")
        .to_str()
        .unwrap()
        .to_string();

    // ── GET 304: re-fetch with the prior ETag -> empty 304 ──
    let (status, _, body) = request(
        &state,
        "GET",
        &format!("/api/tracking/session/{QL_QUEST}/quest-link-suggestion"),
        &[("if-none-match", etag.as_str())],
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());

    // ── POST accept: persists; replies linked + linkType + questId ──
    let (status, headers, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{QL_QUEST}/quest-link"),
        "{\"action\":\"accept\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    // The write is a plain 200: no ETag (unlike the GET).
    assert!(!headers.contains_key(http::header::ETAG));
    let v = body_json(&body);
    assert_eq!(v["sessionId"], QL_QUEST);
    assert_eq!(v["status"], "linked");
    assert_eq!(v["linkType"], "quest");
    assert_eq!(v["questId"], "2");
    assert_eq!(v["questName"], "Quest 2");

    // ── POST decline on another session: EXACTLY {sessionId, status} ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{QL_DECLINE}/quest-link"),
        "{\"action\":\"decline\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    let v = body_json(&body);
    let object = v.as_object().expect("decline body is an object");
    assert_eq!(
        object.len(),
        2,
        "decline omits the link fields entirely (exactly sessionId + status), got {object:?}"
    );
    assert_eq!(v["sessionId"], QL_DECLINE);
    assert_eq!(v["status"], "declined");

    // ── 404: a missing session on BOTH routes ──
    let missing = "00000000-0000-4000-8000-000000000000";
    let (status, _, body) = get(
        &state,
        &format!("/api/tracking/session/{missing}/quest-link-suggestion"),
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
    assert_eq!(body_json(&body)["detail"], "Session not found");
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{missing}/quest-link"),
        "{\"action\":\"decline\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_FOUND);
    assert_eq!(body_json(&body)["detail"], "Session not found");

    // ── 400: an unrecognised action ──
    let (status, _, body) = send_json(
        &state,
        "POST",
        &format!("/api/tracking/session/{QL_QUEST}/quest-link"),
        "{\"action\":\"frobnicate\"}",
    )
    .await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(&body)["detail"],
        "Action must be 'accept' or 'decline'"
    );
}

// ── Tracking PRODUCER routes (start / stop / manual-mob-suggestions) ──
//
// These three reach the live `Arc<HuntTracker>` (and, for the
// suggestions, the bundled mobs catalogue) rather than the read-only
// database surface, so the hermetic harness above (no tracker) cannot
// drive them: it composes hydration only, so they answer the 503
// service-unavailable floor.
// This block boots a SEPARATE substrate that wires both the read surface
// AND a live tracker over a shared single-owner pool, plus a temp data
// dir carrying a `mobs.json` (the suggestions catalogue) and a
// `settings.json` (the attribution gate's config read), then drives the
// full leg set through the public port asserting RESPONSE BODY fields so
// a wrapper degraded to an empty `Response` is caught. The retired
// cross-language oracle proved this surface byte-identical; the
// committed goldens now hold it.

/// A substrate composed with BOTH the read surface and a live tracker
/// over a shared pool. `config_json` seeds `settings.json` (the
/// attribution gate + the idle tag-mode leg read it); a small mobs
/// catalogue seeds the suggestions lookup.
async fn serve_producer_substrate(config_json: &str) -> (Arc<AppState>, tempfile::TempDir) {
    use eo_services::event_bus::EventBus;
    use std::sync::Mutex;

    use eo_services::config_service::ConfigService;
    use eo_services::hotbar_listener::HotbarListener;
    use eo_services::keystroke_source::MockKeystrokeSource;
    use eo_services::repair_ocr::{RepairOcrService, RepairProviders};
    use eo_services::skill_panel::BgrImage;
    use eo_services::tracker::{HuntTracker, Providers};

    let dir = tempfile::tempdir().expect("temp dir");
    // The attribution gate and the idle tag-mode leg read settings.json
    // from the data dir; seed it before composition.
    std::fs::write(dir.path().join("settings.json"), config_json).expect("seed settings");
    // The suggestions lookup reads the mobs catalogue from the game-data
    // store directory.
    let store_dir = dir.path().join("snapshot");
    std::fs::create_dir_all(&store_dir).expect("store dir");
    std::fs::write(
        store_dir.join("mobs.json"),
        r#"[{"id":1,"species":{"name":"Atrox"},"maturities":[{"name":"Young"},{"name":"Old"}]}]"#,
    )
    .expect("seed mobs");

    let db = Db::open(&dir.path().join("entropia_orme.db"))
        .await
        .expect("temp db opens");
    let game_data = Arc::new(GameDataStore::new(&store_dir).expect("mobs store"));
    let clock = Arc::new(RealClock::new());
    let bus = Arc::new(EventBus::new());
    // The tracker shares the substrate's single-owner pool (one
    // connection, serialised access), exactly as composition wires it.
    let tracker = HuntTracker::new(
        bus.clone(),
        eo_services::db::Db::from_pool(db.write().clone()),
        tokio::runtime::Handle::current(),
        clock.clone(),
        Providers::default(),
    )
    .expect("tracker builds over the fresh pool");
    // The settings writer (the config-write routes) shares the same pool,
    // clock, and data dir, exactly as composition wires it, so the write
    // routes serve natively here.
    let config_service = Arc::new(Mutex::new(
        ConfigService::new(dir.path()).expect("config service opens"),
    ));

    // The repair-cost reader, composed over deterministic test providers (no
    // real screen capture or OCR): a fixed region is found and the OCR reads a
    // fixed cost. The manual skill scan moved to the typed-command facade
    // (`eo-api/tests/scan_facade.rs`); only the repair-scan leg is exercised
    // over the HTTP arm here.
    let repair_ocr = Arc::new(RepairOcrService::new(RepairProviders {
        repair_region: Arc::new(|| Some(([10, 20], [110, 60]))),
        capture_region: Arc::new(|_, _, _, _| {
            Some(BgrImage {
                data: vec![0; 12],
                h: 2,
                w: 2,
            })
        }),
        read_text: Arc::new(|_| Some(("2,20 PED".to_string(), 0.97))),
    }));
    // The hotbar listener (the snapshot route reads its running state). A mock
    // source and no resolver, never enabled, so it reports not-running.
    let hotbar = HotbarListener::new(
        bus.clone(),
        Some(Arc::new(MockKeystrokeSource::new())),
        None,
    );

    let hydration = Arc::new(HydrationState::new(
        eo_services::db::Db::from_pool(db.write().clone()),
        game_data,
        clock,
        dir.path().to_path_buf(),
    ));

    let state = Arc::new(
        AppState::new(0)
            .with_hydration(hydration)
            .with_tracker(tracker)
            .with_config_service(config_service)
            .with_repair_ocr(repair_ocr)
            .with_hotbar_listener(hotbar)
            .with_cors(CorsConfig::new(5173, None)),
    );
    (state, dir)
}

/// A settings.json with hotbar mode and slot "1" bound: the attribution
/// gate passes (`_validate_hotbar`), so `/start` succeeds without a
/// configured trifecta.
const HOTBAR_BOUND_CONFIG: &str = r#"{"hotbar_hooks_enabled": true, "hotbar": {"1": 7}}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracking_producer_lifecycle_and_suggestions_serve_natively() {
    let (state, _dir) = serve_producer_substrate(HOTBAR_BOUND_CONFIG).await;

    // ── start: 200 with the lifecycle acknowledgement (plain, no ETag) ──
    let (status, headers, body) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(
        !headers.contains_key(http::header::ETAG),
        "start replies plain (POST is outside the ETag middleware)"
    );
    let started = body_json(&body);
    let session_id = started["session_id"]
        .as_str()
        .expect("session_id is a string")
        .to_string();
    assert!(!session_id.is_empty());
    assert_eq!(started["status"], "active");
    assert!(started["started_at"].as_str().is_some());

    // ── start again while active: 409 "Session already active" ──
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::CONFLICT);
    assert_eq!(body_json(&body)["detail"], "Session already active");

    // ── stop: 200 with the stop acknowledgement, same session id ──
    let (status, headers, body) = send_json(&state, "POST", "/api/tracking/stop", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(!headers.contains_key(http::header::ETAG));
    let stopped = body_json(&body);
    assert_eq!(stopped["session_id"], session_id);
    assert!(stopped["started_at"].as_str().is_some());
    assert!(stopped["ended_at"].as_str().is_some());
    assert_eq!(stopped["kill_count"], 0);

    // ── stop with no active session: 409 "No active session" ──
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/stop", "").await;
    assert_eq!(status, http::StatusCode::CONFLICT);
    assert_eq!(body_json(&body)["detail"], "No active session");

    // ── manual-mob-suggestions success: ETag-scoped 200 over the
    //    catalogue (mob mode -> no tag-mode gate) ──
    let (status, headers, body) = get(&state, "/api/tracking/manual-mob-suggestions?q=atrox").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(
        headers.contains_key(http::header::ETAG),
        "the 200 suggestions leg is ETag-scoped (a GET under /api/tracking)"
    );
    let suggestions = body_json(&body);
    let displays: Vec<&str> = suggestions
        .as_array()
        .expect("array")
        .iter()
        .map(|row| row["display"].as_str().unwrap())
        .collect();
    assert_eq!(displays, ["Old Atrox", "Young Atrox"]);
    assert_eq!(suggestions[0]["species"], "Atrox");
    assert_eq!(suggestions[0]["maturity"], "Old");

    // ── the empty-q short-circuit: 200 [], still ETag-scoped ──
    let (status, headers, body) = get(&state, "/api/tracking/manual-mob-suggestions?q=").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(headers.contains_key(http::header::ETAG));
    assert_eq!(body, b"[]");
    // No `q` at all behaves the same.
    let (status, _, body) = get(&state, "/api/tracking/manual-mob-suggestions").await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body, b"[]");

    // ── the limit clamp: limit=99 clamps to 20 (here only 2 rows exist,
    //    so both surface); limit=0 clamps to 1 (one row) ──
    let (status, _, body) = get(
        &state,
        "/api/tracking/manual-mob-suggestions?q=atrox&limit=99",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body).as_array().unwrap().len(), 2);
    let (status, _, body) = get(
        &state,
        "/api/tracking/manual-mob-suggestions?q=atrox&limit=0",
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body).as_array().unwrap().len(), 1);

    // ── 422: an unparseable limit (the adapter's int_parsing envelope) ──
    let (status, _, body) = get(&state, "/api/tracking/manual-mob-suggestions?q=a&limit=abc").await;
    assert_eq!(status, http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_types(&body), ["int_parsing"]);

    // ── conditional GET: the suggestions 200 earns a 304 on its ETag ──
    let (_, headers, _) = get(&state, "/api/tracking/manual-mob-suggestions?q=atrox").await;
    let etag = headers
        .get(http::header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let (status, _, body) = request(
        &state,
        "GET",
        "/api/tracking/manual-mob-suggestions?q=atrox",
        &[("if-none-match", etag.as_str())],
    )
    .await;
    assert_eq!(status, http::StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracking_start_rejects_an_unready_attribution() {
    // No hotbar, no configured trifecta (the default-preset slots are
    // null): the attribution gate fails with the trifecta 400.
    let (state, _dir) = serve_producer_substrate("{}").await;
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(&body)["detail"],
        "Trifecta attribution requires a configured small weapon, big weapon, and healing tool"
    );
    // Hotbar mode with NO bound slot: the hotbar-specific 400.
    let (state, _dir) = serve_producer_substrate(r#"{"hotbar_hooks_enabled": true}"#).await;
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(&body)["detail"],
        "Bind at least one hotbar slot in the Equipment page before tracking."
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_mob_suggestions_tag_mode_409_precedes_the_empty_q_shortcut() {
    // Idle tag mode: the live config's `mob_tracking_mode == "tag"` gates
    // BEFORE the empty-q short-circuit, so even `q=` 409s (not []).
    let (state, _dir) =
        serve_producer_substrate(r#"{"mob_tracking_mode": "tag", "mob_tracking_tag": "Boss"}"#)
            .await;
    for path in [
        "/api/tracking/manual-mob-suggestions?q=atrox",
        "/api/tracking/manual-mob-suggestions?q=",
        "/api/tracking/manual-mob-suggestions",
    ] {
        let (status, headers, body) = get(&state, path).await;
        assert_eq!(status, http::StatusCode::CONFLICT, "{path}");
        assert_eq!(
            body_json(&body)["detail"],
            "Tag mode disables manual mob selection",
            "{path}"
        );
        assert!(
            !headers.contains_key(http::header::ETAG),
            "the 409 leg is non-2xx, so it carries no ETag: {path}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn producer_routes_answer_503_without_a_composed_tracker() {
    // The read-only harness composes hydration but NO tracker, so the three
    // producer routes hit the defensive service-unavailable floor (503),
    // proving the adapters require the live tracker. The production binary
    // publishes the router only once every service is composed, so this floor
    // is unreached on the normal startup path.
    let (state, _dir) = serve_substrate().await;
    let (status, _, _) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
    let (status, _, _) = send_json(&state, "POST", "/api/tracking/stop", "").await;
    assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
    let (status, _, _) = get(&state, "/api/tracking/manual-mob-suggestions?q=a").await;
    assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
}

/// Read the substrate's `settings.json` (the config-write target) as JSON.
fn read_settings(dir: &std::path::Path) -> Value {
    let raw = std::fs::read_to_string(dir.join("settings.json")).expect("settings.json reads");
    serde_json::from_str(&raw).expect("settings.json parses")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_write_routes_serve_natively_idle_mob_mode() {
    // Mob-mode config, no active session. Drives the native config-write
    // handlers (over the composed ConfigService + skill tracker) and pins
    // their responses + the settings.json they persist; the dead proxy
    // (port 9) means any handler that fell back would 502 instead.
    let (state, dir) = serve_producer_substrate("{}").await;

    // manual-mob-lock: a catalogue match locks; the selection persists.
    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/manual-mob-lock",
        r#"{"species": "Atrox", "maturity": "Old"}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(
        body_json(&body),
        json!({"mobName": "Old Atrox", "species": "Atrox", "maturity": "Old"})
    );
    let cfg = read_settings(dir.path());
    assert_eq!(cfg["manual_mob_species"], "Atrox");
    assert_eq!(cfg["manual_mob_maturity"], "Old");

    // A mob absent from the catalogue is the 400.
    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/manual-mob-lock",
        r#"{"species": "Notamob", "maturity": ""}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(
        body_json(&body)["detail"],
        "Mob is not present in the catalogue"
    );

    // tag-lock outside tag mode is the 409.
    let (status, _, body) =
        send_json(&state, "POST", "/api/tracking/tag-lock", r#"{"tag": "X"}"#).await;
    assert_eq!(status, http::StatusCode::CONFLICT);
    assert_eq!(body_json(&body)["detail"], "Tag mode is not enabled");

    // release-mob in idle manual mode returns the stored display and clears it.
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/release-mob", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body), json!({"released": "Old Atrox"}));
    let cfg = read_settings(dir.path());
    assert_eq!(cfg["manual_mob_species"], "");
    assert_eq!(cfg["manual_mob_maturity"], "");

    // release-mob again with nothing stored: released is null.
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/release-mob", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body), json!({ "released": Value::Null }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_write_routes_serve_natively_idle_tag_mode() {
    // Tag-mode config, no active session: the tag-lock success/empty legs,
    // the manual-mob-lock tag-mode 409, and the idle-tag release branch.
    let (state, dir) = serve_producer_substrate(r#"{"mob_tracking_mode": "tag"}"#).await;

    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/tag-lock",
        r#"{"tag": "Daily Hunt"}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body), json!({"tag": "Daily Hunt"}));
    assert_eq!(read_settings(dir.path())["mob_tracking_tag"], "Daily Hunt");

    // An all-whitespace tag is the 400.
    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/tag-lock",
        r#"{"tag": "   "}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["detail"], "Tag cannot be empty");

    // manual-mob-lock is disabled in tag mode: the 409.
    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/manual-mob-lock",
        r#"{"species": "Atrox", "maturity": ""}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::CONFLICT);
    assert_eq!(
        body_json(&body)["detail"],
        "Tag mode disables manual mob selection"
    );

    // release-mob in idle tag mode returns the trimmed tag and clears it.
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/release-mob", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body), json!({"released": "Daily Hunt"}));
    assert_eq!(read_settings(dir.path())["mob_tracking_tag"], "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_write_routes_serve_natively_active_session() {
    // An active mob-mode session: the tracker in-memory calls fire, the
    // tag-lock active-session 409 leg, and release-mob's active branch
    // (clears the manual selection, not the tag).
    let (state, dir) = serve_producer_substrate(HOTBAR_BOUND_CONFIG).await;
    let (status, _, _) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::OK);

    // manual-mob-lock while tracking: 200, sets the live tracker + config.
    let (status, _, body) = send_json(
        &state,
        "POST",
        "/api/tracking/manual-mob-lock",
        r#"{"species": "Atrox", "maturity": "Young"}"#,
    )
    .await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(body_json(&body)["mobName"], "Young Atrox");
    assert_eq!(read_settings(dir.path())["manual_mob_species"], "Atrox");

    // tag-lock against a mob-mode active session: the session-snapshot 409.
    let (status, _, body) =
        send_json(&state, "POST", "/api/tracking/tag-lock", r#"{"tag": "X"}"#).await;
    assert_eq!(status, http::StatusCode::CONFLICT);
    assert_eq!(
        body_json(&body)["detail"],
        "Active session is not in tag mode"
    );

    // release-mob in an active non-tag session clears the MANUAL selection
    // (the active-non-tag branch), not the tag.
    let (status, _, _) = send_json(&state, "POST", "/api/tracking/release-mob", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert_eq!(
        read_settings(dir.path())["manual_mob_species"],
        "",
        "an active mob-mode release clears the manual selection, not the tag"
    );
}

/// The repair-cost read runs the OCR provider chain and gates on the live
/// `repair_ocr_enabled` flag (the reference's 400 when off). A plain 200
/// (POST, outside the ETag scope) carrying the declared fields in model order.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repair_scan_serves_and_gates_on_the_config_flag() {
    let (state, _dir) = serve_producer_substrate(r#"{"repair_ocr_enabled": true}"#).await;
    let (status, headers, body) =
        send_json(&state, "POST", "/api/tracking/session/abc/repair-scan", "").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(
        !headers.contains_key(http::header::ETAG),
        "POST is outside the ETag scope"
    );
    let result = body_json(&body);
    assert_eq!(result["cost_ped"], 2.2);
    assert_eq!(result["raw_text"], "2,20 PED");
    assert_eq!(result["confidence"], 0.97);
    let keys: Vec<&str> = result
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["cost_ped", "raw_text", "confidence"],
        "success carries the declared fields only, no null error key"
    );

    let (state, _dir) = serve_producer_substrate(r#"{"repair_ocr_enabled": false}"#).await;
    let (status, _, body) =
        send_json(&state, "POST", "/api/tracking/session/abc/repair-scan", "").await;
    assert_eq!(status, http::StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["detail"], "Repair OCR is disabled");
}

/// The dashboard snapshot serves both states under the conditional-GET
/// contract, each keeping its own polymorphic shape in the model's
/// declaration order (the snake-case status trio among the camelCase numbers).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracking_snapshot_serves_idle_and_active() {
    // ── idle, trifecta-mode (the default preset exists, nothing bound) ──
    let (state, _dir) = serve_producer_substrate(r#"{"hotbar_hooks_enabled": false}"#).await;
    let (status, headers, body) = get(&state, "/api/tracking/snapshot").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(
        headers.contains_key(http::header::ETAG),
        "the snapshot GET carries the conditional-GET contract"
    );
    let idle = body_json(&body);
    assert_eq!(idle["status"], "idle");
    assert_eq!(idle["hotbarListenerActive"], false);
    assert_eq!(idle["weaponAttribution"], "trifecta");
    assert_eq!(idle["recentEvents"], serde_json::json!([]));
    let keys: Vec<&str> = idle
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "status",
            "hotbarListenerActive",
            "weaponAttribution",
            "repairOcrEnabled",
            "endOfSessionArmourReminderEnabled",
            "mobEntryMode",
            "currentMob",
            "mobSource",
            "currentTool",
            "trifectaAttribution",
            "recentEvents",
        ]
    );
    // The trifecta summary populates (a default preset exists) with nothing
    // bound, in its own insertion order.
    let summary = &idle["trifectaAttribution"];
    assert_eq!(summary["smallWeapon"], serde_json::Value::Null);
    assert!(summary["presets"].is_array());
    let summary_keys: Vec<&str> = summary
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        summary_keys,
        [
            "activePresetId",
            "presetName",
            "presets",
            "smallWeapon",
            "bigWeapon",
            "healTool",
        ]
    );

    // ── active, hotbar-mode (a started session) ──
    let (state, _dir) = serve_producer_substrate(HOTBAR_BOUND_CONFIG).await;
    let (status, _, body) = send_json(&state, "POST", "/api/tracking/start", "").await;
    assert_eq!(status, http::StatusCode::OK);
    let session_id = body_json(&body)["session_id"]
        .as_str()
        .expect("session id")
        .to_string();

    let (status, headers, body) = get(&state, "/api/tracking/snapshot").await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(headers.contains_key(http::header::ETAG));
    let active = body_json(&body);
    assert_eq!(active["status"], "active");
    assert_eq!(active["session_id"], session_id);
    assert_eq!(active["kill_count"], 0);
    assert_eq!(active["hotbarListenerActive"], false);
    assert_eq!(active["weaponAttribution"], "hotbar");
    assert_eq!(active["trifectaAttribution"], serde_json::Value::Null);
    assert_eq!(active["recentEvents"], serde_json::json!([]));
    assert_eq!(active["warnings"], serde_json::json!([]));
    // The full polymorphic shape in the model's declaration order: status,
    // then the shared envelope, then the active-only block ending in warnings.
    let keys: Vec<&str> = active
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "status",
            "hotbarListenerActive",
            "weaponAttribution",
            "repairOcrEnabled",
            "endOfSessionArmourReminderEnabled",
            "mobEntryMode",
            "currentMob",
            "mobSource",
            "currentTool",
            "trifectaAttribution",
            "recentEvents",
            "session_id",
            "started_at",
            "kill_count",
            "elapsed",
            "cost",
            "returns",
            "pes",
            "net",
            "returnRate",
            "damageDealtTotal",
            "weaponDamageDealt",
            "weaponCost",
            "shotsFiredTotal",
            "criticalHitsTotal",
            "maxDamage",
            "globalsCount",
            "hofsCount",
            "latestKillLoot",
            "multiplierLast",
            "multiplierAvg",
            "multiplierMax",
            "multiplierHistory",
            "cumulativeNetHistory",
            "warnings",
        ]
    );
}
