//! E2E-only deterministic backend stub at the `api_request` IPC boundary.
//!
//! Compiled in ONLY under the `e2e-stub` feature, which the native-shell e2e
//! build enables and dev/release never do (so the shipped binary is unaffected).
//! It relocates the suite's deterministic backend from the retired loopback-HTTP
//! stub (the former `frontend/e2e/stub-backend.mjs`) onto the real IPC
//! transport: the frontend's `invoke('api_request')` round-trip is exercised end
//! to end, but the body is served from the same committed fixtures
//! (`frontend/e2e/fixtures/*.json`) by the same method+path route table, so the
//! rendered state and the committed visual baselines are unchanged. WebDriver
//! cannot intercept `invoke` (as it could not intercept `fetch`), which is why
//! the stub lives in-process here rather than in the test harness.

// Defence-in-depth tripwire: this module compiles only under `e2e-stub`, and the
// e2e build is `--debug` (debug_assertions on). A release build (debug_assertions
// off) that enabled the feature by accident fails to compile here rather than
// shipping the fixture stub.
#[cfg(not(debug_assertions))]
compile_error!("the e2e-stub fixture backend must never be compiled into a release build");

use std::sync::OnceLock;

use serde_json::Value;

use crate::ApiResponse;

const DASHBOARD_FIXTURE: &str = include_str!("../../../e2e/fixtures/dashboard.json");
const ANALYTICS_FIXTURE: &str = include_str!("../../../e2e/fixtures/analytics.json");

/// The method+path route table, mirroring the retired HTTP stub's `ROUTES`
/// exactly so the rendered data (and therefore the committed baselines) are
/// identical. Built once and cached.
fn routes() -> &'static [(&'static str, Value)] {
    static ROUTES: OnceLock<Vec<(&'static str, Value)>> = OnceLock::new();
    ROUTES.get_or_init(|| {
        let dashboard: Value =
            serde_json::from_str(DASHBOARD_FIXTURE).expect("e2e dashboard fixture is valid JSON");
        let analytics: Value =
            serde_json::from_str(ANALYTICS_FIXTURE).expect("e2e analytics fixture is valid JSON");
        vec![
            ("GET /api/tracking/snapshot", dashboard["snapshot"].clone()),
            (
                "GET /api/tracking/session/e2e-session",
                dashboard["sessionDetail"].clone(),
            ),
            ("GET /api/quests", dashboard["quests"].clone()),
            ("GET /api/quests/playlists", dashboard["playlists"].clone()),
            ("GET /api/analytics/overview", analytics["overview"].clone()),
            ("GET /api/analytics/activity", analytics["activity"].clone()),
            ("GET /api/analytics/ledger", analytics["ledger"].clone()),
            (
                "GET /api/analytics/ledger/presets",
                analytics["presets"].clone(),
            ),
            (
                "GET /api/analytics/inventory",
                analytics["inventory"].clone(),
            ),
            ("GET /api/tracking/sessions", analytics["sessions"].clone()),
        ]
    })
}

/// The analytics fixture value under `key` (`overview` / `activity` /
/// `ledger` / `presets` / `inventory`), for the typed analytics read
/// commands. The live analytics surface no longer flows through
/// `api_request` (it migrated to typed IPC commands), so the e2e build
/// serves the same committed fixture through those commands, keeping the
/// visual baselines stable. Built once and cached.
pub fn analytics_fixture(key: &str) -> Value {
    static ANALYTICS: OnceLock<Value> = OnceLock::new();
    ANALYTICS
        .get_or_init(|| {
            serde_json::from_str(ANALYTICS_FIXTURE).expect("e2e analytics fixture is valid JSON")
        })
        .get(key)
        .cloned()
        .unwrap_or(Value::Null)
}

/// The dashboard fixture value under `key` (`snapshot` / `sessionDetail` /
/// `quests` / `playlists`), for the typed tracking read commands. The live
/// tracking surface no longer flows through `api_request` (it migrated to
/// typed IPC commands), so the e2e build serves the same committed
/// dashboard fixture through those commands, keeping the visual baselines
/// stable. The session-list fixture lives in the analytics fixture under
/// `sessions` (`analytics_fixture`). Built once and cached.
pub fn dashboard_fixture(key: &str) -> Value {
    static DASHBOARD: OnceLock<Value> = OnceLock::new();
    DASHBOARD
        .get_or_init(|| {
            serde_json::from_str(DASHBOARD_FIXTURE).expect("e2e dashboard fixture is valid JSON")
        })
        .get(key)
        .cloned()
        .unwrap_or(Value::Null)
}

fn json_response(body: String) -> ApiResponse {
    ApiResponse {
        status: 200,
        status_text: "OK".to_string(),
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body,
    }
}

/// Serve a request from the fixture table. The query string is stripped before
/// matching (mirroring the HTTP stub); an unmatched route returns a forgiving
/// `[]` 200, logged loudly, the same fall-through the HTTP stub used so an
/// incidental list-shaped read never 500s the UI while a missing fixture stays
/// visible in the run output.
pub fn serve(method: &str, path: &str) -> ApiResponse {
    let path = path.split('?').next().unwrap_or(path);
    let key = format!("{method} {path}");
    if let Some((_, value)) = routes().iter().find(|(route, _)| *route == key) {
        return json_response(value.to_string());
    }
    tracing::warn!(target: "eo::e2e_stub", "UNMATCHED {key} -> []");
    json_response("[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::serve;

    #[test]
    fn the_snapshot_route_serves_the_active_session_fixture() {
        let response = serve("GET", "/api/tracking/snapshot");
        assert_eq!(response.status, 200);
        let value: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(value["status"], "active");
        assert_eq!(value["session_id"], "e2e-session");
    }

    #[test]
    fn the_query_string_is_stripped_before_matching() {
        let response = serve("GET", "/api/analytics/overview?period=all");
        let value: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert!(value.get("totalReturnRate").is_some());
    }

    #[test]
    fn an_unmatched_route_falls_through_to_an_empty_list() {
        let response = serve("GET", "/api/something/unmodelled");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "[]");
    }

    /// The analytics fixture the typed read commands serve deserialises into
    /// each command's DTO: a fixture / DTO drift fails here rather than as a
    /// blank analytics surface in the visual run.
    #[test]
    fn the_analytics_fixture_deserialises_into_the_typed_dtos() {
        use eo_api::analytics::{
            AnalyticsActivity, AnalyticsOverview, InventoryItem, LedgerItem, LedgerPreset,
        };
        serde_json::from_value::<AnalyticsOverview>(super::analytics_fixture("overview"))
            .expect("overview fixture matches AnalyticsOverview");
        serde_json::from_value::<AnalyticsActivity>(super::analytics_fixture("activity"))
            .expect("activity fixture matches AnalyticsActivity");
        serde_json::from_value::<Vec<LedgerItem>>(super::analytics_fixture("ledger"))
            .expect("ledger fixture matches Vec<LedgerItem>");
        serde_json::from_value::<Vec<LedgerPreset>>(super::analytics_fixture("presets"))
            .expect("presets fixture matches Vec<LedgerPreset>");
        serde_json::from_value::<Vec<InventoryItem>>(super::analytics_fixture("inventory"))
            .expect("inventory fixture matches Vec<InventoryItem>");
    }
}
