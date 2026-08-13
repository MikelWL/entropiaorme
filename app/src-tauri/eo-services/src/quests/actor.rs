//! The quest service's owning task: the session-tracking state lives
//! here, and the bus-fed flows (session start/stop, mission
//! auto-start) serialise with the watcher's reward-filter calls
//! through one typed message channel. There is no lock; exclusive
//! access IS the task, and inside it the database is simply awaited.
//!
//! Messaging uses call semantics for the same reasons the tracker's
//! actor does: a bus publisher returning from `publish` has always
//! meant the quest flow fully absorbed the event, persistence
//! included (the replay corpus freezes the resulting database state),
//! and the watcher's filter must answer before its tick publishes, so
//! the suppression decision is inherently synchronous. The quest flows
//! publish nothing back onto the bus, so the tracker's self-rendezvous
//! guard has no counterpart here.
//!
//! The subscriptions are permanent (the original subscribes once in
//! its constructor and never unsubscribes): the registrations live in
//! the task, whose forwarders hold its senders, so pump and
//! subscriptions share the composition's lifetime by construction.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};

use crate::bus_events::BusEvent;
use crate::event_bus::{EventBus, Registration, Topic};

use super::QuestService;

/// One message into the owning task. Every variant carries its reply;
/// the sender waits (see the module doc for why calls are synchronous).
pub(super) enum QuestMsg {
    /// A bus event forwarded by a permanent subscription; the reply
    /// closes the rendezvous once the event is fully absorbed.
    Event(BusEvent, oneshot::Sender<()>),
    /// A chat-log MISSION_COMPLETE tick asking which loot item or
    /// skill gain to suppress so the reward is not double-counted.
    RewardFilter {
        mission_name: String,
        loot_items: Vec<Value>,
        skill_gains: Vec<Value>,
        isolated_completion_tick: bool,
        reply: oneshot::Sender<Option<Value>>,
    },
    SignalRewardFilter {
        loot_items: Vec<Value>,
        reply: oneshot::Sender<Option<Value>>,
    },
}

/// Install the permanent bus forwarders. Each forwarder performs the
/// rendezvous documented on [`QuestMsg`]: enqueue, then wait for the
/// task's completion reply, yielding its runtime slot when the
/// publisher is a runtime worker and parking directly on a plain
/// producer thread (the chat-log tail).
pub(super) fn subscribe_handlers(
    bus: &Arc<EventBus>,
    pump: &mpsc::UnboundedSender<QuestMsg>,
) -> Vec<(Topic, Registration)> {
    [
        Topic::SessionStarted,
        Topic::SessionStopped,
        Topic::MissionReceived,
    ]
    .into_iter()
    .map(|topic| {
        let sender = pump.clone();
        let registration = bus.subscribe(topic, move |event| {
            let (done_tx, done_rx) = oneshot::channel();
            if sender
                .send(QuestMsg::Event(event.clone(), done_tx))
                .is_err()
            {
                return;
            }
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| {
                    let _ = done_rx.blocking_recv();
                });
            } else {
                let _ = done_rx.blocking_recv();
            }
        });
        (topic, registration)
    })
    .collect()
}

/// The state-owning task: keep the session watch current and serve
/// messages for the composition's lifetime. The registrations are held
/// here so the subscriptions last exactly as long as the task.
pub(super) async fn run(
    service: Arc<QuestService>,
    mut inbox: mpsc::UnboundedReceiver<QuestMsg>,
    session: watch::Sender<Option<String>>,
    _subscriptions: Vec<(Topic, Registration)>,
) {
    while let Some(message) = inbox.recv().await {
        match message {
            QuestMsg::Event(event, done) => {
                dispatch_event(&service, &session, &event).await;
                let _ = done.send(());
            }
            QuestMsg::RewardFilter {
                mission_name,
                loot_items,
                skill_gains,
                isolated_completion_tick,
                reply,
            } => {
                // A filter error surfaces as no suppression, exactly
                // as the original contains a filter exception.
                let result = service
                    .quest_reward_filter_with_context(
                        &mission_name,
                        &loot_items,
                        &skill_gains,
                        isolated_completion_tick,
                    )
                    .await
                    .unwrap_or(None);
                let _ = reply.send(result);
            }
            QuestMsg::SignalRewardFilter { loot_items, reply } => {
                let result = service
                    .signal_reward_filter(&loot_items)
                    .await
                    .unwrap_or(None);
                let _ = reply.send(result);
            }
        }
    }
}

/// Route one forwarded bus event: session start/stop move the watch,
/// and a received mission auto-starts its matching quest.
async fn dispatch_event(
    service: &QuestService,
    session: &watch::Sender<Option<String>>,
    event: &BusEvent,
) {
    match event {
        BusEvent::SessionStarted(payload) => {
            let _ = session.send(Some(payload.session_id.clone()));
        }
        BusEvent::SessionStopped(_) => {
            let _ = session.send(None);
        }
        // A nameless mission is ignored; a start failure surfaces
        // nowhere, exactly as the original's bus contains a handler
        // exception.
        BusEvent::MissionReceived(payload) if !payload.mission_name.is_empty() => {
            let _ = service
                .start_quest_from_mission(&payload.mission_name)
                .await;
        }
        _ => {}
    }
}
