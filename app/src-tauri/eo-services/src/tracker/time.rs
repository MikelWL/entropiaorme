//! The tracker's time layer: the payload-parsing helpers only the
//! tracker uses, over the shared CPython-compatible conversions in
//! [`crate::time`] (re-exported here so the tracker's modules keep one
//! `time::` path for both).

#[cfg(test)]
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
#[cfg(test)]
use serde_json::Value;

pub(super) use crate::time::{
    epoch_to_instant, instant_to_epoch, local_isoformat, resolve_local, to_iso_utc,
};

use crate::chatlog_time::ChatLogClock;
#[cfg(test)]
use crate::chatlog_time::ChatLogReading;
use crate::time::parse_timestamp_str;

/// Bus payload timestamps are the watcher's isoformat strings (whole
/// seconds; a fractional suffix is tolerated for symmetry with the
/// harness normaliser); the original receives `datetime` objects on
/// its in-process bus. Typed payloads carry their timestamps as
/// `String` now, so only the pinning test still reads through `Value`.
#[cfg(test)]
pub(crate) fn parse_bus_timestamp(value: Option<&Value>) -> Option<NaiveDateTime> {
    parse_timestamp_str(value?.as_str()?).map(ChatLogReading::as_naive)
}

/// The payload timestamp form parsed and resolved to the instant it
/// names. The reading is the game server's wall clock, so the chat-log
/// clock owns the conversion; see [`crate::chatlog_time`].
pub(super) fn parse_timestamp_instant(
    chatlog_clock: &ChatLogClock,
    raw: &str,
) -> Option<DateTime<Utc>> {
    parse_timestamp_str(raw).map(|reading| chatlog_clock.resolve(reading))
}

/// `timedelta.total_seconds()`.
pub(super) fn python_total_seconds(delta: chrono::TimeDelta) -> f64 {
    delta
        .num_microseconds()
        .map(|micros| micros as f64 / 1e6)
        .unwrap_or_else(|| delta.num_seconds() as f64)
}

/// The original's naive-local `datetime.fromtimestamp()` (test
/// assertions only; the interior carries instants).
#[cfg(test)]
pub(super) fn epoch_to_naive(epoch: f64) -> NaiveDateTime {
    let (secs, micros) = crate::time::epoch_to_parts(epoch);
    chrono::DateTime::from_timestamp(secs, micros * 1_000)
        .map(|instant| instant.with_timezone(&chrono::Local).naive_local())
        .unwrap_or_default()
}
