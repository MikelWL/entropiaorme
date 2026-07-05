//! Developer-mode routes: an in-process metrics snapshot and the
//! crash-reporting opt-in toggle.
//!
//! Every route is self-gated on developer mode (404 when off), so they are off
//! the equivalence-covered surface (no golden, no Python arm to diff against)
//! and absent from a default install. They are native-only: they have no
//! counterpart in the Python reference, so they never had a route-override
//! entry. The metrics snapshot is
//! read straight from the process-global registry; the crash-reporting toggle
//! reads and writes the shell-owned `observability.json` (NOT `settings.json`,
//! which is the dual-arm equivalence surface).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

/// Register the dev routes onto the substrate router.
pub fn register(router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    router
        .route("/api/dev/metrics", get(metrics_snapshot))
        .route(
            "/api/dev/crash-reporting",
            get(crash_reporting_status).post(set_crash_reporting),
        )
        .route("/api/dev/compact-database", post(compact_database))
        .route("/api/dev/rebuild-projections", post(rebuild_projections))
}

/// The metrics snapshot (throughput counts, latency histograms, and the
/// resource-drift gauges) as JSON. Gate-off => 404.
async fn metrics_snapshot(State(state): State<Arc<AppState>>) -> Response {
    if !state.developer_mode() {
        return not_found();
    }
    let snapshot = eo_wire::metrics::metrics().snapshot();
    match serde_json::to_string(&snapshot) {
        Ok(body) => json_response(http::StatusCode::OK, body),
        Err(_) => json_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"detail":"metrics serialisation failed"}"#.to_string(),
        ),
    }
}

/// Read the current crash-reporting opt-in. Gate-off => 404.
async fn crash_reporting_status(State(state): State<Arc<AppState>>) -> Response {
    if !state.developer_mode() {
        return not_found();
    }
    let enabled = state
        .data_dir()
        .map(eo_services::observability_config::crash_reporting_enabled)
        .unwrap_or(false);
    json_response(http::StatusCode::OK, crash_reporting_body(enabled))
}

/// Set the crash-reporting opt-in from a `{"crash_reporting_enabled": bool}`
/// body. Gate-off => 404; a malformed body => 400.
async fn set_crash_reporting(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    if !state.developer_mode() {
        return not_found();
    }
    let Some(data_dir) = state.data_dir() else {
        return not_found();
    };
    let enabled = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("crash_reporting_enabled")
                .and_then(serde_json::Value::as_bool)
        });
    let Some(enabled) = enabled else {
        return json_response(
            http::StatusCode::BAD_REQUEST,
            r#"{"detail":"expected {\"crash_reporting_enabled\": <bool>}"}"#.to_string(),
        );
    };
    match eo_services::observability_config::set_crash_reporting_enabled(data_dir, enabled) {
        Ok(()) => json_response(http::StatusCode::OK, crash_reporting_body(enabled)),
        Err(_) => json_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"detail":"could not persist the crash-reporting setting"}"#.to_string(),
        ),
    }
}

fn crash_reporting_body(enabled: bool) -> String {
    format!(r#"{{"crash_reporting_enabled":{enabled}}}"#)
}

/// Compact the database into a fresh copy via `VACUUM INTO`, reclaiming the
/// free pages that churn leaves behind. Writes `entropia_orme-compacted.db`
/// beside the live database and returns its path and byte size; the live
/// file is only read, never locked for the rewrite (off the hot path).
/// Gate-off, no data dir, or no composed database => 404.
async fn compact_database(State(state): State<Arc<AppState>>) -> Response {
    if !state.developer_mode() {
        return not_found();
    }
    let (Some(db), Some(data_dir)) = (state.hydration_db(), state.data_dir()) else {
        return not_found();
    };
    let dest = data_dir.join("entropia_orme-compacted.db");
    // VACUUM INTO refuses to overwrite an existing file; clear any prior
    // copy from an earlier compaction first.
    let _ = std::fs::remove_file(&dest);
    match db.vacuum_into(&dest).await {
        Ok(()) => {
            let bytes = std::fs::metadata(&dest).map(|meta| meta.len()).unwrap_or(0);
            let body = serde_json::json!({
                "path": dest.to_string_lossy(),
                "bytes": bytes,
            });
            json_response(http::StatusCode::OK, body.to_string())
        }
        Err(_) => json_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"detail":"database compaction failed"}"#.to_string(),
        ),
    }
}

/// Rebuild every read-model projection (daily rollups, ledger rollups,
/// session summaries) from the raw tracking tables and report whether each
/// rebuilt byte-identically to the incrementally-maintained rows: the CQRS
/// rebuildability proof, runnable on demand. A `false` in any `matched`
/// names a projection whose incremental maintenance drifted. Gate-off or no
/// composed database => 404.
async fn rebuild_projections(State(state): State<Arc<AppState>>) -> Response {
    if !state.developer_mode() {
        return not_found();
    }
    let Some(db) = state.hydration_db() else {
        return not_found();
    };
    // A maintenance action, off the equivalence surface: the wall clock sets
    // the heal watermark (yesterday). The rebuild and the incremental heal
    // share this `now`, so the equality proof is independent of its value.
    let now = eo_services::tracker::naive_to_epoch(chrono::Utc::now().naive_utc());
    match eo_services::maintenance::rebuild_and_verify(&db, now).await {
        Ok(report) => {
            let body = serde_json::json!({
                "allMatched": report.all_matched(),
                "tables": report.tables,
            });
            json_response(http::StatusCode::OK, body.to_string())
        }
        Err(_) => json_response(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"detail":"projection rebuild failed"}"#.to_string(),
        ),
    }
}

/// A 404 in the backend's `{"detail": ...}` shape, so a gate-off dev route is
/// indistinguishable from an unregistered path.
fn not_found() -> Response {
    json_response(
        http::StatusCode::NOT_FOUND,
        r#"{"detail":"Not Found"}"#.to_string(),
    )
}

fn json_response(status: http::StatusCode, body: String) -> Response {
    Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .expect("static dev-route response builds")
}

#[cfg(test)]
mod tests {
    use crate::{build_router, AppState};
    use axum::body::Body;
    use axum::extract::Request;
    use std::path::Path;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn state_with_dev_mode(dir: &Path, enabled: bool) -> Arc<AppState> {
        std::fs::write(
            dir.join("settings.json"),
            format!(r#"{{"developer_mode_enabled":{enabled}}}"#),
        )
        .unwrap();
        Arc::new(AppState::new(8421).with_data_dir(dir.to_path_buf()))
    }

    #[tokio::test]
    async fn dev_metrics_is_404_when_developer_mode_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let router = build_router(state_with_dev_mode(dir.path(), false));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/dev/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            http::StatusCode::NOT_FOUND,
            "the dev gate is off the equivalence-covered surface by default"
        );
    }

    #[tokio::test]
    async fn dev_metrics_returns_a_snapshot_when_developer_mode_is_on() {
        let dir = tempfile::tempdir().unwrap();
        // Record something so the (process-global) snapshot is non-trivial.
        eo_wire::metrics::metrics().record_event_published();
        let router = build_router(state_with_dev_mode(dir.path(), true));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/dev/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        // It deserialises as a real metrics snapshot, with the recorded event.
        let snapshot: eo_wire::metrics::MetricsSnapshot =
            serde_json::from_slice(&bytes).expect("the body is a metrics snapshot");
        assert!(snapshot.events_published >= 1);
    }

    /// A composed state with a real database, a data dir, and developer mode
    /// on: what the maintenance routes need to serve.
    async fn composed_dev_state(dir: &Path) -> Arc<AppState> {
        use crate::hydration::HydrationState;
        let db = eo_services::db::Db::open(&dir.join("entropia_orme.db"))
            .await
            .unwrap();
        let hydration = Arc::new(HydrationState::new(db));
        std::fs::write(
            dir.join("settings.json"),
            r#"{"developer_mode_enabled":true}"#,
        )
        .unwrap();
        Arc::new(
            AppState::new(8421)
                .with_hydration(hydration)
                .with_data_dir(dir.to_path_buf()),
        )
    }

    async fn post(state: Arc<AppState>, uri: &str) -> (http::StatusCode, Vec<u8>) {
        let response = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn compact_and_rebuild_are_404_when_developer_mode_is_off() {
        let dir = tempfile::tempdir().unwrap();
        for uri in ["/api/dev/compact-database", "/api/dev/rebuild-projections"] {
            let (status, _) = post(state_with_dev_mode(dir.path(), false), uri).await;
            assert_eq!(
                status,
                http::StatusCode::NOT_FOUND,
                "{uri} is off the default surface"
            );
        }
    }

    #[tokio::test]
    async fn rebuild_projections_reports_every_model_matching_under_dev_mode() {
        let dir = tempfile::tempdir().unwrap();
        let state = composed_dev_state(dir.path()).await;
        let (status, body) = post(state, "/api/dev/rebuild-projections").await;
        assert_eq!(status, http::StatusCode::OK);
        let report: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(report["allMatched"], true, "{report}");
        assert_eq!(report["tables"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn compact_database_writes_a_copy_under_dev_mode() {
        let dir = tempfile::tempdir().unwrap();
        let state = composed_dev_state(dir.path()).await;
        let (status, body) = post(state, "/api/dev/compact-database").await;
        assert_eq!(status, http::StatusCode::OK);
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            result["bytes"].as_u64().unwrap() > 0,
            "the compacted copy has a real size: {result}"
        );
        assert!(dir.path().join("entropia_orme-compacted.db").exists());
    }

    #[tokio::test]
    async fn the_crash_reporting_toggle_round_trips_under_developer_mode() {
        let dir = tempfile::tempdir().unwrap();
        // Off by default.
        assert!(!eo_services::observability_config::crash_reporting_enabled(
            dir.path()
        ));
        // POST enables it.
        let response = build_router(state_with_dev_mode(dir.path(), true))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/dev/crash-reporting")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"crash_reporting_enabled":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert!(eo_services::observability_config::crash_reporting_enabled(
            dir.path()
        ));
        // GET reflects it.
        let response = build_router(state_with_dev_mode(dir.path(), true))
            .oneshot(
                Request::builder()
                    .uri("/api/dev/crash-reporting")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("\"crash_reporting_enabled\":true"));
    }
}
