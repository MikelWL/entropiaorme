//! The in-process HTTP layer's shared response machinery and the
//! tracking-session quest-link handlers, byte-faithful to the backend's
//! responses: the body serialisation form the backend's HTTP layer
//! emits, the strong-ETag conditional-GET semantics of its middleware (a
//! SHA-256 ETag over the body, `Cache-Control: no-cache`, and `304 Not
//! Modified` with an empty body when `If-None-Match` already names the
//! representation), the unhandled-exception envelope, and the quest-link
//! suggestion / decision formatters (ids as strings).
//!
//! Route families migrate off this surface onto typed IPC commands
//! family by family (ADR-0019); the quests + playlists reads and writes
//! moved to `eo-api`, leaving the shared helpers here plus the two
//! `/api/tracking/session/{id}/quest-link*` handlers a later family
//! carries. The handlers were each proven against the Python reference
//! over a shared database before its route registered natively.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Response, StatusCode};
use eo_services::clock::Clock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use eo_services::quests::{QuestError, QuestService};
use eo_wire::normalizer::to_wire_json;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// The services the hydration handlers read through.
pub struct HydrationState {
    quests: QuestService,
    pub(crate) db: Db,
    pub(crate) game_data: Arc<GameDataStore>,
    pub(crate) clock: Arc<dyn Clock>,
    /// The data directory the substrate serves: the config read-through
    /// (`settings.json`, now written solely by the native `ConfigService`)
    /// and the `dbPath` settings field both render from it.
    pub(crate) data_dir: PathBuf,
}

impl HydrationState {
    pub fn new(
        db: Db,
        game_data: Arc<GameDataStore>,
        clock: Arc<dyn Clock>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            quests: QuestService::new(db.clone(), clock.clone()),
            db,
            game_data,
            clock,
            data_dir,
        }
    }

    /// The reader pool, for plain reads on the hydration/analytics read
    /// surface (dashboard GETs run concurrently with combat writes).
    pub(crate) fn read(&self) -> &SqlitePool {
        self.db.read()
    }

    /// The writer pool, for the surface's mutations (ledger edits, claims,
    /// equipment CRUD).
    pub(crate) fn write(&self) -> &SqlitePool {
        self.db.write()
    }
}

/// The strong ETag value (quoted SHA-256 hex) for a body, exactly as
/// the backend's middleware computes it.
pub fn compute_strong_etag(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    format!("\"{:x}\"", hasher.finalize())
}

/// Whether `If-None-Match` indicates the client already holds the
/// representation: the wildcard, or any listed tag equal to the
/// current one. A weak `W/` prefix on a listed tag is removed before
/// the comparison, and whitespace after the prefix is tolerated, both
/// exactly as the backend's parser behaves (it strips around the
/// prefix removal).
fn if_none_match_matches(header_value: Option<&str>, current_etag: &str) -> bool {
    let Some(header_value) = header_value else {
        return false;
    };
    if header_value.trim() == "*" {
        return true;
    }
    header_value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        let candidate = candidate.strip_prefix("W/").unwrap_or(candidate).trim();
        candidate == current_etag
    })
}

/// A 200 response under the conditional-GET contract for an arbitrary body
/// and media type: the strong ETag over the body, `Cache-Control: no-cache`,
/// and `304 Not Modified` (empty body) on a matching `If-None-Match`. The
/// ETag middleware covers every 2xx GET under its prefixes regardless of
/// media type, so the manual-scan capture PNG rides this exactly as the JSON
/// reads do; [`json_response`] is this specialised to JSON.
pub(crate) fn conditional_response(
    body: Vec<u8>,
    media_type: &'static str,
    if_none_match: Option<&str>,
) -> Response<Body> {
    let etag = compute_strong_etag(&body);
    let not_modified = if_none_match_matches(if_none_match, &etag);
    let mut response = Response::builder()
        .status(if not_modified {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header(header::ETAG, &etag)
        .header(header::CACHE_CONTROL, "no-cache");
    if !not_modified {
        response = response.header(header::CONTENT_TYPE, media_type);
    }
    response
        .body(if not_modified {
            Body::empty()
        } else {
            Body::from(body)
        })
        .expect("response assembles")
}

/// A hydration JSON response under the conditional-GET contract: 200
/// with the body (or 304 with none) plus the ETag and Cache-Control
/// headers either way.
pub(crate) fn json_response(payload: &Value, if_none_match: Option<&str>) -> Response<Body> {
    conditional_response(
        to_wire_json(payload).into_bytes(),
        "application/json",
        if_none_match,
    )
}

/// A non-2xx JSON error response (no ETag: the middleware touches
/// only successful responses).
pub(crate) fn error_response(status: StatusCode, payload: &Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(to_wire_json(payload).into_bytes()))
        .expect("response assembles")
}

/// The backend's HTTPException rendering: `{"detail": <message>}`.
pub(crate) fn detail(message: &str) -> Value {
    json!({"detail": message})
}

/// A service failure surfaces as the backend's unhandled-exception
/// envelope (500 with the generic body).
pub(crate) fn internal_error() -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("Internal Server Error"))
        .expect("response assembles")
}

// ── Router-layer formatters ────────────

/// `str(value)` as the formatters apply it to ids (integers render
/// identically in both languages; strings pass through).
pub(crate) fn python_str_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

/// `str(id) if id is not None else None` over a suggestion's nullable
/// id (the quest-link routes stringify the integer ids, leaving null
/// through).
fn str_id_or_null(value: &Value) -> Value {
    if value.is_null() {
        Value::Null
    } else {
        json!(python_str_of(value))
    }
}

/// The quest-link suggestion wire shape, mirroring
/// `get_session_quest_link_suggestion`'s dict construction (all seven
/// fields always present; the link fields null when absent).
fn format_quest_link_suggestion(session_id: &str, suggestion: &Value) -> Value {
    json!({
        "sessionId": session_id,
        "suggestionType": suggestion["suggestion_type"],
        "reason": suggestion["reason"],
        "questId": str_id_or_null(&suggestion["quest_id"]),
        "questName": suggestion["quest_name"],
        "playlistId": str_id_or_null(&suggestion["playlist_id"]),
        "playlistName": suggestion["playlist_name"],
    })
}

/// The write surface: each method mirrors its router handler (the
/// service call, the 404 mapping, the formatter), replying without
/// conditional-GET headers (the backend's middleware covers 2xx GETs
/// only).
impl HydrationState {
    /// GET /api/tracking/session/{session_id}/quest-link-suggestion (a
    /// read: the conditional-GET contract applies). 404 if the session
    /// is absent, checked before the quest service runs.
    pub async fn session_quest_link_suggestion(
        &self,
        session_id: &str,
        if_none_match: Option<&str>,
    ) -> Response<Body> {
        match self.session_exists(session_id).await {
            Ok(true) => {}
            Ok(false) => return session_not_found(),
            Err(_) => return internal_error(),
        }
        match self.quests.get_session_link_suggestion(session_id).await {
            Ok(suggestion) => json_response(
                &format_quest_link_suggestion(session_id, &suggestion),
                if_none_match,
            ),
            Err(error) => quest_error_response(error),
        }
    }

    /// POST /api/tracking/session/{session_id}/quest-link (a plain-200
    /// write). 404 if the session is absent; an unknown action is a
    /// 400. Accept persists the curated suggestion and replies with the
    /// full link object; decline records the refusal and replies with
    /// only `sessionId`/`status` (the reference serialises the decision
    /// with `response_model_exclude_unset`, so the two arms emit
    /// different field sets).
    pub async fn decide_session_quest_link(
        &self,
        session_id: &str,
        action: &str,
    ) -> Response<Body> {
        match self.session_exists(session_id).await {
            Ok(true) => {}
            Ok(false) => return session_not_found(),
            Err(_) => return internal_error(),
        }
        let action = action.trim().to_lowercase();
        if action == "accept" {
            return match self.quests.accept_session_link_suggestion(session_id).await {
                Ok(suggestion) => plain_json_response(&json!({
                    "sessionId": session_id,
                    "status": "linked",
                    "linkType": suggestion["suggestion_type"],
                    "questId": str_id_or_null(&suggestion["quest_id"]),
                    "questName": suggestion["quest_name"],
                    "playlistId": str_id_or_null(&suggestion["playlist_id"]),
                    "playlistName": suggestion["playlist_name"],
                })),
                // The route catches the no-linkable-suggestion
                // ValueError and maps it to 409 (unlike the rest of the
                // quest surface, where Invalid stays an unhandled 500);
                // a genuine database failure still surfaces as 500.
                Err(QuestError::Invalid(message)) => {
                    error_response(StatusCode::CONFLICT, &detail(&message))
                }
                Err(QuestError::Db(_) | QuestError::Rollup(_)) => internal_error(),
            };
        }
        if action == "decline" {
            return match self.quests.decline_session_link(session_id).await {
                Ok(()) => plain_json_response(&json!({
                    "sessionId": session_id,
                    "status": "declined",
                })),
                Err(error) => quest_error_response(error),
            };
        }
        error_response(
            StatusCode::BAD_REQUEST,
            &detail("Action must be 'accept' or 'decline'"),
        )
    }

    /// The session-existence precondition both quest-link routes apply
    /// before the quest service runs (a bare `SELECT id`, the
    /// reference's own guard).
    async fn session_exists(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT id FROM tracking_sessions WHERE id = ?")
            .bind(session_id)
            .fetch_optional(self.read())
            .await?;
        Ok(row.is_some())
    }
}

/// A plain JSON 200: no conditional-GET headers. Write replies use
/// it everywhere; reads outside the ETag middleware's prefixes
/// (settings, character, equipment) use it too.
pub(crate) fn plain_json_response(payload: &Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(to_wire_json(payload)))
        .expect("write response builds")
}

fn session_not_found() -> Response<Body> {
    error_response(StatusCode::NOT_FOUND, &detail("Session not found"))
}

/// The quest router's error mapping: the quests router catches no
/// service error, so every failure surfaces as the backend's
/// unhandled-exception envelope.
pub fn quest_error_response(_error: QuestError) -> Response<Body> {
    internal_error()
}

// Expected values in these tests are the backend's own outputs: the
// ETag form and conditional semantics from its middleware, the
// formatter shapes from its routers, and the error envelopes from its
// HTTP layer (all held byte-for-byte by the A/B fidelity test; these
// hermetic pins keep the same surface guarded without a live backend).
#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[test]
    fn the_etag_is_the_quoted_body_hash() {
        assert_eq!(
            compute_strong_etag(b"hello"),
            "\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\""
        );
    }

    #[test]
    fn if_none_match_parses_the_backend_way() {
        let current = "\"abc\"";
        assert!(!if_none_match_matches(None, current));
        assert!(if_none_match_matches(Some("*"), current));
        assert!(if_none_match_matches(Some("\"abc\""), current));
        assert!(if_none_match_matches(Some("\"x\", \"abc\""), current));
        assert!(if_none_match_matches(Some("W/\"abc\""), current));
        // Whitespace after the weak prefix is tolerated, as the
        // backend strips around its prefix removal.
        assert!(if_none_match_matches(Some("W/ \"abc\""), current));
        assert!(if_none_match_matches(Some("W/\t\"abc\""), current));
        assert!(if_none_match_matches(Some("\"x\", W/ \"abc\""), current));
        assert!(!if_none_match_matches(Some("\"nope\""), current));
    }

    #[test]
    fn scalar_helpers_match_the_router_layer() {
        assert_eq!(python_str_of(&json!("s")), "s");
        assert_eq!(python_str_of(&json!(42)), "42");
        assert_eq!(detail("gone"), json!({"detail": "gone"}));
    }

    async fn parts(response: Response<Body>) -> (StatusCode, http::HeaderMap, Vec<u8>) {
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, headers, bytes)
    }

    #[tokio::test]
    async fn responses_carry_the_conditional_get_contract() {
        let payload = json!({"a": 1});
        let (status, headers, body) = parts(json_response(&payload, None)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, b"{\"a\":1}");
        let etag = headers.get("etag").unwrap().to_str().unwrap().to_string();
        assert_eq!(etag, compute_strong_etag(b"{\"a\":1}"));
        assert_eq!(headers.get("cache-control").unwrap(), "no-cache");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");

        let (status, headers, body) = parts(json_response(&payload, Some(&etag))).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(body.is_empty());
        assert_eq!(headers.get("etag").unwrap().to_str().unwrap(), etag);
        assert_eq!(headers.get("cache-control").unwrap(), "no-cache");
        assert!(headers.get("content-type").is_none());

        let (status, _, body) = parts(error_response(
            StatusCode::NOT_FOUND,
            &detail("Species 'X' not found"),
        ))
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, b"{\"detail\":\"Species 'X' not found\"}");

        let (status, headers, body) = parts(internal_error()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            headers.get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(body, b"Internal Server Error");

        // The quest router catches no service error, so its mapping is
        // the same unhandled-exception envelope.
        let (status, _, body) =
            parts(quest_error_response(QuestError::Invalid("any".to_string()))).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, b"Internal Server Error");
    }
}
