//! Native route adapters: the HTTP-request face of the proven
//! [`hydration`](crate::hydration) handlers.
//!
//! The live tracking families have migrated onto typed IPC commands
//! (`eo_api::tracking`, ADR-0019); what remains here is the guide-mode demo
//! read namespace (`/api/demo/*`). Each adapter extracts its route's
//! parameters, resolves the lazily-built demo state, and returns its response,
//! answering a defensive `service_unavailable` 503 when the demo state is not
//! yet available.

use std::sync::Arc;

use crate::extract::{decode_path_segment, QueryString};
use crate::{arm_routed, service_unavailable, AppState};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, Response, StatusCode};
use axum::routing::MethodFilter;
use axum::Router;

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
/// route. The raw segment percent-decodes before the lookup; a decoded slash
/// reproduces the backend's route-level 404 (matching precedes the handler).
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

/// Register the guide-mode demo read routes; one `arm_routed` line per route.
/// The live tracking families have migrated onto typed IPC commands
/// (ADR-0019), leaving only the `/api/demo/*` read namespace served here.
pub(crate) fn register(router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    router
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
}
