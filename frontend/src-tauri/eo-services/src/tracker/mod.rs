//! The hunt tracker: the central coordinator that subscribes to the
//! bus, accumulates combat stats, creates kill records on loot events,
//! and persists to the database. Ported from the original Python
//! implementation, then re-founded: the session state is a typestate
//! (`Idle | Active(ActiveSession)`), and the state is owned by a
//! single actor task fed over a typed message channel (see `actor`)
//! rather than shared behind a mutex. `HuntTracker` is the handle:
//! commands are async message calls, and the two hot predicates read
//! a watch channel the actor keeps current.
//!
//! The kills model: shots accumulate with cost; a loot group is a
//! kill (snapshot the accumulator, stamp the configured mob or tag,
//! persist, reset); deaths are invisible; a session ending with
//! unresolved shots carries them as dangling cost.
//!
//! Representation differences from the original, all
//! observation-equivalent: the original's `_last_kill` alias of
//! `session.kills[-1]` is the `last_mut()` of the kills list;
//! phase-keyed tool stats live in an ordered vector rather than an
//! insertion-ordered dict; the original's logging, debug-only
//! performance counters and development-build priming hook are
//! omitted, as is its `enhancer_tt_lookup` provider (stored but never
//! read there).

mod actor;
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
pub use providers::{
    DefaultTrackingConfig, EquipmentLibrary, EquipmentProfile, InertEquipment, Providers,
    TrackingConfig,
};
pub(crate) use time::parse_timestamp_str;
pub use time::{
    epoch_to_instant, instant_to_epoch, local_isoformat, naive_isoformat, naive_to_epoch,
    resolve_local, to_iso_utc,
};

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, watch};

use crate::clock::Clock;
use crate::db::{Db, DbError};
use crate::event_bus::EventBus;
use crate::mob_lookup_service::python_whitespace;
use crate::ped::Ped;
use crate::tracking_models::TrackingSession;

use actor::{TrackerActor, TrackerMsg, TrackerStatus};
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

/// The tracker handle: an unbounded sender into the actor plus the
/// watch the actor keeps current. The composition root keeps one
/// `Arc` for the process lifetime as before.
pub struct HuntTracker {
    db: Db,
    sender: mpsc::UnboundedSender<TrackerMsg>,
    status: watch::Receiver<TrackerStatus>,
}

impl HuntTracker {
    /// Build the tracker over an already-migrated pool: spawn the
    /// state-owning actor on the current runtime and wait for its
    /// crash-orphan recovery to finish (a recovery failure surfaces
    /// here, exactly as the blocking constructor's did).
    pub async fn new(
        bus: Arc<EventBus>,
        db: Db,
        clock: Arc<dyn Clock>,
        mut providers: Providers,
    ) -> Result<Arc<Self>, DbError> {
        providers.player_name = providers
            .player_name
            .trim_matches(python_whitespace)
            .to_string();
        let (sender, inbox) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(TrackerStatus::default());
        let (ready_tx, ready_rx) = oneshot::channel();
        tokio::spawn(TrackerActor::run(
            bus,
            db.clone(),
            clock,
            providers,
            sender.clone(),
            inbox,
            status_tx,
            ready_tx,
        ));
        ready_rx.await.expect("tracker actor start")?;
        Ok(Arc::new(Self {
            db,
            sender,
            status: status_rx,
        }))
    }

    /// One message call: enqueue with a reply channel, await the reply.
    /// The actor lives as long as any handle does, so a dead channel is
    /// a bug, not a condition to handle.
    async fn call<R>(&self, build: impl FnOnce(oneshot::Sender<R>) -> TrackerMsg) -> R {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(build(reply_tx))
            .expect("tracker actor alive");
        reply_rx.await.expect("tracker actor replies")
    }

    pub fn is_tracking(&self) -> bool {
        self.status.borrow().tracking
    }

    /// Whether the active session was captured in tag mode: the
    /// per-session mode snapshotted at `start_session`, not the live
    /// config. Idle reads `false` structurally (the snapshot lives in
    /// the `Active` payload).
    pub fn is_session_tag_mode(&self) -> bool {
        let status = self.status.borrow();
        status.tracking && status.tag_mode
    }

    /// Start a new tracking session; any prior session stops first.
    pub async fn start_session(&self) -> Result<TrackingSession, DbError> {
        self.call(TrackerMsg::Start).await
    }

    /// Stop the active session (None when idle).
    pub async fn stop_session(&self) -> Result<Option<TrackingSession>, DbError> {
        self.call(TrackerMsg::Stop).await
    }

    /// Refresh trifecta-attribution and loot-filter state after config
    /// changes.
    pub async fn reload_config(&self) {
        self.call(TrackerMsg::ReloadConfig).await
    }

    /// Immediately set the active free-text tag for tag-mode kill
    /// stamping.
    pub async fn set_manual_tag(&self, tag: &str) -> Result<(), TrackerCommandError> {
        self.call(|reply| TrackerMsg::SetManualTag(tag.to_string(), reply))
            .await
    }

    /// Immediately set the active mob for manual kill stamping.
    pub async fn set_manual_mob(
        &self,
        mob_name: &str,
        species: &str,
        maturity: &str,
    ) -> Result<(), TrackerCommandError> {
        self.call(|reply| TrackerMsg::SetManualMob {
            name: mob_name.to_string(),
            species: species.to_string(),
            maturity: maturity.to_string(),
            reply,
        })
        .await
    }

    /// Clear the current mob selection, returning the released name.
    pub async fn release_current_mob(&self) -> Option<String> {
        self.call(TrackerMsg::ReleaseMob).await
    }

    /// Prime the tracker with a fully-formed demo session (guide-mode
    /// demo playback over a throwaway database only).
    pub async fn prime_demo(
        &self,
        session: TrackingSession,
        mob: MobSelection,
        mode: TrackingMode,
    ) {
        self.call(|reply| TrackerMsg::PrimeDemo {
            session,
            mob,
            mode,
            reply,
        })
        .await
    }

    /// The in-memory aggregate half of the snapshot (see
    /// `session::SessionAggregate`).
    async fn aggregate(&self) -> (Option<String>, Option<session::SessionAggregate>) {
        self.call(|reply| TrackerMsg::Aggregate(Box::new(reply)))
            .await
    }

    /// Test-only structural inspection: run a probe against the
    /// actor's owned state and return its result.
    #[cfg(test)]
    async fn inspect<R, F>(&self, probe: F) -> R
    where
        R: Send + 'static,
        F: FnOnce(&mut TrackerActor) -> R + Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(TrackerMsg::Inspect(Box::new(move |actor| {
                let _ = reply_tx.send(probe(actor));
            })))
            .expect("tracker actor alive");
        reply_rx.await.expect("tracker actor replies")
    }
}
