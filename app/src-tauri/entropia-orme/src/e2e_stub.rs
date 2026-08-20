//! E2E-only deterministic backend fixtures for the typed read commands.
//!
//! Compiled in ONLY under the `e2e-stub` feature, which the native-shell e2e
//! build enables and dev/release never do (so the shipped binary is unaffected).
//! It relocates the suite's deterministic backend from the retired loopback-HTTP
//! stub (the former `app/e2e/stub-backend.mjs`) onto the real IPC
//! transport: the frontend's typed read commands are exercised end to end, but
//! the body is served from the same committed fixtures
//! (`app/e2e/fixtures/*.json`), so the rendered state and the committed
//! visual baselines are unchanged. WebDriver cannot intercept `invoke` (as it
//! could not intercept `fetch`), which is why the stub lives in-process here
//! rather than in the test harness.

// Defence-in-depth tripwire: this module compiles only under `e2e-stub`, and the
// e2e build is `--debug` (debug_assertions on). A release build (debug_assertions
// off) that enabled the feature by accident fails to compile here rather than
// shipping the fixture stub.
#[cfg(not(debug_assertions))]
compile_error!("the e2e-stub fixture backend must never be compiled into a release build");

use std::sync::OnceLock;

use serde_json::Value;

const DASHBOARD_FIXTURE: &str = include_str!("../../../e2e/fixtures/dashboard.json");
const ANALYTICS_FIXTURE: &str = include_str!("../../../e2e/fixtures/analytics.json");

/// The analytics fixture value under `key` (`overview` / `activity` /
/// `ledger` / `presets` / `inventory` / `sessions`), for the typed analytics
/// and session-list read commands. The e2e build serves the same committed
/// fixture through those commands, keeping the visual baselines stable. Built
/// once and cached.
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

/// The session-list fixture, optionally narrowed to one definition in
/// the same way as the live command. The definition reference is fixture
/// metadata rather than part of the public list row DTO.
pub fn analytics_sessions_fixture(definition_id: Option<i64>) -> Value {
    let sessions = analytics_fixture("sessions");
    let Some(definition_id) = definition_id else {
        return sessions;
    };
    filter_sessions_fixture(sessions, definition_id)
}

fn filter_sessions_fixture(sessions: Value, definition_id: i64) -> Value {
    let Value::Array(sessions) = sessions else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        sessions
            .into_iter()
            .filter(|session| {
                session.get("definitionId").is_some_and(|value| {
                    value.as_i64() == Some(definition_id)
                        || value.as_str().and_then(|value| value.parse::<i64>().ok())
                            == Some(definition_id)
                })
            })
            .collect(),
    )
}

/// The dashboard fixture value under `key` (`snapshot` / `sessionDetail` /
/// `quests`), for the typed tracking read commands. The e2e build
/// serves the same committed dashboard fixture through those commands, keeping
/// the visual baselines stable. The session-list fixture lives in the analytics
/// fixture under `sessions` (`analytics_fixture`). Built once and cached.
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

#[cfg(test)]
mod tests {
    /// The analytics fixture the typed read commands serve deserialises into
    /// each command's DTO: a fixture / DTO drift fails here rather than as a
    /// blank analytics surface in the visual run.
    #[test]
    fn the_analytics_fixture_deserialises_into_the_typed_dtos() {
        use eo_api::analytics::{
            AnalyticsHarvest, AnalyticsHunting, AnalyticsHuntingActivity, AnalyticsOverview,
            InventoryItem, LedgerItem, LedgerPreset,
        };
        serde_json::from_value::<AnalyticsOverview>(super::analytics_fixture("overview"))
            .expect("overview fixture matches AnalyticsOverview");
        serde_json::from_value::<AnalyticsHunting>(super::analytics_fixture("hunting"))
            .expect("hunting fixture matches AnalyticsHunting");
        serde_json::from_value::<AnalyticsHarvest>(super::analytics_fixture("harvest"))
            .expect("harvest fixture matches AnalyticsHarvest");
        serde_json::from_value::<AnalyticsHuntingActivity>(super::analytics_fixture(
            "huntingActivity",
        ))
        .expect("huntingActivity fixture matches AnalyticsHuntingActivity");
        serde_json::from_value::<Vec<LedgerItem>>(super::analytics_fixture("ledger"))
            .expect("ledger fixture matches Vec<LedgerItem>");
        serde_json::from_value::<Vec<LedgerPreset>>(super::analytics_fixture("presets"))
            .expect("presets fixture matches Vec<LedgerPreset>");
        serde_json::from_value::<Vec<InventoryItem>>(super::analytics_fixture("inventory"))
            .expect("inventory fixture matches Vec<InventoryItem>");
    }

    /// The tracking reads that feed the dashboard baseline serve fixtures
    /// through their typed commands: the snapshot and session-detail from the
    /// dashboard fixture, the session list from the analytics fixture. A
    /// fixture / DTO drift fails here rather than as a blank dashboard in the
    /// visual run (this also guards the session-detail DTO's `notableEvents`
    /// `serde(default)`, which lets the fixture omit that array).
    #[test]
    fn the_dashboard_fixture_deserialises_into_the_tracking_dtos() {
        use eo_api::tracking::{SessionDetail, TrackingSession, TrackingSnapshot};
        serde_json::from_value::<TrackingSnapshot>(super::dashboard_fixture("snapshot"))
            .expect("snapshot fixture matches TrackingSnapshot");
        serde_json::from_value::<SessionDetail>(super::dashboard_fixture("sessionDetail"))
            .expect("sessionDetail fixture matches SessionDetail");
        serde_json::from_value::<Vec<TrackingSession>>(super::analytics_fixture("sessions"))
            .expect("sessions fixture matches Vec<TrackingSession>");
    }

    #[test]
    fn a_definition_scope_filters_session_fixture_metadata() {
        let sessions = serde_json::json!([
            { "id": "one", "definitionId": 1 },
            { "id": "two", "definitionId": "2" },
            { "id": "loose" }
        ]);
        let filtered = super::filter_sessions_fixture(sessions, 2);
        assert_eq!(
            filtered,
            serde_json::json!([{ "id": "two", "definitionId": "2" }])
        );
    }
}
