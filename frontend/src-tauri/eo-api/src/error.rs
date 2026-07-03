//! The typed error surface every facade operation returns.
//!
//! The serialised shape (`kind` + `message`) is the IPC error contract:
//! the frontend's command wrapper maps `kind` onto its thrown error
//! class, preserving the message verbatim. Internal failures carry a
//! fixed message on purpose (a driver or storage error never leaks its
//! detail across the boundary); the full chain is logged server-side by
//! the operation that saw it.

use serde::Serialize;

/// A facade operation's failure, in the shape the IPC boundary
/// serialises.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ApiError {
    /// The request named something that cannot be acted on (a missing
    /// required field, a forbidden transition). Maps to the thrown
    /// 400-class error the frontend already handles.
    #[error("{message}")]
    BadRequest { message: String },
    /// The named resource does not exist.
    #[error("{message}")]
    NotFound { message: String },
    /// The operation conflicts with current state (a referenced row).
    #[error("{message}")]
    Conflict { message: String },
    /// An internal failure whose detail stays server-side.
    #[error("Internal Server Error")]
    Internal,
    /// The backend substrate has not composed yet (the startup window,
    /// or a declined composition); the frontend re-drives its reads on
    /// the substrate-installed event.
    #[error("backend substrate not ready")]
    Unavailable,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_serialised_shape_carries_kind_and_message() {
        let err = ApiError::not_found("Equipment item 9 not found");
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            serde_json::json!({"kind": "notFound", "message": "Equipment item 9 not found"})
        );
        assert_eq!(
            serde_json::to_value(ApiError::Internal).unwrap(),
            serde_json::json!({"kind": "internal"})
        );
        assert_eq!(err.to_string(), "Equipment item 9 not found");
    }
}
