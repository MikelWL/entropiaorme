//! CPython-compatible time helpers: the payload timestamp form, the
//! naive-local epoch conversions (fold=0 semantics), and the isoformat
//! renderings the readout and producer routes share.

use chrono::{DateTime, NaiveDateTime, Utc};
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

/// The payload timestamp form parsed and resolved to the instant it
/// names (chat-log timestamps are local wall-clock readings).
pub(super) fn parse_timestamp_instant(raw: &str) -> Option<DateTime<Utc>> {
    parse_timestamp_str(raw).map(resolve_local)
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

/// Resolve a naive local wall-clock reading to the instant it names,
/// with the original's fold=0 rule: the earliest interpretation for
/// ambiguous readings, and a reading inside a DST gap resolved through
/// the neighbouring hour's offset. This is the tracker's one
/// local-to-instant boundary: wall-clock inputs (the injected clock,
/// chat-log timestamps) resolve here once, and the interior carries
/// the instant.
pub fn resolve_local(reading: NaiveDateTime) -> DateTime<Utc> {
    let resolved = match reading.and_local_timezone(chrono::Local) {
        chrono::LocalResult::Single(instant) => Some(instant),
        chrono::LocalResult::Ambiguous(earliest, _) => Some(earliest),
        chrono::LocalResult::None => (reading + chrono::TimeDelta::hours(1))
            .and_local_timezone(chrono::Local)
            .earliest()
            .map(|shifted| shifted - chrono::TimeDelta::hours(1)),
    };
    resolved
        .map(|instant| instant.with_timezone(&Utc))
        .unwrap_or_default()
}

/// The instant as the epoch-seconds float the database stores
/// (fractional seconds preserved).
pub fn instant_to_epoch(instant: DateTime<Utc>) -> f64 {
    instant.timestamp() as f64 + f64::from(instant.timestamp_subsec_micros()) / 1e6
}

/// The instant a stored epoch float names.
pub fn epoch_to_instant(epoch: f64) -> DateTime<Utc> {
    let (secs, micros) = epoch_to_parts(epoch);
    DateTime::from_timestamp(secs, micros * 1_000).unwrap_or_default()
}

/// Render an instant the way the naive-local surfaces always have:
/// the local wall-clock isoformat (no offset suffix, microseconds only
/// when non-zero). A reading inside a DST gap cannot round-trip (no
/// local wall clock ever showed it); the instant is the truth and the
/// render normalises.
pub fn local_isoformat(instant: DateTime<Utc>) -> String {
    naive_isoformat(instant.with_timezone(&chrono::Local).naive_local())
}

/// The original's naive-local `datetime.timestamp()` (fold=0), kept
/// for the surfaces still carrying naive readings; the tracker's
/// interior uses `resolve_local` + `instant_to_epoch` (this is their
/// composition).
pub fn naive_to_epoch(instant: NaiveDateTime) -> f64 {
    // `resolve_local` collapses an unresolvable reading to the epoch
    // origin; the original returned 0.0 there, which is the same value.
    instant_to_epoch(resolve_local(instant))
}

/// The original's naive-local `datetime.fromtimestamp()` (test
/// assertions only; the interior now carries instants).
#[cfg(test)]
pub(super) fn epoch_to_naive(epoch: f64) -> NaiveDateTime {
    let (secs, micros) = epoch_to_parts(epoch);
    chrono::DateTime::from_timestamp(secs, micros * 1_000)
        .map(|instant| instant.with_timezone(&chrono::Local).naive_local())
        .unwrap_or_default()
}

/// A naive `datetime.isoformat()`: no offset suffix, microseconds
/// only when non-zero. The instant-carrying surfaces render through
/// `local_isoformat`, which converts to the local wall clock and then
/// through this.
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
