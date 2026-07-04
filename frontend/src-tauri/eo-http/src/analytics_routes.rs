//! The analytics HTTP read surface for the guide-mode demo namespace:
//! thin adapters from [`HydrationState`] onto the [`AnalyticsService`]
//! domain reads, shaping each result into the router's JSON response.
//!
//! The live `/api/analytics/*` surface has migrated to typed IPC commands
//! (ADR-0019); these adapters remain to serve the parallel `/api/demo/*`
//! namespace over the curated demo database until that surface migrates in
//! turn. The Overview / Activity aggregates, the ledger, the presets, and
//! the inventory reads all compute in [`eo_services::analytics`]; only the
//! HTTP shaping (the plain JSON body and the keyset `X-Next-Cursor` header)
//! lives here.

use axum::body::Body;
use axum::http::{HeaderValue, Response, StatusCode};
use eo_services::analytics::AnalyticsError;
use serde_json::Value;

use crate::hydration::{
    detail, error_response, internal_error, plain_json_response, HydrationState,
};

impl HydrationState {
    /// GET analytics/overview?period=...
    ///
    /// Scales O(days), not O(kills): the aggregate reads the daily rollup
    /// projection for completed days and touches the raw tables only for
    /// the partial edge days.
    pub async fn analytics_overview(&self, period: &str) -> Response<Body> {
        match self.analytics().overview(period).await {
            Ok(value) => plain_json_response(&value),
            Err(_) => internal_error(),
        }
    }

    /// GET analytics/activity (no conditional-GET contract: the analytics
    /// surface is outside the ETag middleware's prefixes).
    pub async fn analytics_activity(&self, _if_none_match: Option<&str>) -> Response<Body> {
        match self.analytics().activity().await {
            Ok(value) => plain_json_response(&value),
            Err(_) => internal_error(),
        }
    }

    /// GET analytics/ledger?cursor=&limit= : the page of entries as the
    /// JSON body plus the opaque `X-Next-Cursor` header when a further page
    /// exists. A malformed cursor answers a 400.
    pub async fn list_ledger(&self, cursor: Option<&str>, limit: Option<i64>) -> Response<Body> {
        match self.analytics().list_ledger(cursor, limit).await {
            Ok(page) => {
                let mut response = plain_json_response(&Value::Array(page.entries));
                if let Some(cursor) = page.next_cursor {
                    if let Ok(value) = HeaderValue::from_str(&cursor) {
                        response.headers_mut().insert("x-next-cursor", value);
                    }
                }
                response
            }
            Err(AnalyticsError::InvalidCursor) => {
                error_response(StatusCode::BAD_REQUEST, &detail("Invalid cursor"))
            }
            Err(_) => internal_error(),
        }
    }

    /// GET analytics/ledger/presets
    pub async fn list_ledger_presets(&self) -> Response<Body> {
        match self.analytics().list_ledger_presets().await {
            Ok(rows) => plain_json_response(&Value::Array(rows)),
            Err(_) => internal_error(),
        }
    }

    /// GET analytics/inventory
    pub async fn list_inventory(&self) -> Response<Body> {
        match self.analytics().list_inventory().await {
            Ok(rows) => plain_json_response(&Value::Array(rows)),
            Err(_) => internal_error(),
        }
    }
}
