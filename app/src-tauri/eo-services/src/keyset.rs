//! The opaque keyset-cursor codec shared by the paginated list reads
//! (the ledger and session lists): base64url (no padding) of the JSON
//! seek key of the last row on a page. Opaque so clients treat the
//! token as a unit, and robust to any characters a user-entered value
//! or a UUID id carries.

use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Encode a seek key (any serialisable tuple) as an opaque cursor token.
pub fn encode_cursor<T: Serialize>(key: &T) -> String {
    let json = serde_json::to_vec(key).expect("a cursor key serialises");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

/// Decode a cursor token back to its seek key, or `None` for a malformed
/// token (which the caller answers as a bad request).
pub fn decode_cursor<T: DeserializeOwned>(token: &str) -> Option<T> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}
