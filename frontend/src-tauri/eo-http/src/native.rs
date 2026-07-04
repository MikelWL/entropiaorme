//! Native route adapters: the HTTP-request face of the proven
//! [`hydration`](crate::hydration) handlers.
//!
//! Each adapter extracts its route's parameters through
//! [`extract`](crate::extract) (reproducing the backend's validation
//! envelopes), calls the corresponding handler, and returns its
//! response. The shell publishes the router only after composition has
//! installed every service, so an adapter whose service is somehow absent
//! returns a defensive `service_unavailable`
//! 503 rather than the retired proxy fallback.

use std::sync::Arc;

use crate::body::{self, internal_server_error};
use crate::extract::{decode_path_segment, query_int_or_default, QueryString, Validation};
use crate::hydration::internal_error;
use crate::{arm_routed, service_unavailable, AppState};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, Response, StatusCode};
use axum::routing::MethodFilter;
use axum::Router;

/// `If-None-Match`, as the backend's middleware reads it (a non-UTF-8
/// value reads as absent).
fn if_none_match(req: &Request) -> Option<String> {
    req.headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The framework-level 404 the backend serves when no route matches
/// (a percent-encoded slash inside a path parameter de-matches the
/// route there, because the backend decodes the path before matching).
fn router_not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{\"detail\":\"Not Found\"}"))
        .expect("static 404 builds")
}

macro_rules! simple_get {
    ($fn_name:ident, $handler:ident) => {
        async fn $fn_name(state: Arc<AppState>, req: Request) -> Response<Body> {
            let Some(hydration) = state.hydration() else {
                return service_unavailable();
            };
            let inm = if_none_match(&req);
            hydration.$handler(inm.as_deref()).await
        }
    };
}

/// Split a request into its content type and collected body bytes.
///
/// A transport-level read failure answers the backend's unhandled-error
/// 500: the reference never reaches the handler when the body cannot be
/// read, so nothing may be written from a partial payload (an empty
/// fallback would make optional-body routes proceed with defaults).
async fn body_parts(req: Request) -> Result<(Option<String>, Vec<u8>), Box<Response<Body>>> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => Ok((content_type, bytes.to_vec())),
        Err(_) => Err(Box::new(internal_server_error())),
    }
}

/// Read the request body to its VALUE level, answering the standalone
/// reply forms (the scanner's envelope, the encoding/depth 400s, the
/// render 500) that the backend never aggregates with path issues.
async fn standalone_body_value(req: Request) -> Result<body::BodyValue, Box<Response<Body>>> {
    let (content_type, bytes) = body_parts(req).await?;
    let mut value_validation = Validation::new();
    match body::read_body_value(content_type.as_deref(), &bytes, &mut value_validation) {
        Some(value) => Ok(value),
        None => Err(Box::new(value_validation.into_response())),
    }
}

// ── Repair-scan adapter (scan_manual.py) ───────────────────────────────
//
// The one-shot repair-cost read serves over the composed `RepairOcrService`
// (always constructed; off Windows the OCR runtime is absent, so it reports
// "engine unavailable" but still serves). It answers the 503
// service-unavailable floor only when composition was declined. The manual
// skill-scan family (status, capture, undo, and their kin) moved to the
// typed-command facade (`eo_api::scan`); only the tracking repair-scan leg
// remains here, with its projection helper in `scan_routes`.

/// POST /api/tracking/session/{session_id}/repair-scan: the `session_id` is
/// routing-only (the reference ignores it), so a decoded slash still
/// de-matches (the framework 404) but the value is unused. Gated on the live
/// `repair_ocr_enabled` config flag (400 when off).
async fn tracking_repair_scan(state: Arc<AppState>, req: Request) -> Response<Body> {
    let (Some(repair), Some(config)) = (state.repair_ocr(), state.config_service()) else {
        return service_unavailable();
    };
    if let Err(reply) = string_path_id(session_id_segment(req.uri().path(), "/repair-scan")) {
        return *reply;
    }
    let enabled = {
        let Ok(guard) = config.lock() else {
            return internal_error();
        };
        guard.get().repair_ocr_enabled
    };
    crate::scan_routes::repair_scan(&repair, enabled)
}

// ── Tracking session-read adapters ──────────────────────────────────

simple_get!(tracking_sessions, tracking_sessions);

/// GET /api/tracking/session/{session_id}: the one path-parameter route of
/// this surface. The raw segment percent-decodes before the lookup; a
/// decoded slash reproduces the backend's route-level 404 (matching precedes
/// the handler, exactly as the codex-ranks adapter handles it).
async fn tracking_session(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let raw = req
        .uri()
        .path()
        .strip_prefix("/api/tracking/session/")
        .unwrap_or_default();
    let session_id = match string_path_id(raw) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let inm = if_none_match(&req);
    hydration
        .tracking_session(&session_id, inm.as_deref())
        .await
}

// ── Guide-mode demo adapters (`/api/demo/*`) ────────────────────────
//
// The demo serves a curated read-only dataset over a parallel hydration +
// tracker built on a writable clone of the bundled demo DB (see [`crate::demo`]).
// Each adapter resolves the lazily-built demo state, answering the 503
// service-unavailable floor when it is unavailable (no native composition, no
// bundled demo DB, or a build failure). The demo prefix is outside the ETag
// middleware, so every
// reply is a plain JSON 200 (the demo state's methods enforce that).

async fn demo_analytics_overview(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    let query = QueryString::parse(req.uri().query());
    let period = query.last("period").unwrap_or("all").to_string();
    demo.analytics_overview(&period).await
}

async fn demo_analytics_activity(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    demo.analytics_activity().await
}

async fn demo_analytics_ledger(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    let query = QueryString::parse(req.uri().query());
    let limit = query.last("limit").and_then(|raw| raw.parse::<i64>().ok());
    demo.list_ledger(query.last("cursor"), limit).await
}

async fn demo_analytics_ledger_presets(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    demo.list_ledger_presets().await
}

async fn demo_analytics_inventory(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    demo.list_inventory().await
}

async fn demo_tracking_sessions(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    demo.list_sessions().await
}

/// GET /api/demo/tracking/session/{session_id}: the demo's one path-parameter
/// route, decoded exactly as the live [`tracking_session`] adapter.
async fn demo_tracking_session(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    let raw = req
        .uri()
        .path()
        .strip_prefix("/api/demo/tracking/session/")
        .unwrap_or_default();
    let session_id = match string_path_id(raw) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    demo.get_session(&session_id).await
}

async fn demo_tracking_snapshot(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let Some(demo) = crate::demo::ensure_demo(&state).await else {
        return service_unavailable();
    };
    demo.tracking_snapshot().await
}

/// GET /api/tracking/tag-suggestions?q=&limit=: `q` defaults to the empty
/// string (short-circuiting to `[]`), `limit` to 10 (clamped to 1..=20 in
/// the handler). An unparseable `limit` is the backend's 422 int_parsing.
async fn tracking_tag_suggestions(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let query = QueryString::parse(req.uri().query());
    let q = query.last("q").unwrap_or("").to_string();
    let mut validation = Validation::new();
    let limit = query_int_or_default(&mut validation, &query, "limit", 10);
    if !validation.is_ok() {
        return validation.into_response();
    }
    let inm = if_none_match(&req);
    hydration
        .tracking_tag_suggestions(&q, limit.expect("validated"), inm.as_deref())
        .await
}

// ── Tracking producer adapters (live-tracker spine) ─────────────────
//
// These three reach a DIFFERENT dependency than the session-read
// adapters above: the live `Arc<HuntTracker>` (start/stop, and the
// manual-mob tag-mode gate) plus the bundled mobs catalogue (the
// suggestions). They answer the 503 service-unavailable floor unless BOTH the
// tracker and the read surface are composed (the suggestions handler reads the
// catalogue through the hydration state), exactly as the read surface answers
// the floor without `with_hydration`.

/// POST /api/tracking/start
async fn tracking_start(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let (Some(hydration), Some(tracker)) = (state.hydration(), state.tracker()) else {
        return service_unavailable();
    };
    hydration.tracking_start(&tracker).await
}

/// POST /api/tracking/stop
async fn tracking_stop(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let (Some(hydration), Some(tracker)) = (state.hydration(), state.tracker()) else {
        return service_unavailable();
    };
    hydration.tracking_stop(&tracker).await
}

/// GET /api/tracking/manual-mob-suggestions?q=&limit=: `q` defaults to the
/// empty string (the tag-mode 409 still precedes the `[]` short-circuit),
/// `limit` to 10 (clamped to 1..=20 in the handler). An unparseable `limit`
/// is the backend's 422 int_parsing.
async fn tracking_manual_mob_suggestions(state: Arc<AppState>, req: Request) -> Response<Body> {
    let (Some(hydration), Some(tracker)) = (state.hydration(), state.tracker()) else {
        return service_unavailable();
    };
    let query = QueryString::parse(req.uri().query());
    let q = query.last("q").unwrap_or("").to_string();
    let mut validation = Validation::new();
    let limit = query_int_or_default(&mut validation, &query, "limit", 10);
    if !validation.is_ok() {
        return validation.into_response();
    }
    let inm = if_none_match(&req);
    hydration
        .tracking_manual_mob_suggestions(&tracker, &q, limit.expect("validated"), inm.as_deref())
        .await
}

/// POST /api/tracking/release-mob: clear the locked mob or tag (empty body).
async fn tracking_release_mob(state: Arc<AppState>, _req: Request) -> Response<Body> {
    let (Some(hydration), Some(config), Some(tracker)) =
        (state.hydration(), state.config_service(), state.tracker())
    else {
        return service_unavailable();
    };
    hydration.release_mob(&config, &tracker).await
}

/// POST /api/tracking/manual-mob-lock: `{species, maturity?}`.
async fn tracking_manual_mob_lock(state: Arc<AppState>, req: Request) -> Response<Body> {
    let (Some(hydration), Some(config), Some(tracker)) =
        (state.hydration(), state.config_service(), state.tracker())
    else {
        return service_unavailable();
    };
    let (content_type, bytes) = match body_parts(req).await {
        Ok(parts) => parts,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::read_object(content_type.as_deref(), &bytes, &mut v) else {
        return v.into_response();
    };
    let species = body::required_str(&mut v, &object, "species");
    let maturity = body::str_or_default(&mut v, &object, "maturity", "");
    if !v.is_ok() {
        return v.into_response();
    }
    hydration
        .manual_mob_lock(
            &config,
            &tracker,
            &species.expect("validated"),
            &maturity.expect("validated"),
        )
        .await
}

/// POST /api/tracking/tag-lock: `{tag}`.
async fn tracking_tag_lock(state: Arc<AppState>, req: Request) -> Response<Body> {
    let (Some(hydration), Some(config), Some(tracker)) =
        (state.hydration(), state.config_service(), state.tracker())
    else {
        return service_unavailable();
    };
    let (content_type, bytes) = match body_parts(req).await {
        Ok(parts) => parts,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::read_object(content_type.as_deref(), &bytes, &mut v) else {
        return v.into_response();
    };
    let tag = body::required_str(&mut v, &object, "tag");
    if !v.is_ok() {
        return v.into_response();
    }
    hydration
        .tag_lock(
            &config,
            &tracker,
            &tag.expect("validated"),
            v.binding_taint(),
        )
        .await
}

/// GET /api/tracking/snapshot: the consolidated dashboard hydration, over the
/// live tracker, the config, and the hotbar listener's running state. Answers
/// the 503 service-unavailable floor unless all three are composed (the
/// snapshot reads each).
async fn tracking_snapshot(state: Arc<AppState>, req: Request) -> Response<Body> {
    let (Some(hydration), Some(tracker), Some(hotbar)) =
        (state.hydration(), state.tracker(), state.hotbar_listener())
    else {
        return service_unavailable();
    };
    let inm = if_none_match(&req);
    hydration
        .tracking_snapshot(&tracker, &hotbar, inm.as_deref())
        .await
}

// ── Tracking session-edit write adapters ──────────────────────────────

/// The `{session_id}` of a `/api/tracking/session/{session_id}/<suffix>`
/// edit route. A percent-encoded slash de-matches (the framework 404),
/// exactly as the single-segment string path-id rule elsewhere.
fn session_id_segment<'p>(path: &'p str, suffix: &str) -> &'p str {
    path.strip_prefix("/api/tracking/session/")
        .and_then(|rest| rest.strip_suffix(suffix))
        .unwrap_or_default()
}

/// POST /api/tracking/session/{session_id}/rename-mob
async fn tracking_rename_mob(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let session_id = match string_path_id(session_id_segment(req.uri().path(), "/rename-mob")) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let body_value = match standalone_body_value(req).await {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::object_from_body(body_value, &mut v) else {
        return v.into_response();
    };
    // RenameMobRequest declaration order: fromMobName, then toMobName.
    let from_mob = body::required_str(&mut v, &object, "fromMobName");
    let to_mob = body::required_str(&mut v, &object, "toMobName");
    if !v.is_ok() {
        return v.into_response();
    }
    if v.binding_taint() {
        return internal_server_error();
    }
    hydration
        .tracking_rename_mob(
            &session_id,
            &from_mob.expect("validated"),
            &to_mob.expect("validated"),
        )
        .await
}

/// POST /api/tracking/session/{session_id}/restore-mob
async fn tracking_restore_mob(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let session_id = match string_path_id(session_id_segment(req.uri().path(), "/restore-mob")) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let body_value = match standalone_body_value(req).await {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::object_from_body(body_value, &mut v) else {
        return v.into_response();
    };
    let current_mob = body::required_str(&mut v, &object, "currentMobName");
    if !v.is_ok() {
        return v.into_response();
    }
    if v.binding_taint() {
        return internal_server_error();
    }
    hydration
        .tracking_restore_mob(&session_id, &current_mob.expect("validated"))
        .await
}

/// The `{session_id}` and `{item_name:path}` of a loot-flip route. The
/// item segment is a FastAPI `:path` converter: it CONTAINS slashes
/// (raw or percent-encoded), so a decoded slash is KEPT rather than
/// turned into a 404 (the single-segment rule). The session id stays a
/// single segment, so its own decoded slash still de-matches.
fn loot_flip_segments(path: &str, suffix: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/tracking/session/")?;
    let rest = rest.strip_suffix(suffix)?;
    // rest is `{session_id}/loot-item/{item_name:path}`; the session id
    // is the first segment, the item name is everything after the
    // `/loot-item/` marker (slashes included).
    let (session_raw, after) = rest.split_once('/')?;
    let item_raw = after.strip_prefix("loot-item/")?;
    let session_id = decode_path_segment(session_raw);
    if session_id.contains('/') {
        return None;
    }
    Some((session_id, decode_path_segment(item_raw)))
}

/// POST /api/tracking/session/{session_id}/loot-item/{item_name:path}/{deactivate|activate}
///
/// One adapter for both flip directions: axum's catch-all (`{*rest}`)
/// must be terminal, so the two suffix-distinguished FastAPI routes land
/// on a single wildcard registration here, dispatched on the trailing
/// `/deactivate` or `/activate` segment. A tail matching neither suffix
/// is the framework 404 (no FastAPI route would have matched it either).
async fn tracking_loot_item_flip(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let path = req.uri().path();
    if let Some((session_id, item_name)) = loot_flip_segments(path, "/deactivate") {
        hydration
            .tracking_deactivate_loot_item(&session_id, &item_name)
            .await
    } else if let Some((session_id, item_name)) = loot_flip_segments(path, "/activate") {
        hydration
            .tracking_activate_loot_item(&session_id, &item_name)
            .await
    } else {
        router_not_found()
    }
}

/// POST /api/tracking/session/{session_id}/armour-cost
async fn tracking_armour_cost(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let session_id = match string_path_id(session_id_segment(req.uri().path(), "/armour-cost")) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let body_value = match standalone_body_value(req).await {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::object_from_body(body_value, &mut v) else {
        return v.into_response();
    };
    let cost = body::required_f64(&mut v, &object, "cost");
    if !v.is_ok() {
        return v.into_response();
    }
    if v.binding_taint() {
        return internal_server_error();
    }
    hydration
        .tracking_set_armour_cost(&session_id, cost.expect("validated"))
        .await
}

/// GET /api/tracking/session/{session_id}/quest-link-suggestion: the
/// curated post-session linkage suggestion, under the conditional-GET
/// contract. A decoded slash in the session id de-matches (framework
/// 404), exactly as the other single-segment session routes.
async fn tracking_quest_link_suggestion(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let session_id = match string_path_id(session_id_segment(
        req.uri().path(),
        "/quest-link-suggestion",
    )) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let inm = if_none_match(&req);
    hydration
        .session_quest_link_suggestion(&session_id, inm.as_deref())
        .await
}

/// POST /api/tracking/session/{session_id}/quest-link: persist the
/// accept/decline decision. The body model (`SessionQuestLinkDecisionBody`)
/// requires a string `action`; validation precedes the route logic, so a
/// missing action is the 422 the framework raises before the handler's 404.
async fn tracking_quest_link_decide(state: Arc<AppState>, req: Request) -> Response<Body> {
    let Some(hydration) = state.hydration() else {
        return service_unavailable();
    };
    let session_id = match string_path_id(session_id_segment(req.uri().path(), "/quest-link")) {
        Ok(id) => id,
        Err(reply) => return *reply,
    };
    let body_value = match standalone_body_value(req).await {
        Ok(value) => value,
        Err(reply) => return *reply,
    };
    let mut v = Validation::new();
    let Some(object) = body::object_from_body(body_value, &mut v) else {
        return v.into_response();
    };
    let action = body::required_str(&mut v, &object, "action");
    if !v.is_ok() {
        return v.into_response();
    }
    if v.binding_taint() {
        return internal_server_error();
    }
    hydration
        .decide_session_quest_link(&session_id, &action.expect("validated"))
        .await
}

// ── Shared string path-id decoding ──

/// Decode a string path-id; a percent-encoded slash de-matches the route
/// (the backend decodes before matching), reproducing its framework 404.
fn string_path_id(raw_segment: &str) -> Result<String, Box<Response<Body>>> {
    let decoded = decode_path_segment(raw_segment);
    if decoded.contains('/') {
        return Err(Box::new(router_not_found()));
    }
    Ok(decoded)
}

/// Register the natively-served tracking, scan, and remaining hydration
/// routes; one `arm_routed` line per route. Every route is served
/// in-process; there is no runtime override left to consult. Route
/// families migrate off here onto typed IPC commands over time (ADR-0019);
/// the quests + playlists and analytics families have moved to `eo-api`.
pub(crate) fn register(router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    router
        .route(
            "/api/tracking/sessions",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/sessions",
                tracking_sessions,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/session/{session_id}",
                tracking_session,
            ),
        )
        .route(
            "/api/tracking/tag-suggestions",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/tag-suggestions",
                tracking_tag_suggestions,
            ),
        )
        .route(
            "/api/tracking/start",
            arm_routed(MethodFilter::POST, "/api/tracking/start", tracking_start),
        )
        .route(
            "/api/tracking/stop",
            arm_routed(MethodFilter::POST, "/api/tracking/stop", tracking_stop),
        )
        .route(
            "/api/tracking/manual-mob-suggestions",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/manual-mob-suggestions",
                tracking_manual_mob_suggestions,
            ),
        )
        .route(
            "/api/tracking/snapshot",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/snapshot",
                tracking_snapshot,
            ),
        )
        // Guide-mode demo read namespace: the eight
        // GETs the guide retargets analytics/tracking reads onto.
        .route(
            "/api/demo/analytics/overview",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/analytics/overview",
                demo_analytics_overview,
            ),
        )
        .route(
            "/api/demo/analytics/activity",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/analytics/activity",
                demo_analytics_activity,
            ),
        )
        .route(
            "/api/demo/analytics/ledger",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/analytics/ledger",
                demo_analytics_ledger,
            ),
        )
        .route(
            "/api/demo/analytics/ledger/presets",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/analytics/ledger/presets",
                demo_analytics_ledger_presets,
            ),
        )
        .route(
            "/api/demo/analytics/inventory",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/analytics/inventory",
                demo_analytics_inventory,
            ),
        )
        .route(
            "/api/demo/tracking/sessions",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/tracking/sessions",
                demo_tracking_sessions,
            ),
        )
        .route(
            "/api/demo/tracking/session/{session_id}",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/tracking/session/{session_id}",
                demo_tracking_session,
            ),
        )
        .route(
            "/api/demo/tracking/snapshot",
            arm_routed(
                MethodFilter::GET,
                "/api/demo/tracking/snapshot",
                demo_tracking_snapshot,
            ),
        )
        .route(
            "/api/tracking/release-mob",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/release-mob",
                tracking_release_mob,
            ),
        )
        .route(
            "/api/tracking/manual-mob-lock",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/manual-mob-lock",
                tracking_manual_mob_lock,
            ),
        )
        .route(
            "/api/tracking/tag-lock",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/tag-lock",
                tracking_tag_lock,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/rename-mob",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/rename-mob",
                tracking_rename_mob,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/restore-mob",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/restore-mob",
                tracking_restore_mob,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/loot-item/{*item_action}",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/loot-item/{*item_action}",
                tracking_loot_item_flip,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/armour-cost",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/armour-cost",
                tracking_armour_cost,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/quest-link-suggestion",
            arm_routed(
                MethodFilter::GET,
                "/api/tracking/session/{session_id}/quest-link-suggestion",
                tracking_quest_link_suggestion,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/quest-link",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/quest-link",
                tracking_quest_link_decide,
            ),
        )
        .route(
            "/api/tracking/session/{session_id}/repair-scan",
            arm_routed(
                MethodFilter::POST,
                "/api/tracking/session/{session_id}/repair-scan",
                tracking_repair_scan,
            ),
        )
}
