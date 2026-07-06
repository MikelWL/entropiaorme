//! Quest service, ported from the original Python implementation:
//! the quest and playlist CRUD surface with its shared helper layer
//! (row shaping, cooldown derivation, reward-markup normalisation,
//! mob and playlist-item management), plus the lifecycle actions
//! (start/complete/cancel with ledger and claim integration), the
//! curated session-link suggestions, and the chat-log mission
//! detection (auto-start, auto-complete, and reward suppression),
//! and the analytics readers (per-quest and per-playlist
//! sustainability metrics over curated session links).
//!
//! Payload semantics mirror the original's `dict.get` rules exactly: a
//! key that is ABSENT takes the documented default, while a key that is
//! PRESENT binds its value even when null (the original passes the
//! explicit `None` through). Truthiness gates (`reward_is_skill`, the
//! mobs list) follow Python falsiness: null, false, zero, and empty
//! strings/arrays/objects all read as false.
//!
//! Row values surface with their stored types (`reward_is_skill` and
//! `is_active` as 0/1 integers, ids as integers), exactly as the
//! original's `dict(row)` does; the camelCase wire shaping lives in the
//! router layer, not here.
//!
//! One `QuestService` exists per composition, started with [`start`]
//! (`QuestService::start`): the bus-fed flows (session tracking,
//! mission auto-start) and the watcher's reward-filter calls serialise
//! through a single owning task (see `actor`), while the CRUD, linking,
//! and analytics surfaces are plain `&self` async reads and writes over
//! the shared database handles.

mod actor;
mod analytics;
mod crud;
mod lifecycle;
mod linking;
mod missions;
mod payload;
mod playlists;
#[cfg(test)]
mod tests;

pub use missions::{normalize_quest_name, FUZZY_THRESHOLD};
pub use playlists::{PLAYLIST_GROUP_IMMEDIATE, PLAYLIST_GROUP_LONG_HORIZON};

use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};

use crate::chatlog_watcher::QuestRewardFilter;
use crate::clock::Clock;
use crate::db::Db;
use crate::event_bus::EventBus;

use actor::QuestMsg;

/// The identifier source for ledger rows and session-less completion
/// keys (random by default; injected by the tests so the committed
/// goldens stamp the same identifiers).
pub type IdSource = Arc<dyn Fn() -> String + Send + Sync>;

/// The service's error surface: `Invalid` carries the rejection
/// messages verbatim as the frozen goldens pin them (including the
/// null-list refusal texts the original implementation left unworded).
/// By owned contract, the typed facade maps every `QuestError` variant
/// (`Invalid` and `Db` alike) to its internal-error reply; this family
/// deliberately has no bad-request arm (see `eo-api`'s quests module).
#[derive(Debug, thiserror::Error)]
pub enum QuestError {
    #[error("{0}")]
    Invalid(String),
    /// A daily-rollup refresh failure inside a quest write.
    #[error(transparent)]
    Rollup(#[from] crate::db::DbError),
}

/// Quest operations: CRUD, playlists, the completion lifecycle,
/// chat-log mission detection, and the analytics readers.
pub struct QuestService {
    db: Db,
    clock: Arc<dyn Clock>,
    id_source: IdSource,
    /// The active tracking session, kept current by the owning task;
    /// reads are lock-free snapshots.
    session: watch::Receiver<Option<String>>,
    /// The sender into the owning task, for the watcher's
    /// reward-filter rendezvous.
    pump: mpsc::UnboundedSender<QuestMsg>,
}

impl QuestService {
    /// Start the quest service: subscribe the permanent bus forwarders
    /// (session start/stop track the active session, and a received
    /// mission auto-starts its matching quest, exactly the original's
    /// constructor-time subscriptions) and spawn the owning task on
    /// `runtime`.
    pub fn start(bus: &Arc<EventBus>, db: Db, clock: Arc<dyn Clock>, runtime: Handle) -> Arc<Self> {
        Self::start_with_id_source(
            bus,
            db,
            clock,
            runtime,
            Arc::new(|| uuid::Uuid::new_v4().to_string()),
        )
    }

    /// [`start`](Self::start) with an explicit identifier source (the
    /// tests pin deterministic ledger and completion keys).
    pub fn start_with_id_source(
        bus: &Arc<EventBus>,
        db: Db,
        clock: Arc<dyn Clock>,
        runtime: Handle,
        id_source: IdSource,
    ) -> Arc<Self> {
        let (pump, inbox) = mpsc::unbounded_channel();
        let (session_tx, session_rx) = watch::channel(None);
        let service = Arc::new(Self {
            db,
            clock,
            id_source,
            session: session_rx,
            pump: pump.clone(),
        });
        let subscriptions = actor::subscribe_handlers(bus, &pump);
        runtime.spawn(actor::run(
            service.clone(),
            inbox,
            session_tx,
            subscriptions,
        ));
        service
    }

    pub(super) fn next_id(&self) -> String {
        (self.id_source)()
    }

    /// The service's one local-to-instant boundary: the injected
    /// clock's wall-clock reading resolved to the instant it names
    /// (the fold=0 rule), as the epoch-seconds float the database
    /// stores. Every timestamp the service stamps or compares passes
    /// through here; renders back to wire form go through
    /// [`crate::time::to_iso_utc`].
    pub(super) fn now_epoch(&self) -> f64 {
        crate::time::instant_to_epoch(crate::time::resolve_local(self.clock.now()))
    }

    /// A snapshot of the active tracking session id (the owning task
    /// keeps the watch current; an empty string passes through, and
    /// the truthiness gates downstream treat it as no session).
    pub(super) fn current_session(&self) -> Option<String> {
        self.session.borrow().clone()
    }

    /// The chat-log watcher's reward-filter seam: a synchronous closure
    /// the watcher invokes from its tail thread on a MISSION_COMPLETE
    /// tick. Each call is a rendezvous into the owning task (enqueue,
    /// wait for the reply), so filter decisions serialise with the
    /// session events the task owns; the tick does not publish until
    /// the filter has answered, preserving the original's synchronous
    /// suppression contract. A filter error surfaces as no suppression,
    /// exactly as the original contains a filter exception.
    pub fn watcher_filter(&self) -> QuestRewardFilter {
        let pump = self.pump.clone();
        Arc::new(move |mission_name, loot_items, skill_gains| {
            let (reply_tx, reply_rx) = oneshot::channel();
            let message = QuestMsg::RewardFilter {
                mission_name: mission_name.to_string(),
                loot_items: loot_items.to_vec(),
                skill_gains: skill_gains.to_vec(),
                reply: reply_tx,
            };
            if pump.send(message).is_err() {
                return None;
            }
            let result = if Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| reply_rx.blocking_recv())
            } else {
                reply_rx.blocking_recv()
            };
            result.unwrap_or(None)
        })
    }
}
