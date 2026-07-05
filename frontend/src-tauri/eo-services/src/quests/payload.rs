//! Shared JSON payload semantics, ported from the original: Python
//! truthiness, the sqlite3 adapter's bind rules, and Python str()
//! rendering for byte-exact error messages.

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

/// Bind a JSON payload value with the original's sqlite3 adapter
/// semantics (booleans as integers); a structured value has no adapter
/// and is a caller error, as it is in the original.
pub(super) fn bind_json<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    value: &'q Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match value {
        Value::Null => query.bind(None::<String>),
        Value::Bool(flag) => query.bind(i64::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => query.bind(integer),
            None => query.bind(number.as_f64().expect("finite numeric payload")),
        },
        Value::String(text) => query.bind(text.as_str()),
        other => panic!("unbindable payload value: {other}"),
    }
}

/// Render a JSON value the way a Python f-string renders the
/// corresponding object (for byte-exact error messages).
pub(super) fn python_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
