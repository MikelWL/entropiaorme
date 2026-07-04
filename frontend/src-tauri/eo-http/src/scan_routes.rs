//! The one-shot repair-cost read (the repair-scan leg of tracking),
//! served over the composed [`RepairOcrService`].
//!
//! The result is a JSON [`Value`] already in the service's shape; the
//! route projects it into the response model's field order (replicating
//! Pydantic's `response_model_exclude_unset` = the keys the service set,
//! emitted in declaration order) and serialises it the backend's way. The
//! service's logical refusals ride the body of a plain 200 (the reference
//! returns the `{"error": ...}` dict, it does not raise); only repair-scan
//! gates a 400.
//!
//! The manual skill-scan family (status, capture, undo, and their kin)
//! moved to the typed-command facade (`eo_api::scan`); its projection
//! helper [`project`] stays here, shared with the producer routes.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Response, StatusCode};
use eo_services::repair_ocr::RepairOcrService;
use serde_json::{Map, Value};

use crate::hydration::{detail, error_response, plain_json_response};

const REPAIR_FIELDS: [&str; 4] = ["cost_ped", "raw_text", "confidence", "error"];

/// Project a service value into a response model's field order, emitting
/// only the keys present in the value (Pydantic's `exclude_unset`). The
/// non-exclude models in this surface (the repair result) always carry
/// their full declared set from the service, so this also yields their
/// complete ordered object; the one declared-optional key absent on
/// success (`error`) is correctly omitted exactly as `extra="allow"`
/// leaves it. Shared with the snapshot route, whose polymorphic
/// `exclude_unset` model carries no undeclared top-level keys, so the same
/// present-keys-in-order rule applies.
pub(crate) fn project(value: &Value, order: &[&str]) -> Value {
    let mut out = Map::new();
    if let Some(object) = value.as_object() {
        for &field in order {
            if let Some(found) = object.get(field) {
                out.insert(field.to_string(), found.clone());
            }
        }
    }
    Value::Object(out)
}

/// POST /api/tracking/session/{session_id}/repair-scan: run the repair-cost
/// OCR, gated on the `repair_ocr_enabled` config flag (400 when disabled,
/// exactly as the reference). A plain 200 (POST, outside the ETag scope); the
/// failure legs ride the body with the declared fields first, then the extra
/// `error` key, as the `extra="allow"` model serialises them.
pub(crate) fn repair_scan(repair: &Arc<RepairOcrService>, enabled: bool) -> Response<Body> {
    if !enabled {
        return error_response(StatusCode::BAD_REQUEST, &detail("Repair OCR is disabled"));
    }
    plain_json_response(&project(&repair.scan_repair_cost(), &REPAIR_FIELDS))
}
