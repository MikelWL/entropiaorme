//! CPython-compatible time helpers: the payload timestamp form, the
//! naive-local epoch conversions (fold=0 semantics), and the isoformat
//! renderings the readout and producer routes share.

use chrono::NaiveDateTime;
use eo_wire::normalizer::round_half_even;
#[cfg(test)]
use serde_json::Value;

/// Bus payload timestamps are the watcher's isoformat strings (whole
/// seconds; a fractional suffix is tolerated for symmetry with the
/// harness normaliser); the original receives `datetime` objects on
/// its in-process bus. Typed payloads carry their timestamps as
/// `String` now, so only the pinning test still reads through `Value`.
#[cfg(test)]
pub(crate) fn parse_bus_timestamp(value: Option<&Value>) -> Option<NaiveDateTime> {
    parse_timestamp_str(value?.as_str()?)
}

/// The payload timestamp form (isoformat, with or without fractional
/// seconds) parsed back to the instant.
pub(crate) fn parse_timestamp_str(raw: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
        .ok()
}

/// `timedelta.total_seconds()`.
pub(super) fn python_total_seconds(delta: chrono::TimeDelta) -> f64 {
    delta
        .num_microseconds()
        .map(|micros| micros as f64 / 1e6)
        .unwrap_or_else(|| delta.num_seconds() as f64)
}

/// CPython `datetime.fromtimestamp`'s split of an epoch float into
/// whole seconds and half-even-rounded microseconds.
pub(super) fn epoch_to_parts(epoch: f64) -> (i64, u32) {
    let mut secs = epoch.trunc() as i64;
    let mut micros = round_half_even((epoch - epoch.trunc()) * 1e6, 0) as i64;
    if micros >= 1_000_000 {
        secs += 1;
        micros -= 1_000_000;
    } else if micros < 0 {
        secs -= 1;
        micros += 1_000_000;
    }
    (secs, micros as u32)
}

/// The original's naive-local `datetime.timestamp()` (fold=0): the
/// earliest interpretation for ambiguous instants; an instant inside
/// a DST gap resolves through the neighbouring hour's offset.
pub fn naive_to_epoch(instant: NaiveDateTime) -> f64 {
    let resolved = match instant.and_local_timezone(chrono::Local) {
        chrono::LocalResult::Single(instant) => Some(instant),
        chrono::LocalResult::Ambiguous(earliest, _) => Some(earliest),
        chrono::LocalResult::None => (instant + chrono::TimeDelta::hours(1))
            .and_local_timezone(chrono::Local)
            .earliest()
            .map(|shifted| shifted - chrono::TimeDelta::hours(1)),
    };
    resolved
        .map(|instant| {
            instant.timestamp() as f64 + f64::from(instant.timestamp_subsec_micros()) / 1e6
        })
        .unwrap_or(0.0)
}

/// The original's naive-local `datetime.fromtimestamp()`.
pub(super) fn epoch_to_naive(epoch: f64) -> NaiveDateTime {
    let (secs, micros) = epoch_to_parts(epoch);
    chrono::DateTime::from_timestamp(secs, micros * 1_000)
        .map(|instant| instant.with_timezone(&chrono::Local).naive_local())
        .unwrap_or_default()
}

/// `datetime.isoformat()` for naive instants (ledger dates, the
/// readout's started_at): microseconds only when non-zero.
/// A naive `datetime.isoformat()`: no offset suffix, microseconds only
/// when non-zero. The start/stop producer routes render
/// `session.start_time.isoformat()` / `session.end_time.isoformat()`
/// through this, exactly as the snapshot renders `started_at`.
pub fn naive_isoformat(instant: NaiveDateTime) -> String {
    if instant.and_utc().timestamp_subsec_micros() == 0 {
        instant.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        instant.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
    }
}

/// The original `to_iso_utc` helper: render an epoch float as
/// `datetime.fromtimestamp(ts, tz=UTC).isoformat()` does (the `T`
/// separator, microseconds only when non-zero, `+00:00` suffix).
pub fn to_iso_utc(ts: f64) -> String {
    let (secs, micros) = epoch_to_parts(ts);
    let instant = chrono::DateTime::from_timestamp(secs, micros * 1_000).unwrap_or_default();
    if micros == 0 {
        format!("{}+00:00", instant.format("%Y-%m-%dT%H:%M:%S"))
    } else {
        format!("{}+00:00", instant.format("%Y-%m-%dT%H:%M:%S%.6f"))
    }
}
