//! Shared JSON payload semantics, an owned contract pinned by the frozen
//! goldens: Python truthiness, sqlite3-adapter bind rules, and Python
//! str() rendering for byte-exact refusal messages.

use serde_json::Value;

/// Python truthiness over JSON values: null, false, zero, and empty
/// strings/arrays/objects are false.
pub(super) fn json_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(entries)) => !entries.is_empty(),
    }
}

/// A JSON payload value as an owned SQLite parameter, with the
/// original's sqlite3 adapter semantics (booleans as integers); a
/// structured value has no adapter and is a caller error, as it is in
/// the original. The owned [`rusqlite::types::Value`] binds by value, so
/// it moves into a `with_writer` closure with the rest of the captured
/// parameters.
pub(super) fn value_to_sql(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as SqlValue;
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(flag) => SqlValue::Integer(i64::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => SqlValue::Integer(integer),
            None => SqlValue::Real(number.as_f64().expect("finite numeric payload")),
        },
        Value::String(text) => SqlValue::Text(text.clone()),
        other => panic!("unbindable payload value: {other}"),
    }
}
