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

use std::sync::{Arc, Mutex, MutexGuard};

use tokio::runtime::Handle;

use crate::bus_events::BusEvent;
use crate::clock::Clock;
use crate::db::Db;
use crate::event_bus::{EventBus, Registration, Topic};

/// The service's error surface: `Invalid` carries the original's
/// raised-exception messages (its `ValueError` texts verbatim; the
/// null-list refusals name crashes the original leaves unworded). The
/// quest router leaves these unhandled, so they surface as 500s, not
/// 400s; the future router slice must preserve that. `Db` is a
/// database failure (also 500).
#[derive(Debug, thiserror::Error)]
pub enum QuestError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// A daily-rollup refresh failure inside a quest write.
    #[error(transparent)]
    Rollup(#[from] crate::db::DbError),
}

/// Quest operations: CRUD, playlists, the completion lifecycle,
/// chat-log mission detection, and the analytics readers.
pub struct QuestService {
    db: Db,
    clock: Arc<dyn Clock>,
    /// The active tracking session, fed by the bus handlers.
    current_session_id: Mutex<Option<String>>,
    /// The identifier source for ledger rows and session-less
    /// completion keys (random by default; injected by the tests so the
    /// committed goldens stamp the same identifiers).
    id_source: Mutex<Arc<dyn Fn() -> String + Send + Sync>>,
    /// The runtime the bus handlers bridge their database work onto,
    /// set when the service subscribes.
    runtime: Mutex<Option<Handle>>,
    /// Held for the service's lifetime: the original subscribes once
    /// in its constructor and never unsubscribes.
    _subscriptions: Mutex<Vec<(Topic, Registration)>>,
}

impl QuestService {
    pub fn new(db: Db, clock: Arc<dyn Clock>) -> Self {
        Self {
            db,
            clock,
            current_session_id: Mutex::new(None),
            id_source: Mutex::new(Arc::new(|| uuid::Uuid::new_v4().to_string())),
            runtime: Mutex::new(None),
            _subscriptions: Mutex::new(Vec::new()),
        }
    }

    /// Replace the identifier source (tests and the differential).
    pub fn set_id_source(&self, source: Arc<dyn Fn() -> String + Send + Sync>) {
        *self
            .id_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = source;
    }

    pub(super) fn next_id(&self) -> String {
        let source = self
            .id_source
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        source()
    }

    /// The session guard, tolerating poison: a contained panic must
    /// not brick the service.
    pub(super) fn lock_session(&self) -> MutexGuard<'_, Option<String>> {
        self.current_session_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Subscribe to the bus (the original's constructor-time
    /// subscriptions): session start/stop track the active session,
    /// and a received mission auto-starts its matching quest.
    pub fn subscribe(self: &Arc<Self>, bus: &Arc<EventBus>, runtime: Handle) {
        *self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(runtime);
        type Handler = fn(&QuestService, &BusEvent);
        let pairs: [(Topic, Handler); 3] = [
            (Topic::SessionStarted, Self::on_session_start),
            (Topic::SessionStopped, Self::on_session_stop),
            (Topic::MissionReceived, Self::on_mission_received),
        ];
        let mut subscriptions = Vec::new();
        for (topic, handler) in pairs {
            let subscriber = self.clone();
            let registration = bus.subscribe(topic, move |data| handler(&subscriber, data));
            subscriptions.push((topic, registration));
        }
        *self
            ._subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = subscriptions;
    }

    /// Bridge a database future from either calling context (the
    /// tracker's dual shape).
    fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        let handle = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("a subscribed service carries its runtime");
        if Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| handle.block_on(future))
        } else {
            handle.block_on(future)
        }
    }

    fn on_session_start(&self, event: &BusEvent) {
        let BusEvent::SessionStarted(payload) = event else {
            return;
        };
        *self.lock_session() = Some(payload.session_id.clone());
    }

    fn on_session_stop(&self, _event: &BusEvent) {
        *self.lock_session() = None;
    }

    fn on_mission_received(&self, event: &BusEvent) {
        let BusEvent::MissionReceived(payload) = event else {
            return;
        };
        if !payload.mission_name.is_empty() {
            // A failure surfaces nowhere, exactly as the original's
            // bus contains a handler exception.
            let _ = self.block_on(self.start_quest_from_mission(&payload.mission_name));
        }
    }
}
