//! Shared test scaffolding for the facade integration tests.
//!
//! The facade constructor takes the settings-write producer handles (the
//! config writer, the tracker, the hotbar gate, the chat-log watcher) by
//! value under the construct-then-share invariant. Families that do not
//! exercise the settings surface still need them present, so this builds a
//! minimal, inert set over a fresh event bus: the listeners spawn no
//! threads until started, and the tracker is constructed but never driven.

use std::path::Path;
use std::sync::{Arc, Mutex};

use eo_services::chatlog_watcher::ChatlogWatcher;
use eo_services::clock::RealClock;
use eo_services::config_service::ConfigService;
use eo_services::db::Db;
use eo_services::event_bus::EventBus;
use eo_services::hotbar_listener::HotbarListener;
use eo_services::tracker::{HuntTracker, Providers};

/// Build the four settings-write producer handles for a facade under test.
/// `handle` is the runtime the tracker schedules onto (`Handle::current()`
/// inside a `#[tokio::test]`, or a built runtime's handle in a sync
/// harness). The tracker reads over its own pool handle so the caller's
/// database is left to move into the facade.
#[allow(clippy::type_complexity)]
pub fn producer_handles(
    db: &Db,
    data_dir: &Path,
    handle: tokio::runtime::Handle,
) -> (
    Arc<Mutex<ConfigService>>,
    Arc<HuntTracker>,
    Arc<HotbarListener>,
    Arc<ChatlogWatcher>,
) {
    let bus = Arc::new(EventBus::new());
    let config_service = Arc::new(Mutex::new(
        ConfigService::new(data_dir).expect("config service"),
    ));
    let tracker = HuntTracker::new(
        bus.clone(),
        Db::from_pool(db.write().clone()),
        handle,
        Arc::new(RealClock::new()),
        Providers::default(),
    )
    .expect("tracker");
    let hotbar = HotbarListener::new(bus.clone(), None, None);
    let watcher = Arc::new(ChatlogWatcher::new(bus.clone(), data_dir.join("chat.log"), None));
    (config_service, tracker, hotbar, watcher)
}
