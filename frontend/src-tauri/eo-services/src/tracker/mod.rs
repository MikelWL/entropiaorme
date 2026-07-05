//! The hunt tracker: the central coordinator that subscribes to the
//! bus, accumulates combat stats, creates kill records on loot events,
//! and persists to the database. Ported from the original Python
//! implementation, with the session state re-founded as a typestate:
//! `Idle | Active(ActiveSession)`, so the invariants the original held
//! in comments (an accumulator exists exactly while a session runs,
//! a mob stamp always carries its source) are unrepresentable when
//! violated rather than checked at each use.
//!
//! The kills model: shots accumulate with cost; a loot group is a
//! kill (snapshot the accumulator, stamp the configured mob or tag,
//! persist, reset); deaths are invisible; a session ending with
//! unresolved shots carries them as dangling cost.
//!
//! Concurrency shape: one mutex owns the in-memory session state, and
//! the original's documented invariants hold structurally here. Bus
//! publishes run only after the guard drops; the session-persistence
//! writes run after the guard drops (bridged onto the async pool
//! through a runtime handle, preserving the original's lock order:
//! the tracker lock is never held across SQLite for the tracker's own
//! writes); the provider callbacks reached from handlers may read the
//! database while the guard is held, exactly as the original's lock
//! order allows.
//!
//! Representation differences, all observation-equivalent: the
//! original's `_last_kill` alias of `session.kills[-1]` is the
//! `last_mut()` of the kills list; phase-keyed tool stats live in an
//! ordered vector rather than an insertion-ordered dict; the
//! original's logging, debug-only performance counters and
//! development-build priming hook are omitted, as is its
//! `enhancer_tt_lookup` provider (stored but never read there).

mod combat;
mod loot;
mod mob;
mod persistence;
mod providers;
mod session;
#[cfg(test)]
mod tests;
mod time;
mod weapons;

pub use mob::{MobSelection, MobSource, TrackingMode};
pub use providers::{EquipmentProfile, Providers};
pub(crate) use time::parse_timestamp_str;
pub use time::{naive_isoformat, naive_to_epoch, to_iso_utc};

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eo_wire::domain_events::{
    TrackingReason, TrackingSessionUpdated, TrackingSessionUpdatedPayload,
    TrackingSessionUpdatedTag, TrackingStatus,
};
use tokio::runtime::Handle;

use crate::bus_events::BusEvent;
use crate::clock::Clock;
use crate::db::{Db, DbError};
use crate::event_bus::{EventBus, Registration, Topic};
use crate::loot_filter::normalize_blacklist;
use crate::mob_lookup_service::python_whitespace;
use crate::ped::Ped;

use session::ActiveSession;

/// Loot groups with an identical fingerprint within this window are
/// duplicates.
pub const LOOT_DEDUP_WINDOW_SECONDS: f64 = 2.0;

/// Tagging a global/HoF onto the latest kill requires the kill to be
/// at most this many seconds away.
const GLOBAL_CORRELATION_WINDOW_SECONDS: f64 = 5.0;

/// The mob/tag command preconditions the original raises as
/// `RuntimeError`/`ValueError`; the messages match verbatim so the
/// command boundary surfaces identical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrackerCommandError {
    #[error("No active session")]
    NoActiveSession,
    #[error("Active session is not in tag mode")]
    NotTagMode,
    #[error("Tag cannot be empty")]
    EmptyTag,
    #[error("Tag mode sessions do not allow manual mob locking")]
    TagModeLocksMob,
    #[error("Manual mob entry is not enabled for this session")]
    ManualEntryDisabled,
}

/// The session typestate: everything session-scoped lives inside the
/// `Active` payload and cannot exist without a session.
#[derive(Default)]
enum SessionState {
    #[default]
    Idle,
    Active(Box<ActiveSession>),
}

impl SessionState {
    fn active(&self) -> Option<&ActiveSession> {
        match self {
            SessionState::Idle => None,
            SessionState::Active(active) => Some(active),
        }
    }

    fn active_mut(&mut self) -> Option<&mut ActiveSession> {
        match self {
            SessionState::Idle => None,
            SessionState::Active(active) => Some(active),
        }
    }
}

/// The equipped heal tool. Hotbar-equipment state, NOT session state:
/// a heal tool equipped during one session stays equipped into the
/// next (the original never reset these fields at start or stop; only
/// a heal-tool change or a trifecta reload moves them).
pub(super) struct HealTool {
    pub(super) name: Option<String>,
    pub(super) cost_per_use: Ped,
    pub(super) reload_seconds: f64,
    pub(super) amount_min: Option<f64>,
    pub(super) amount_max: Option<f64>,
}

impl Default for HealTool {
    fn default() -> Self {
        Self {
            name: None,
            cost_per_use: Ped::ZERO,
            reload_seconds: 2.5,
            amount_min: None,
            amount_max: None,
        }
    }
}

/// The in-memory state the tracker's one mutex owns: the session
/// typestate plus the two pieces that outlive a session (the loot
/// blacklist and the equipped heal tool).
#[derive(Default)]
struct TrackerState {
    session: SessionState,
    loot_blacklist: BTreeSet<String>,
    heal_tool: HealTool,
}

pub struct HuntTracker {
    bus: Arc<EventBus>,
    db: Db,
    runtime: Handle,
    clock: Arc<dyn Clock>,
    providers: Providers,
    state: Mutex<TrackerState>,
    subscriptions: Mutex<Vec<(Topic, Registration)>>,
    subscribed: AtomicBool,
}

impl HuntTracker {
    /// Build the tracker over an already-migrated pool. Recovery of
    /// crash-orphaned sessions runs here, as the original's
    /// constructor does. The handler closures hold the tracker
    /// strongly while subscribed (released on session stop), so the
    /// composition root keeps one `Arc` for the process lifetime.
    pub fn new(
        bus: Arc<EventBus>,
        db: Db,
        runtime: Handle,
        clock: Arc<dyn Clock>,
        mut providers: Providers,
    ) -> Result<Arc<Self>, DbError> {
        providers.player_name = providers
            .player_name
            .trim_matches(python_whitespace)
            .to_string();
        let state = TrackerState {
            loot_blacklist: normalize_blacklist(Some(
                providers.loot_filter_blacklist.iter().map(String::as_str),
            )),
            ..TrackerState::default()
        };

        let tracker = Arc::new(Self {
            bus,
            db,
            runtime,
            clock,
            providers,
            state: Mutex::new(state),
            subscriptions: Mutex::new(Vec::new()),
            subscribed: AtomicBool::new(false),
        });
        tracker.refresh_loot_filter_locked(&mut tracker.lock_state());
        tracker.recover_orphaned_sessions()?;
        Ok(tracker)
    }

    /// Bridge a database future onto the runtime from either calling
    /// context: a runtime worker thread (the command layer) yields its
    /// slot via `block_in_place`, while a plain producer thread (the
    /// chat-log tail, the hotbar listener) parks directly.
    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        if Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.runtime.block_on(future))
        } else {
            self.runtime.block_on(future)
        }
    }

    /// The state guard, tolerating poison: a panicking provider or
    /// cost computation must not brick the tracker, mirroring the
    /// original's per-event exception containment (its state stays
    /// serviceable after a contained failure).
    fn lock_state(&self) -> std::sync::MutexGuard<'_, TrackerState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn is_tracking(&self) -> bool {
        self.lock_state().session.active().is_some()
    }

    /// Whether the active session was captured in tag mode: the
    /// per-session mode snapshotted at `start_session`, not the live
    /// config. Idle reads `false` structurally (the snapshot lives in
    /// the `Active` payload); the manual-mob-suggestions handler only
    /// consults it while tracking, gating the idle case on the live
    /// config instead.
    pub fn is_session_tag_mode(&self) -> bool {
        self.lock_state()
            .session
            .active()
            .is_some_and(|active| active.mode == TrackingMode::Tag)
    }

    fn subscribe_handlers(self: &Arc<Self>) {
        if self.subscribed.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut subscriptions = self.subscriptions.lock().expect("subscriptions");
        type Handler = fn(&HuntTracker, &BusEvent);
        let pairs: [(Topic, Handler); 7] = [
            (Topic::Combat, Self::on_combat),
            (Topic::LootGroup, Self::on_loot),
            (Topic::ActiveToolChanged, Self::on_tool_changed),
            (Topic::ActiveHealToolChanged, Self::on_heal_tool_changed),
            (Topic::Global, Self::on_global),
            (Topic::EnhancerBreak, Self::on_enhancer_break),
            (Topic::TickFlushed, Self::on_tick_flushed),
        ];
        for (topic, handler) in pairs {
            let tracker = self.clone();
            let registration = self
                .bus
                .subscribe(topic, move |data| handler(&tracker, data));
            subscriptions.push((topic, registration));
        }
    }

    fn unsubscribe_handlers(&self) {
        if !self.subscribed.swap(false, Ordering::SeqCst) {
            return;
        }
        let mut subscriptions = self.subscriptions.lock().expect("subscriptions");
        for (topic, registration) in subscriptions.drain(..) {
            self.bus.unsubscribe(topic, registration);
        }
    }

    /// Publish the coarse, frontend-facing tracking.session.updated
    /// event: the typed envelope rides the bus directly, the same
    /// shape the original's model dump records and the domain bridge
    /// forwards. `occurred_at` is stamped from the domain timestamp
    /// that triggered the event, not a fresh clock read, so the event
    /// is deterministic under replay.
    fn emit_session_event(
        &self,
        reason: TrackingReason,
        status: TrackingStatus,
        occurred_ts: f64,
        session_id: Option<&str>,
    ) {
        self.bus
            .publish(&BusEvent::TrackingSessionUpdated(TrackingSessionUpdated {
                topic: TrackingSessionUpdatedTag,
                event_version: 1,
                occurred_at: to_iso_utc(occurred_ts),
                payload: TrackingSessionUpdatedPayload {
                    session_id: session_id.map(str::to_string),
                    status,
                    reason,
                },
            }));
    }

    fn refresh_loot_filter_locked(&self, state: &mut TrackerState) {
        let blacklist: Vec<String> = match &self.providers.loot_filter_blacklist_provider {
            Some(provider) => provider(),
            None => self.providers.loot_filter_blacklist.clone(),
        };
        state.loot_blacklist = normalize_blacklist(Some(blacklist.iter().map(String::as_str)));
    }
}
