//! The chat-log time base: what a chat-log timestamp actually names,
//! and how it becomes an instant.
//!
//! The game stamps its chat log in the game server's own zone. Every
//! player reads the same digits for the same event whatever their own
//! machine is set to, so a reading is **not** a host wall-clock
//! reading and must not resolve through [`crate::time::resolve_local`].
//! Resolving one that way puts every logged event out by the
//! difference between the two zones, which is invisible until
//! something compares a logged instant against one the app itself
//! stamped: a British machine in summer against a UTC server reads
//! every kill and every loot clump an hour into the past, while the
//! session, interval, and quest-run boundaries beside them are stamped
//! from the host clock and are correct.
//!
//! Neither zone is knowable from configuration that stays true. The
//! host's offset moves with travel and with its own daylight saving;
//! the server's moves with the server's. What the conversion needs is
//! the server's offset from UTC, and that is directly observable: the
//! watcher tails from end-of-file on a short poll, so every line it
//! reads was appended moments earlier, and the gap between a reading
//! taken as UTC and the instant it was read is that offset plus the
//! small read lag. Real zone offsets are whole quarter-hours, so
//! snapping the gap to the nearest quarter-hour recovers the offset
//! exactly and discards the lag.
//!
//! Deriving against UTC rather than against the host zone is
//! deliberate: the host's zone has no part in what a server-stamped
//! reading means, and leaving it out of the conversion also leaves out
//! the host's own daylight-saving gaps and ambiguities, which would
//! otherwise smear an hour across a reading that was never in doubt.
//!
//! [`ChatLogReading`] is a newtype for one reason: it is the only
//! thing [`crate::time::parse_timestamp_str`] hands back, and it
//! offers no route to an instant except [`ChatLogClock::resolve`]. A
//! later reader cannot reach for `resolve_local` out of habit and
//! quietly reintroduce the skew.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};

/// Real UTC offsets are whole quarter-hours (+05:30, +05:45, +12:45
/// all exist), so a derived offset that is not one is not a zone.
const QUARTER_HOUR_SECONDS: i64 = 15 * 60;

/// The widest real offset from UTC in either direction (+14:00 for
/// Kiritimati, -12:00 for Baker Island).
const MAX_OFFSET_SECONDS: i64 = 14 * 3600;

/// How far a sample may sit from the quarter-hour it snaps to before
/// it stops being evidence. A live tail lands within a second or two;
/// anything further out was not read live (a suspended host, a stalled
/// share, a writer that paused mid-flush) and says nothing true about
/// the server's zone.
const MAX_READ_LAG_SECONDS: i64 = 120;

/// Consecutive agreeing samples needed to move an offset that has
/// already settled. One sample settles an unset clock, because running
/// knowingly wrong is worse than acting on good early evidence; moving
/// a settled one makes a daylight-saving step prove itself rather than
/// following a single odd reading.
const CONFIRMATIONS_TO_MOVE: u32 = 3;

/// One timestamp as the chat log wrote it: the server's wall-clock
/// digits, carrying no zone of their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChatLogReading(NaiveDateTime);

impl ChatLogReading {
    pub(crate) fn new(reading: NaiveDateTime) -> Self {
        Self(reading)
    }

    /// The digits the log showed, for rendering and for the payload
    /// strings that carry a reading onward verbatim. This is not a
    /// route to an instant: resolving it against any zone but the
    /// server's is exactly the mistake the newtype exists to prevent.
    pub fn as_naive(self) -> NaiveDateTime {
        self.0
    }
}

/// Which base a reading resolves against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base {
    /// A reading is a host wall-clock reading. What every non-live
    /// surface wants: recorded lines replayed from a committed
    /// scenario are not a live tail, so no sample taken from them
    /// could say anything true about a server's zone, and pinning
    /// them to the host clock keeps a replay's output a property of
    /// the scenario rather than of the machine's calendar.
    HostLocal,
    /// A reading resolves against the server offset the live tail has
    /// derived, and against the host clock until the first sample
    /// lands (a few hundred milliseconds after the first logged line).
    Observed,
    /// A reading resolves against a known offset; nothing observed
    /// moves it.
    Pinned,
}

struct Candidate {
    offset_seconds: i64,
    agreements: u32,
}

struct Inner {
    base: Base,
    /// Seconds the server's wall clock runs ahead of UTC. Read on
    /// every resolve, so an atomic rather than a lock.
    offset_seconds: AtomicI64,
    settled: AtomicBool,
    candidate: Mutex<Option<Candidate>>,
}

/// The shared handle every chat-log consumer resolves through. Cloning
/// shares one derived offset, so the watcher's observations reach the
/// trackers that resolve the payloads it publishes.
#[derive(Clone)]
pub struct ChatLogClock {
    inner: Arc<Inner>,
}

impl ChatLogClock {
    fn with_base(base: Base, offset_seconds: i64, settled: bool) -> Self {
        Self {
            inner: Arc::new(Inner {
                base,
                offset_seconds: AtomicI64::new(offset_seconds),
                settled: AtomicBool::new(settled),
                candidate: Mutex::new(None),
            }),
        }
    }

    /// The live tail's clock: host-local until its first accepted
    /// sample, then the derived server offset.
    pub fn observed() -> Self {
        Self::with_base(Base::Observed, 0, false)
    }

    /// The historical base, for recorded lines and for any surface
    /// where "live" has no meaning.
    pub fn host_local() -> Self {
        Self::with_base(Base::HostLocal, 0, false)
    }

    /// A clock pinned to a known server offset from UTC, in seconds.
    /// A test seam for the cases that turn on a specific offset: the
    /// app derives its offset by observation and exposes no setting
    /// for one.
    pub fn pinned(offset_seconds: i64) -> Self {
        Self::with_base(Base::Pinned, offset_seconds, true)
    }

    /// The derived server offset from UTC in seconds, once one has
    /// settled. `None` while a reading still resolves host-local.
    pub fn server_offset_seconds(&self) -> Option<i64> {
        (self.inner.base != Base::HostLocal && self.inner.settled.load(Ordering::Acquire))
            .then(|| self.inner.offset_seconds.load(Ordering::Acquire))
    }

    /// Feed one live reading and the instant it was read. Samples that
    /// do not look like a live read of a real zone are discarded, so a
    /// bad one costs nothing and never unsettles a good offset.
    pub fn observe(&self, reading: ChatLogReading, read_at: DateTime<Utc>) {
        if self.inner.base != Base::Observed {
            return;
        }
        let gap = reading
            .0
            .and_utc()
            .signed_duration_since(read_at)
            .num_seconds();
        let snapped = snap_to_quarter_hour(gap);
        if snapped.abs() > MAX_OFFSET_SECONDS || (gap - snapped).abs() > MAX_READ_LAG_SECONDS {
            return;
        }

        let mut candidate = self
            .inner
            .candidate
            .lock()
            .expect("chat-log offset candidate");
        if !self.inner.settled.load(Ordering::Acquire) {
            self.inner.offset_seconds.store(snapped, Ordering::Release);
            self.inner.settled.store(true, Ordering::Release);
            *candidate = None;
            return;
        }
        if snapped == self.inner.offset_seconds.load(Ordering::Acquire) {
            *candidate = None;
            return;
        }
        match candidate.as_mut() {
            Some(pending) if pending.offset_seconds == snapped => {
                pending.agreements += 1;
                if pending.agreements >= CONFIRMATIONS_TO_MOVE {
                    self.inner.offset_seconds.store(snapped, Ordering::Release);
                    *candidate = None;
                }
            }
            _ => {
                *candidate = Some(Candidate {
                    offset_seconds: snapped,
                    agreements: 1,
                })
            }
        }
    }

    /// The instant a reading names.
    pub fn resolve(&self, reading: ChatLogReading) -> DateTime<Utc> {
        match self.server_offset_seconds() {
            Some(offset) => reading.0.and_utc() - TimeDelta::seconds(offset),
            None => crate::time::resolve_local(reading.0),
        }
    }

    /// The reading's instant as the epoch-seconds float the database
    /// stores.
    pub fn resolve_epoch(&self, reading: ChatLogReading) -> f64 {
        crate::time::instant_to_epoch(self.resolve(reading))
    }
}

impl Default for ChatLogClock {
    fn default() -> Self {
        Self::host_local()
    }
}

fn snap_to_quarter_hour(seconds: i64) -> i64 {
    let quarters = (seconds as f64 / QUARTER_HOUR_SECONDS as f64).round() as i64;
    quarters * QUARTER_HOUR_SECONDS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(raw: &str) -> ChatLogReading {
        ChatLogReading::new(
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S").expect("reading"),
        )
    }

    fn instant(raw: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
            .expect("instant")
            .and_utc()
    }

    #[test]
    fn a_utc_server_read_live_derives_a_zero_offset() {
        let clock = ChatLogClock::observed();
        // The log says 09:35:26 and we read it as it lands. The
        // server is UTC, whatever the host machine is set to.
        clock.observe(
            reading("2026-08-22T09:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(0));
        assert_eq!(
            clock.resolve(reading("2026-08-22T09:35:26")),
            instant("2026-08-22T09:35:26")
        );
    }

    #[test]
    fn a_whole_hour_server_offset_resolves_back_to_the_true_instant() {
        let clock = ChatLogClock::observed();
        // A server two hours ahead of UTC: its 11:35 is 09:35 UTC.
        clock.observe(
            reading("2026-08-22T11:35:26"),
            instant("2026-08-22T09:35:25"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(2 * 3600));
        assert_eq!(
            clock.resolve(reading("2026-08-22T11:35:26")),
            instant("2026-08-22T09:35:26")
        );
    }

    #[test]
    fn a_quarter_hour_server_offset_survives_snapping() {
        let clock = ChatLogClock::observed();
        clock.observe(
            reading("2026-08-22T15:05:26"),
            instant("2026-08-22T09:35:25"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(5 * 3600 + 30 * 60));
    }

    #[test]
    fn the_read_lag_is_discarded_not_carried_into_the_offset() {
        let clock = ChatLogClock::observed();
        // Read a minute and a half late: still the same zone.
        clock.observe(
            reading("2026-08-22T09:35:26"),
            instant("2026-08-22T09:36:56"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(0));
    }

    #[test]
    fn a_sample_that_is_not_a_live_read_is_discarded() {
        let clock = ChatLogClock::observed();
        // Mid-bucket: not a plausible zone plus a small lag.
        clock.observe(
            reading("2026-08-22T09:28:00"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), None);
        // With nothing derived, a reading still resolves the way it
        // always did rather than silently becoming UTC.
        assert_eq!(
            clock.resolve(reading("2026-08-22T09:35:26")),
            crate::time::resolve_local(reading("2026-08-22T09:35:26").as_naive())
        );
    }

    #[test]
    fn an_absurd_gap_cannot_settle_an_offset() {
        let clock = ChatLogClock::observed();
        clock.observe(
            reading("2027-08-22T09:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), None);
    }

    #[test]
    fn one_odd_sample_does_not_move_a_settled_offset() {
        let clock = ChatLogClock::observed();
        clock.observe(
            reading("2026-08-22T09:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        clock.observe(
            reading("2026-08-22T10:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(0));
    }

    #[test]
    fn a_daylight_saving_step_moves_the_offset_once_it_is_confirmed() {
        let clock = ChatLogClock::observed();
        clock.observe(
            reading("2026-10-25T01:00:00"),
            instant("2026-10-25T01:00:00"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(0));
        for minute in 0..CONFIRMATIONS_TO_MOVE {
            let read_at = instant("2026-10-25T02:00:00") + TimeDelta::minutes(minute.into());
            clock.observe(
                ChatLogReading::new((read_at + TimeDelta::hours(1)).naive_utc()),
                read_at,
            );
        }
        assert_eq!(clock.server_offset_seconds(), Some(3600));
    }

    #[test]
    fn an_interrupted_run_of_samples_restarts_the_confirmation() {
        let clock = ChatLogClock::observed();
        clock.observe(
            reading("2026-08-22T09:00:00"),
            instant("2026-08-22T09:00:00"),
        );
        clock.observe(
            reading("2026-08-22T10:00:01"),
            instant("2026-08-22T09:00:01"),
        );
        clock.observe(
            reading("2026-08-22T11:00:02"),
            instant("2026-08-22T09:00:02"),
        );
        clock.observe(
            reading("2026-08-22T10:00:03"),
            instant("2026-08-22T09:00:03"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(0));
    }

    #[test]
    fn a_host_local_clock_never_observes() {
        let clock = ChatLogClock::host_local();
        clock.observe(
            reading("2026-08-22T11:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), None);
        assert_eq!(
            clock.resolve(reading("2026-08-22T09:35:26")),
            crate::time::resolve_local(reading("2026-08-22T09:35:26").as_naive())
        );
    }

    #[test]
    fn a_pinned_clock_ignores_what_the_tail_sees() {
        let clock = ChatLogClock::pinned(3600);
        clock.observe(
            reading("2026-08-22T09:35:26"),
            instant("2026-08-22T09:35:26"),
        );
        assert_eq!(clock.server_offset_seconds(), Some(3600));
        assert_eq!(
            clock.resolve(reading("2026-08-22T10:35:26")),
            instant("2026-08-22T09:35:26")
        );
    }
}
