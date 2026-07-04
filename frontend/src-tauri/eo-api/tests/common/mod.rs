//! Shared test scaffolding for the facade integration tests.
//!
//! The facade constructor takes the migrated write families' producer
//! handles (the config writer, the hunt tracker, the hotbar gate, the
//! chat-log watcher, the skill tracker) by value under the
//! construct-then-share invariant. Families that do not exercise those
//! surfaces still need them present, so this builds a minimal, inert set
//! over a fresh event bus: the listeners spawn no threads until started,
//! and the trackers are constructed but never driven.

use std::path::Path;
use std::sync::{Arc, Mutex};

use eo_services::chatlog_watcher::ChatlogWatcher;
use eo_services::clock::RealClock;
use eo_services::config_service::ConfigService;
use eo_services::db::Db;
use eo_services::event_bus::EventBus;
use eo_services::hotbar_listener::HotbarListener;
use eo_services::skill_tracker::SkillTracker;
use eo_services::tracker::{HuntTracker, Providers};

/// The five write-family producer handles a facade under test takes by
/// value.
pub struct ProducerHandles {
    pub config_service: Arc<Mutex<ConfigService>>,
    pub tracker: Arc<HuntTracker>,
    pub hotbar: Arc<HotbarListener>,
    pub watcher: Arc<ChatlogWatcher>,
    pub skill_tracker: Arc<SkillTracker>,
}

/// Build the write-family producer handles for a facade under test.
/// `handle` is the runtime the trackers schedule onto (`Handle::current()`
/// inside a `#[tokio::test]`, or a built runtime's handle in a sync
/// harness). The trackers read over their own pool handles so the caller's
/// database is left to move into the facade.
pub fn producer_handles(
    db: &Db,
    data_dir: &Path,
    handle: tokio::runtime::Handle,
) -> ProducerHandles {
    let bus = Arc::new(EventBus::new());
    let config_service = Arc::new(Mutex::new(
        ConfigService::new(data_dir).expect("config service"),
    ));
    let tracker = HuntTracker::new(
        bus.clone(),
        Db::from_pool(db.write().clone()),
        handle.clone(),
        Arc::new(RealClock::new()),
        Providers::default(),
    )
    .expect("tracker");
    let hotbar = HotbarListener::new(bus.clone(), None, None);
    let watcher = Arc::new(ChatlogWatcher::new(
        bus.clone(),
        data_dir.join("chat.log"),
        None,
    ));
    let skill_tracker = SkillTracker::new(
        &bus,
        Db::from_pool(db.write().clone()),
        handle,
        Arc::new(RealClock::new()),
    );
    ProducerHandles {
        config_service,
        tracker,
        hotbar,
        watcher,
        skill_tracker,
    }
}
