//! The manual skill-scan family: the user-driven, page-by-page skill
//! scan (status, start, capture, cancel, undo, process, accept, reject,
//! the held pending result, and the per-page capture preview) plus the
//! hands-free spacebar-capture toggle.
//!
//! Ported from the HTTP route handlers onto typed DTOs over the composed
//! [`SkillScanManual`] / [`SpacebarCaptureListener`]. The scanner never
//! raises for a logical refusal: it returns a `{"error": ...}` value on a
//! plain 200, and the HTTP layer projected that through each verb's
//! response-model field order (Pydantic `exclude_unset`). Two shapes are
//! preserved here:
//!
//! * The **status-shaped verbs** (`status`, `start`, `cancel`, `process`,
//!   and the status-carrying `capture` / `undo`) return the full
//!   [`ScanStatus`], whose `error` field carries the refusal message. On
//!   success `error` is null and every status field is present, byte-for-
//!   byte as the HTTP body. On a refusal the current status is returned
//!   with `error` set: the sole family contract movement, from the HTTP
//!   one-key `{"error": ...}` body to the full status carrying that error.
//!   Every consumer reads `.error` first and reads the status fields
//!   defensively, so the superset is invisible.
//! * The **disposition verbs** (`accept`, `reject`) return their own small
//!   results, preserving the HTTP polymorphic shape exactly: the success
//!   keys on success, the lone `error` key on a refusal (skip-none).
//!
//! The capture preview returns raw PNG bytes (base64-encoded at the shell
//! command, which keeps its bespoke non-manifest form); the pending read
//! returns `None` rather than the HTTP 404 (the typed transport has no
//! status code, so the wrapper maps absence to null directly).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Api, ApiError};

/// The settled scan phase, in the wire vocabulary the overlay switches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Idle,
    Capturing,
    Processing,
    AwaitingReview,
}

/// The per-page OCR progress counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScanProgress {
    pub done: i64,
    pub total: i64,
}

/// The full manual-scan status, in the response-model field order the
/// HTTP layer emitted. `error` carries a logical refusal's message (null
/// on success); every other field is always present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScanStatus {
    pub active: bool,
    pub processing: bool,
    pub captured_pages: i64,
    pub expected_pages: i64,
    pub last_scan_time: Option<f64>,
    pub skills_count: i64,
    pub configured: bool,
    pub game_window_present: bool,
    pub phase: ScanPhase,
    pub processing_progress: ScanProgress,
    pub has_pending_result: bool,
    pub error: Option<String>,
}

/// A capture verb's result: the status the grab settled on, plus the
/// 1-indexed page and whether the frame was captured. On a refusal the
/// page/captured extras are absent (the status carries the error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureResult {
    #[serde(flatten)]
    pub status: ScanStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured: Option<bool>,
}

/// The undo verb's result: the status after the pop, plus the popped
/// page number. Absent on a refusal (the status carries the error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UndoResult {
    #[serde(flatten)]
    pub status: ScanStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undone_page: Option<i64>,
}

/// The accept verb's result: `{ok, skills_persisted}` on success, the
/// lone `error` on a refusal (each key skipped when absent, preserving
/// the HTTP polymorphic body exactly).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AcceptResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills_persisted: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The reject verb's result: `{ok: true}` on success, the lone `error`
/// on a refusal.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct RejectResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The spacebar-capture toggle acknowledgement: the resulting enabled
/// state after the flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SpacebarResult {
    pub ok: bool,
    pub enabled: bool,
}

/// The held OCR result awaiting review: canonical skill name to level.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SkillScanPending {
    pub skills: BTreeMap<String, f64>,
}

impl Api {
    /// The current manual-scan status.
    pub fn scan_status(&self) -> Result<ScanStatus, ApiError> {
        self.status_from(self.skill_scan.get_status())
    }

    /// Begin a scan (optionally over a specific page count). A logical
    /// refusal (engine/window absent, count out of range, busy) rides
    /// the returned status's `error`.
    pub fn scan_start(&self, page_count: Option<i64>) -> Result<ScanStatus, ApiError> {
        self.status_from(self.skill_scan.start(page_count))
    }

    /// Grab the current page.
    pub fn scan_capture(&self) -> Result<CaptureResult, ApiError> {
        let value = self.skill_scan.capture_current_page();
        if value.get("phase").is_some() {
            serde_json::from_value(value).map_err(ApiError::internal("scan capture decode"))
        } else {
            Ok(CaptureResult {
                status: self.status_from(value)?,
                page: None,
                captured: None,
            })
        }
    }

    /// Abandon the active scan, returning the settled (idle) status.
    pub fn scan_cancel(&self) -> Result<ScanStatus, ApiError> {
        self.status_from(self.skill_scan.cancel())
    }

    /// Pop the most recent capture.
    pub fn scan_undo(&self) -> Result<UndoResult, ApiError> {
        let value = self.skill_scan.undo_last_capture();
        if value.get("phase").is_some() {
            serde_json::from_value(value).map_err(ApiError::internal("scan undo decode"))
        } else {
            Ok(UndoResult {
                status: self.status_from(value)?,
                undone_page: None,
            })
        }
    }

    /// Kick off extraction on the captured pages.
    pub fn scan_process(&self) -> Result<ScanStatus, ApiError> {
        self.status_from(self.skill_scan.process())
    }

    /// Persist the held scan result.
    pub fn scan_accept(&self) -> Result<AcceptResult, ApiError> {
        let value = self.skill_scan.accept();
        Ok(AcceptResult {
            ok: value.get("ok").and_then(Value::as_bool),
            skills_persisted: value.get("skills_persisted").and_then(Value::as_i64),
            error: error_of(&value),
        })
    }

    /// Discard the held scan result.
    pub fn scan_reject(&self) -> Result<RejectResult, ApiError> {
        let value = self.skill_scan.reject();
        Ok(RejectResult {
            ok: value.get("ok").and_then(Value::as_bool),
            error: error_of(&value),
        })
    }

    /// The held OCR result awaiting review, or `None` when none awaits.
    pub fn scan_pending(&self) -> Result<Option<SkillScanPending>, ApiError> {
        match self.skill_scan.get_pending_result() {
            None => Ok(None),
            Some(pairs) => {
                let skills = pairs.into_iter().collect();
                Ok(Some(SkillScanPending { skills }))
            }
        }
    }

    /// The stored PNG for a 1-indexed captured page. `NotFound` when the
    /// page has no capture (the HTTP 404 leg); the shell command base64-
    /// encodes the bytes for the `<img>` `data:` URL.
    pub fn scan_capture_png(&self, page: i64) -> Result<Vec<u8>, ApiError> {
        self.skill_scan
            .get_capture_png(page)
            .ok_or_else(|| ApiError::not_found("Capture not available"))
    }

    /// Toggle the hands-free spacebar-capture listener, acknowledging
    /// with the resulting enabled state.
    pub fn scan_set_spacebar_capture(&self, enabled: bool) -> Result<SpacebarResult, ApiError> {
        self.spacebar.set_enabled(enabled);
        Ok(SpacebarResult {
            ok: true,
            enabled: self.spacebar.is_enabled(),
        })
    }

    /// Shape a scan service value into the typed status. A full status
    /// value (carrying `phase`) decodes directly; a bare `{"error": ...}`
    /// refusal returns the current status with that error overlaid (the
    /// family's ratified contract movement).
    fn status_from(&self, value: Value) -> Result<ScanStatus, ApiError> {
        if value.get("phase").is_some() {
            serde_json::from_value(value).map_err(ApiError::internal("scan status decode"))
        } else {
            let mut status: ScanStatus = serde_json::from_value(self.skill_scan.get_status())
                .map_err(ApiError::internal("scan status decode"))?;
            status.error = error_of(&value);
            Ok(status)
        }
    }
}

/// The `error` string of a service value, when present.
fn error_of(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
}
