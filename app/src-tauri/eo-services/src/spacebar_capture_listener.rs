//! Spacebar-capture listener: an optional hook for
//! hands-free capture during a manual skill scan.
//!
//! When enabled (the scan-overlay toggle), the listener consumes a
//! [`KeystrokeSource`] (production: the shared low-level keyboard hook
//! filtered to the space key at its boundary; tests: the mock) and, on a
//! press edge (auto-repeat suppressed via release tracking), dispatches
//! `capture_current_page` on the skill scan when it is in the `capturing`
//! phase. Idle: a no-op. Listening is pass-through (the press is not
//! consumed), so the game client still receives the keystroke. The capture
//! runs on a short-lived thread to keep the dispatch callback cheap, exactly
//! as the original offloads it. The original's logging is omitted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::keystroke_source::{KeystrokeEvent, KeystrokeKind, KeystrokeSource};
use crate::skill_scan_manual::SkillScanManual;

/// A keystroke observer (the recording controller's seam): called with
/// `(key, kind)` for each space press/release edge.
pub type KeyTap = Arc<dyn Fn(&str, &str) + Send + Sync>;

struct Flags {
    skill_enabled: AtomicBool,
    research_enabled: AtomicBool,
    source_running: AtomicBool,
    space_down: AtomicBool,
    // One-shot per start episode: whether the "first keystroke delivered"
    // breadcrumb has fired, so the spacebar faculty has a positive
    // delivery signal in the rolling logfile (mirrors the hotbar listener).
    first_delivery_logged: AtomicBool,
}

pub struct SpacebarCaptureListener {
    skill_scan: Arc<SkillScanManual>,
    source: Option<Arc<dyn KeystrokeSource>>,
    transition: Mutex<()>,
    flags: Flags,
    key_tap: Mutex<Option<KeyTap>>,
    research_capture: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl SpacebarCaptureListener {
    /// A `None` source leaves the listener inert, matching the original's
    /// missing-hook-library path. The listener subscribes to the source on
    /// construction; the source's strong handle through the subscriber
    /// closure keeps the listener alive, so only an explicit [`stop`] (or the
    /// source's own teardown) releases it.
    ///
    /// [`stop`]: SpacebarCaptureListener::stop
    pub fn new(
        skill_scan: Arc<SkillScanManual>,
        source: Option<Arc<dyn KeystrokeSource>>,
    ) -> Arc<Self> {
        let listener = Arc::new(Self {
            skill_scan,
            source: source.clone(),
            transition: Mutex::new(()),
            flags: Flags {
                skill_enabled: AtomicBool::new(false),
                research_enabled: AtomicBool::new(false),
                source_running: AtomicBool::new(false),
                space_down: AtomicBool::new(false),
                first_delivery_logged: AtomicBool::new(false),
            },
            key_tap: Mutex::new(None),
            research_capture: Mutex::new(None),
        });
        if let Some(source) = source {
            // A weak handle, so the source's subscription does not form a
            // strong cycle (`listener -> source -> callback -> listener`) that
            // `stop` cannot break. The listener's real owners (the app state
            // and the exit holder) keep it alive, so the upgrade succeeds for
            // its whole lifetime; once they drop it, the callback is inert.
            let dispatch = Arc::downgrade(&listener);
            source.subscribe(Arc::new(move |event: &KeystrokeEvent| {
                if let Some(listener) = dispatch.upgrade() {
                    listener.on_keystroke(event);
                }
            }));
        }
        listener
    }

    /// True when the keystroke source is currently delivering events.
    pub fn is_running(&self) -> bool {
        self.flags.source_running.load(Ordering::SeqCst)
    }

    /// Whether the overlay toggle is on.
    pub fn is_enabled(&self) -> bool {
        self.flags.skill_enabled.load(Ordering::SeqCst)
    }

    /// Whether the development auction-fee capture consumer is armed.
    pub fn is_research_enabled(&self) -> bool {
        self.flags.research_enabled.load(Ordering::SeqCst)
    }

    /// Install the research capture action. Composition calls this once with
    /// the actor-owned research service; the keyboard listener remains the
    /// one implementation of Space edge handling.
    pub fn set_research_capture(&self, capture: Arc<dyn Fn() + Send + Sync>) {
        *self.research_capture.lock().expect("research capture") = Some(capture);
    }

    /// Install a keystroke observer (called for each space press/release edge).
    pub fn set_key_tap(&self, tap: KeyTap) {
        *self.key_tap.lock().expect("key tap") = Some(tap);
    }

    /// Remove the keystroke observer.
    pub fn clear_key_tap(&self) {
        *self.key_tap.lock().expect("key tap") = None;
    }

    /// Toggle the listener; idempotent. Enabling starts the source, disabling
    /// stops it (the source still only delivers while a listener wants it).
    pub fn set_enabled(&self, enabled: bool) {
        let _transition = self.transition.lock().expect("spacebar transition");
        if self.flags.skill_enabled.swap(enabled, Ordering::SeqCst) == enabled {
            return;
        }
        self.reconcile_source();
    }

    /// Arm or disarm the development research consumer independently of the
    /// skill-overlay preference. The shared source stays attached while
    /// either consumer needs it.
    pub fn set_research_enabled(&self, enabled: bool) {
        let _transition = self.transition.lock().expect("spacebar transition");
        if self.flags.research_enabled.swap(enabled, Ordering::SeqCst) == enabled {
            return;
        }
        self.reconcile_source();
    }

    /// Tear down at shutdown.
    pub fn stop(&self) {
        let _transition = self.transition.lock().expect("spacebar transition");
        self.flags.skill_enabled.store(false, Ordering::SeqCst);
        self.flags.research_enabled.store(false, Ordering::SeqCst);
        self.reconcile_source();
    }

    /// Reconcile the one shared hook while holding the transition lock. The
    /// callback path reads only atomics, so stopping can join the hook worker
    /// without a lock cycle while concurrent consumers still cannot race a
    /// double-start or detach a source the other consumer needs.
    fn reconcile_source(&self) {
        let Some(source) = &self.source else {
            return;
        };
        let desired = self.flags.skill_enabled.load(Ordering::SeqCst)
            || self.flags.research_enabled.load(Ordering::SeqCst);
        let running = self.flags.source_running.load(Ordering::SeqCst);
        if desired && !running {
            // The source reports whether the underlying mechanism actually
            // attached; running honestly reflects whether events will come.
            let attached = source.start();
            self.flags.source_running.store(attached, Ordering::SeqCst);
            self.flags
                .first_delivery_logged
                .store(false, Ordering::SeqCst);
            tracing::info!(target: "eo::input", attached, "spacebar capture source start requested");
        } else if !desired && running {
            // Make callbacks inert before joining the platform worker. The
            // worker may already be dispatching, but it never waits for the
            // transition lock held here.
            self.flags.source_running.store(false, Ordering::SeqCst);
            self.flags.space_down.store(false, Ordering::SeqCst);
            source.stop();
        }
    }

    fn is_capturing(&self) -> bool {
        self.skill_scan.get_status()["phase"] == "capturing"
    }

    fn on_keystroke(&self, event: &KeystrokeEvent) {
        if !self.flags.source_running.load(Ordering::SeqCst) {
            return;
        }
        // One-shot per start: confirm the shared hook is delivering
        // keystrokes to the spacebar faculty. Non-content: no key value.
        if !self
            .flags
            .first_delivery_logged
            .swap(true, Ordering::SeqCst)
        {
            tracing::info!(
                target: "eo::input",
                "spacebar capture listener received its first keystroke since start"
            );
        }
        if event.key != "space" {
            return;
        }
        match event.kind {
            KeystrokeKind::Press => self.on_space_press(),
            KeystrokeKind::Release => self.on_space_release(),
        }
    }

    fn on_space_press(&self) {
        // Auto-repeat suppression: only the first press edge fires.
        if self.flags.space_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let tap = self.key_tap.lock().expect("key tap").clone();
        if let Some(tap) = tap {
            let _ =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tap("space", "press")));
        }
        if self.flags.research_enabled.load(Ordering::SeqCst) {
            let capture = self
                .research_capture
                .lock()
                .expect("research capture")
                .clone();
            if let Some(capture) = capture {
                let _ = std::thread::Builder::new()
                    .name("auction-fee-capture".into())
                    .spawn(move || capture());
            }
            return;
        }
        if !self.is_capturing() {
            return;
        }
        // Off-thread to keep the dispatch callback cheap, exactly as the
        // original spawns a daemon thread for the capture.
        let scan = self.skill_scan.clone();
        let _ = std::thread::Builder::new()
            .name("spacebar-capture".into())
            .spawn(move || {
                let _ = scan.capture_current_page();
            });
    }

    fn on_space_release(&self) {
        self.flags.space_down.store(false, Ordering::SeqCst);
        let tap = self.key_tap.lock().expect("key tap").clone();
        if let Some(tap) = tap {
            let _ =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tap("space", "release")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MockClock;
    use crate::keystroke_source::MockKeystrokeSource;
    use crate::skill_scan_manual::{ScanProviders, SkillScanManual};
    use chrono::{DateTime, Utc};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Barrier;
    use std::time::Duration;

    #[derive(Default)]
    struct CountingSource {
        starts: AtomicUsize,
        stops: AtomicUsize,
    }

    struct JoiningSource {
        callback: Mutex<Option<crate::keystroke_source::KeystrokeCallback>>,
        dispatch: Arc<Barrier>,
        worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    }

    impl JoiningSource {
        fn new() -> Self {
            Self {
                callback: Mutex::new(None),
                dispatch: Arc::new(Barrier::new(2)),
                worker: Mutex::new(None),
            }
        }
    }

    impl KeystrokeSource for JoiningSource {
        fn subscribe(&self, callback: crate::keystroke_source::KeystrokeCallback) {
            *self.callback.lock().unwrap() = Some(callback);
        }

        fn start(&self) -> bool {
            let callback = self.callback.lock().unwrap().clone().unwrap();
            let dispatch = self.dispatch.clone();
            *self.worker.lock().unwrap() = Some(std::thread::spawn(move || {
                dispatch.wait();
                callback(&KeystrokeEvent {
                    key: "space".into(),
                    timestamp: now(),
                    kind: KeystrokeKind::Release,
                });
            }));
            true
        }

        fn stop(&self) {
            self.dispatch.wait();
            self.worker.lock().unwrap().take().unwrap().join().unwrap();
        }
    }

    impl KeystrokeSource for CountingSource {
        fn subscribe(&self, _callback: crate::keystroke_source::KeystrokeCallback) {}

        fn start(&self) -> bool {
            self.starts.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            true
        }

        fn stop(&self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-19T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn scan() -> Arc<SkillScanManual> {
        SkillScanManual::new(
            ScanProviders {
                engine_available: Arc::new(|| true),
                skill_region: Arc::new(|| Some(([0, 0], [100, 200]))),
                capture_region: Arc::new(|_| Some(vec![1, 2, 3])),
                extract_page_levels: Arc::new(|_| Vec::new()),
            },
            Arc::new(MockClock::new(None, 0.0)),
            None,
            None,
            0,
        )
    }

    fn captured(scan: &SkillScanManual) -> i64 {
        scan.get_status()["captured_pages"].as_i64().unwrap_or(-1)
    }

    /// Wait (bounded) for the off-thread capture to land the expected count.
    fn wait_for_captures(scan: &SkillScanManual, want: i64) {
        for _ in 0..100 {
            if captured(scan) == want {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(captured(scan), want, "capture count never settled");
    }

    #[test]
    fn the_toggle_starts_and_stops_the_source() {
        let source = Arc::new(MockKeystrokeSource::new());
        let listener = SpacebarCaptureListener::new(scan(), Some(source.clone()));
        assert!(!listener.is_running());
        assert!(!listener.is_enabled());

        listener.set_enabled(true);
        assert!(listener.is_running());
        assert!(listener.is_enabled());

        // Idempotent: a second enable is a no-op.
        listener.set_enabled(true);
        assert!(listener.is_running());

        listener.set_enabled(false);
        assert!(!listener.is_running());
        assert!(!listener.is_enabled());
    }

    #[test]
    fn research_uses_the_same_space_edge_and_keeps_the_skill_toggle_independent() {
        let source = Arc::new(MockKeystrokeSource::new());
        let listener = SpacebarCaptureListener::new(scan(), Some(source.clone()));
        let captures = Arc::new(Mutex::new(0usize));
        let sink = captures.clone();
        listener.set_research_capture(Arc::new(move || {
            *sink.lock().unwrap() += 1;
        }));

        listener.set_research_enabled(true);
        assert!(listener.is_research_enabled());
        assert!(!listener.is_enabled());
        source.inject("space", now(), KeystrokeKind::Press);
        wait_until(|| *captures.lock().unwrap() == 1);
        source.inject("space", now(), KeystrokeKind::Press);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(*captures.lock().unwrap(), 1, "auto-repeat stays suppressed");
        source.inject("space", now(), KeystrokeKind::Release);
        source.inject("space", now(), KeystrokeKind::Press);
        wait_until(|| *captures.lock().unwrap() == 2);

        listener.set_enabled(true);
        listener.set_research_enabled(false);
        assert!(
            listener.is_running(),
            "the skill consumer keeps the source alive"
        );
        assert!(listener.is_enabled());
    }

    #[test]
    fn concurrent_consumers_share_one_serialised_source_lease() {
        let source = Arc::new(CountingSource::default());
        let listener = SpacebarCaptureListener::new(scan(), Some(source.clone()));
        let gate = Arc::new(Barrier::new(3));

        let skill_listener = listener.clone();
        let skill_gate = gate.clone();
        let skill = std::thread::spawn(move || {
            skill_gate.wait();
            skill_listener.set_enabled(true);
        });
        let research_listener = listener.clone();
        let research_gate = gate.clone();
        let research = std::thread::spawn(move || {
            research_gate.wait();
            research_listener.set_research_enabled(true);
        });
        gate.wait();
        skill.join().unwrap();
        research.join().unwrap();

        assert_eq!(source.starts.load(Ordering::SeqCst), 1);
        listener.set_enabled(false);
        assert!(listener.is_running());
        listener.set_research_enabled(false);
        assert!(!listener.is_running());
        assert_eq!(source.stops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stop_can_join_a_worker_dispatching_at_the_listener_boundary() {
        let source = Arc::new(JoiningSource::new());
        let listener = SpacebarCaptureListener::new(scan(), Some(source));
        listener.set_enabled(true);

        // stop releases the worker to enter the callback, then joins it. The
        // callback must not need the transition lock held by stop.
        listener.set_enabled(false);
        assert!(!listener.is_running());
    }

    fn wait_until(predicate: impl Fn() -> bool) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(predicate(), "condition never settled");
    }

    #[test]
    fn space_fires_capture_only_while_capturing_with_auto_repeat_suppressed() {
        let source = Arc::new(MockKeystrokeSource::new());
        let scan = scan();
        let listener = SpacebarCaptureListener::new(scan.clone(), Some(source.clone()));
        listener.set_enabled(true);

        // Idle scan: a space press is a no-op (no active capture target).
        source.inject("space", now(), KeystrokeKind::Press);
        source.inject("space", now(), KeystrokeKind::Release);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(captured(&scan), 0, "no capture while idle");

        // Start the scan (capturing); a press fires one capture.
        scan.start(Some(3));
        source.inject("space", now(), KeystrokeKind::Press);
        wait_for_captures(&scan, 1);

        // A second press WITHOUT a release is auto-repeat: suppressed.
        source.inject("space", now(), KeystrokeKind::Press);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(captured(&scan), 1, "auto-repeat is suppressed");

        // Release then press fires again.
        source.inject("space", now(), KeystrokeKind::Release);
        source.inject("space", now(), KeystrokeKind::Press);
        wait_for_captures(&scan, 2);
    }

    #[test]
    fn a_stopped_listener_ignores_keys_and_non_space_is_ignored() {
        let source = Arc::new(MockKeystrokeSource::new());
        let scan = scan();
        let listener = SpacebarCaptureListener::new(scan.clone(), Some(source.clone()));
        listener.set_enabled(true);
        scan.start(Some(3));

        // A non-space key never fires a capture.
        source.inject("1", now(), KeystrokeKind::Press);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(captured(&scan), 0, "non-space keys are ignored");

        // After stop the source no longer delivers, so a space press is inert.
        listener.stop();
        assert!(!listener.is_running());
        source.inject("space", now(), KeystrokeKind::Press);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(captured(&scan), 0, "a stopped listener ignores space");
    }

    #[test]
    fn the_key_tap_observes_both_edges() {
        let source = Arc::new(MockKeystrokeSource::new());
        let listener = SpacebarCaptureListener::new(scan(), Some(source.clone()));
        let taps: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = taps.clone();
        listener.set_key_tap(Arc::new(move |key: &str, kind: &str| {
            sink.lock()
                .unwrap()
                .push((key.to_string(), kind.to_string()));
        }));
        listener.set_enabled(true);

        source.inject("space", now(), KeystrokeKind::Press);
        source.inject("space", now(), KeystrokeKind::Release);
        std::thread::sleep(Duration::from_millis(20));
        let observed = taps.lock().unwrap().clone();
        assert_eq!(
            observed,
            vec![
                ("space".to_string(), "press".to_string()),
                ("space".to_string(), "release".to_string()),
            ]
        );

        listener.clear_key_tap();
        source.inject("space", now(), KeystrokeKind::Press);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(taps.lock().unwrap().len(), 2, "a cleared tap stays silent");
    }
}
