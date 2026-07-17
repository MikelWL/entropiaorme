//! Coordinate capture: reading the avatar's on-screen position (the
//! minimap's longitude/latitude/altitude readout) for one-click map
//! pins, plus the guided two-point calibration that defines where that
//! readout sits on screen.
//!
//! The minimap is freely movable and resizable, so the capture
//! rectangle is per-user state, not a constant. Calibration is a
//! two-step flow: the user hovers the readout's top-left corner and
//! presses Enter, then its bottom-right corner and presses Enter; the
//! cursor position at each press defines a corner. The completed
//! rectangle persists through an injected sink, and a validation scan
//! runs immediately so the user sees what the calibrated region reads.
//!
//! Two seams are deliberately narrow, in this service and its callers:
//!
//! - **The region provider** (`region`): how the capture rectangle is
//!   obtained is opaque to everything downstream of it. Today it reads
//!   the persisted manual calibration; an automatic UI-element locator
//!   can replace the provider wholesale, demoting manual calibration to
//!   an escape-hatch override, with no change to the scan path.
//! - **The frame reader** (`read_text`): the digit read is one closure
//!   over the shared OCR engine. A specialised recogniser replaces that
//!   single closure; capture, parsing, and the plausibility gate stay.
//!
//! Every scan validates before it answers: the text must parse as two
//! or three integer runs, and when the caller supplies the selected
//! planet's calibrated bounds the coordinates must fall inside them. An
//! implausible read is a typed refusal, never a silently-wrong pin.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::skill_panel::{digit_value, BgrImage};

/// The persisted capture rectangle, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordRegion {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// The cursor-position seam (calibration corners come from it).
pub type CursorPosition = Arc<dyn Fn() -> Option<(i64, i64)> + Send + Sync>;
/// The region-provider seam: how the capture rectangle is obtained.
pub type RegionProvider = Arc<dyn Fn() -> Option<CoordRegion> + Send + Sync>;
/// The screen-capture seam: an `x/y/w/h` rectangle as BGR pixels.
pub type RegionCapture = Arc<dyn Fn(i64, i64, i64, i64) -> Option<BgrImage> + Send + Sync>;
/// The recognition seam: one frame to `(text, confidence)`.
pub type FrameReader = Arc<dyn Fn(&BgrImage) -> Option<(String, f64)> + Send + Sync>;
/// The persistence sink a completed calibration writes through.
pub type RegionSink = Arc<dyn Fn(CoordRegion) -> Result<(), String> + Send + Sync>;

/// The provider seams the composition root wires in.
pub struct CoordCaptureProviders {
    pub cursor_position: CursorPosition,
    pub region: RegionProvider,
    pub capture_region: RegionCapture,
    pub read_text: FrameReader,
    pub persist_region: RegionSink,
}

impl Default for CoordCaptureProviders {
    fn default() -> Self {
        Self {
            cursor_position: Arc::new(|| None),
            region: Arc::new(|| None),
            capture_region: Arc::new(|_, _, _, _| None),
            read_text: Arc::new(|_| None),
            persist_region: Arc::new(|_| Ok(())),
        }
    }
}

/// Where the calibration flow currently stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationPhase {
    Idle,
    AwaitTopLeft,
    AwaitBottomRight { top_left: (i64, i64) },
}

impl CalibrationPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalibrationPhase::Idle => "idle",
            CalibrationPhase::AwaitTopLeft => "awaitTopLeft",
            CalibrationPhase::AwaitBottomRight { .. } => "awaitBottomRight",
        }
    }
}

/// A successful coordinate read.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordRead {
    pub lon: i64,
    pub lat: i64,
    pub altitude: Option<i64>,
    pub raw_text: String,
    pub confidence: f64,
}

/// A scan's outcome: exactly one typed answer per failure leg, so the
/// UI can say precisely what went wrong (and a wrong read can never
/// masquerade as a pin).
#[derive(Debug, Clone, PartialEq)]
pub enum CoordScanOutcome {
    Read(CoordRead),
    /// No capture rectangle is available (never calibrated, or the
    /// provider stood down).
    NoRegion,
    /// The screen grab failed (capture unavailable on this host).
    CaptureFailed,
    /// The OCR engine is unavailable.
    EngineUnavailable,
    /// The captured text did not parse as a coordinate readout.
    Unreadable {
        raw_text: String,
        confidence: f64,
    },
    /// The parsed coordinates fall outside the selected planet's map
    /// bounds.
    Implausible {
        lon: i64,
        lat: i64,
        raw_text: String,
    },
}

/// A rectangle a calibration may persist: both dimensions at least this
/// many pixels (a degenerate rectangle reads nothing).
const MIN_REGION_PX: i64 = 4;

/// The largest rectangle a scan will capture per axis. The readout is a
/// small strip; a stored region beyond any plausible screen (a
/// hand-edited config) reads as uncalibrated rather than driving an
/// enormous grab.
const MAX_REGION_PX: i64 = 4096;

/// The coordinate-capture service: the calibration state machine plus
/// the one-shot scan, over the injected seams.
pub struct CoordCaptureService {
    providers: CoordCaptureProviders,
    phase: Mutex<CalibrationPhase>,
    /// The validation read taken when a calibration completes, so the
    /// UI can echo "we read: X, Y; look right?".
    last_validation: Mutex<Option<CoordScanOutcome>>,
    /// The Enter listener's gate, attached post-construction (weak: the
    /// listener holds the service strongly). The service flips it so
    /// the listener is enabled exactly while a flow is live.
    confirm_listener: Mutex<Option<std::sync::Weak<CoordConfirmListener>>>,
}

impl CoordCaptureService {
    pub fn new(providers: CoordCaptureProviders) -> Arc<Self> {
        Arc::new(Self {
            providers,
            phase: Mutex::new(CalibrationPhase::Idle),
            last_validation: Mutex::new(None),
            confirm_listener: Mutex::new(None),
        })
    }

    /// Attach the Enter listener whose gate this service drives.
    pub fn attach_confirm_listener(&self, listener: &Arc<CoordConfirmListener>) {
        *self.confirm_listener.lock().expect("listener slot") = Some(Arc::downgrade(listener));
    }

    fn set_listener_enabled(&self, enabled: bool) {
        let listener = self
            .confirm_listener
            .lock()
            .expect("listener slot")
            .as_ref()
            .and_then(std::sync::Weak::upgrade);
        if let Some(listener) = listener {
            listener.set_enabled(enabled);
        }
    }

    /// Begin (or restart) the two-point calibration flow; the Enter
    /// listener arms with it.
    pub fn calibration_start(&self) -> CalibrationPhase {
        let phase = {
            let mut phase = self.phase.lock().expect("calibration phase");
            *phase = CalibrationPhase::AwaitTopLeft;
            *self.last_validation.lock().expect("validation slot") = None;
            *phase
        };
        self.set_listener_enabled(true);
        phase
    }

    /// Abandon an in-flight flow (the persisted region is untouched);
    /// the Enter listener disarms with it.
    pub fn calibration_cancel(&self) -> CalibrationPhase {
        let phase = {
            let mut phase = self.phase.lock().expect("calibration phase");
            *phase = CalibrationPhase::Idle;
            *phase
        };
        self.set_listener_enabled(false);
        phase
    }

    pub fn calibration_phase(&self) -> CalibrationPhase {
        *self.phase.lock().expect("calibration phase")
    }

    /// The persisted capture region, through the provider seam.
    pub fn region(&self) -> Option<CoordRegion> {
        (self.providers.region)()
    }

    /// The validation read echoed after the last completed calibration.
    pub fn last_validation(&self) -> Option<CoordScanOutcome> {
        self.last_validation
            .lock()
            .expect("validation slot")
            .clone()
    }

    /// Whether an Enter press should currently be observed at all: the
    /// listener's gate, so the flow being idle means Enter presses are
    /// ignored entirely.
    pub fn calibration_active(&self) -> bool {
        !matches!(self.calibration_phase(), CalibrationPhase::Idle)
    }

    /// Advance the flow on an Enter press: capture the cursor position
    /// as the pending corner. On the second corner the rectangle is
    /// normalised (corners may be given in any order), size-checked,
    /// persisted, and a validation scan is taken. Returns the phase
    /// after the press.
    pub fn on_confirm(&self) -> CalibrationPhase {
        let cursor = (self.providers.cursor_position)();
        let mut phase = self.phase.lock().expect("calibration phase");
        let Some((cx, cy)) = cursor else {
            // No cursor position available: the flow cannot proceed on
            // this host; abandon rather than sit unfinishable.
            tracing::warn!(
                target: "eo::coord_capture",
                "cursor position unavailable; calibration abandoned"
            );
            *phase = CalibrationPhase::Idle;
            drop(phase);
            self.set_listener_enabled(false);
            return CalibrationPhase::Idle;
        };
        match *phase {
            CalibrationPhase::Idle => {}
            CalibrationPhase::AwaitTopLeft => {
                *phase = CalibrationPhase::AwaitBottomRight { top_left: (cx, cy) };
            }
            CalibrationPhase::AwaitBottomRight { top_left } => {
                let (tx, ty) = top_left;
                let region = CoordRegion {
                    x: tx.min(cx),
                    y: ty.min(cy),
                    w: (cx - tx).abs(),
                    h: (cy - ty).abs(),
                };
                if region.w < MIN_REGION_PX || region.h < MIN_REGION_PX {
                    // A degenerate rectangle re-arms the second corner
                    // rather than persisting something unreadable.
                    tracing::warn!(
                        target: "eo::coord_capture",
                        "calibration rectangle degenerate ({}x{}); second corner re-armed",
                        region.w, region.h
                    );
                    return *phase;
                }
                if let Err(err) = (self.providers.persist_region)(region) {
                    tracing::error!(
                        target: "eo::coord_capture",
                        "calibration region could not persist ({err}); flow abandoned"
                    );
                    *phase = CalibrationPhase::Idle;
                    drop(phase);
                    self.set_listener_enabled(false);
                    return CalibrationPhase::Idle;
                }
                *phase = CalibrationPhase::Idle;
                drop(phase);
                self.set_listener_enabled(false);
                // The validation echo: read the freshly calibrated
                // region once (no bounds gate; the echo shows the raw
                // read for the user to eyeball).
                let validation = self.scan(None);
                *self.last_validation.lock().expect("validation slot") = Some(validation);
                return CalibrationPhase::Idle;
            }
        }
        *phase
    }

    /// One coordinate scan through the seams: region -> capture ->
    /// read -> parse -> optional bounds gate.
    ///
    /// The game shows the readout as two stacked lines (`Lon: <value>`
    /// above `Lat: <value>`), and the recogniser is a single-line
    /// model, so the captured rectangle is split at mid-height and each
    /// line reads separately, taking the line's trailing digit run (the
    /// value follows its label, so a label misread such as `L0n` cannot
    /// inject a spurious value). A whole-rectangle single-line read
    /// remains the fallback for a readout that is not stacked.
    pub fn scan(&self, bounds: Option<CoordBounds>) -> CoordScanOutcome {
        let Some(region) = (self.providers.region)() else {
            return CoordScanOutcome::NoRegion;
        };
        if region.w <= 0 || region.h <= 0 || region.w > MAX_REGION_PX || region.h > MAX_REGION_PX {
            return CoordScanOutcome::NoRegion;
        }
        let Some(frame) = (self.providers.capture_region)(region.x, region.y, region.w, region.h)
        else {
            return CoordScanOutcome::CaptureFailed;
        };

        let mut parsed: Option<(i64, i64, Option<i64>, String, f64)> = None;
        if frame.h >= 2 {
            let (top, bottom) = split_rows(&frame);
            let Some((top_text, top_conf)) = (self.providers.read_text)(&top) else {
                return CoordScanOutcome::EngineUnavailable;
            };
            let Some((bottom_text, bottom_conf)) = (self.providers.read_text)(&bottom) else {
                return CoordScanOutcome::EngineUnavailable;
            };
            if let Some((lon, lat)) = trailing_run(&top_text).zip(trailing_run(&bottom_text)) {
                let raw = format!("{top_text} | {bottom_text}");
                parsed = Some((lon, lat, None, raw, top_conf.min(bottom_conf)));
            }
        }
        if parsed.is_none() {
            let Some((text, confidence)) = (self.providers.read_text)(&frame) else {
                return CoordScanOutcome::EngineUnavailable;
            };
            match parse_coordinates(&text) {
                Some((lon, lat, altitude)) => {
                    parsed = Some((lon, lat, altitude, text, confidence));
                }
                None => {
                    return CoordScanOutcome::Unreadable {
                        raw_text: text,
                        confidence,
                    };
                }
            }
        }
        let (lon, lat, altitude, raw_text, confidence) = parsed.expect("parsed set above");
        if let Some(bounds) = bounds {
            if !bounds.contains(lon, lat) {
                return CoordScanOutcome::Implausible { lon, lat, raw_text };
            }
        }
        CoordScanOutcome::Read(CoordRead {
            lon,
            lat,
            altitude,
            raw_text,
            confidence,
        })
    }
}

/// The Enter listener for the calibration flow, mirroring the
/// spacebar-capture listener's lifecycle over the SAME shared OS hook:
/// enabled only while a flow is live (the facade's start/cancel verbs
/// flip it, and completing the flow disables it from inside), starting
/// the shared source on enable and stopping it on disable, so outside a
/// calibration episode Enter presses are not observed at all.
/// Listening is pass-through: the game still receives the keystroke.
pub struct CoordConfirmListener {
    service: Arc<CoordCaptureService>,
    source: Option<Arc<dyn crate::keystroke_source::KeystrokeSource>>,
    enabled: std::sync::atomic::AtomicBool,
    source_running: std::sync::atomic::AtomicBool,
    return_down: std::sync::atomic::AtomicBool,
}

impl CoordConfirmListener {
    /// A `None` source leaves the listener inert. Subscription uses a
    /// weak handle so the source's callback cannot keep the listener
    /// alive past its owners (the spacebar listener's pattern).
    pub fn new(
        service: Arc<CoordCaptureService>,
        source: Option<Arc<dyn crate::keystroke_source::KeystrokeSource>>,
    ) -> Arc<Self> {
        use std::sync::atomic::AtomicBool;
        let listener = Arc::new(Self {
            service,
            source: source.clone(),
            enabled: AtomicBool::new(false),
            source_running: AtomicBool::new(false),
            return_down: AtomicBool::new(false),
        });
        if let Some(source) = source {
            let dispatch = Arc::downgrade(&listener);
            source.subscribe(Arc::new(
                move |event: &crate::keystroke_source::KeystrokeEvent| {
                    if let Some(listener) = dispatch.upgrade() {
                        listener.on_keystroke(event);
                    }
                },
            ));
        }
        listener
    }

    /// Toggle the listener; idempotent. Enabling starts the shared
    /// source, disabling stops this listener's claim on it.
    pub fn set_enabled(&self, enabled: bool) {
        use std::sync::atomic::Ordering;
        if self.enabled.swap(enabled, Ordering::SeqCst) == enabled {
            return;
        }
        if enabled {
            self.start_source();
        } else {
            self.stop_source();
        }
    }

    /// Tear down at shutdown.
    pub fn stop(&self) {
        use std::sync::atomic::Ordering;
        self.enabled.store(false, Ordering::SeqCst);
        self.stop_source();
    }

    fn start_source(&self) {
        use std::sync::atomic::Ordering;
        let Some(source) = &self.source else {
            return;
        };
        if self.source_running.load(Ordering::SeqCst) {
            return;
        }
        let attached = source.start();
        self.source_running.store(attached, Ordering::SeqCst);
        tracing::info!(
            target: "eo::input",
            attached,
            "coordinate-calibration confirm source start requested"
        );
    }

    fn stop_source(&self) {
        use std::sync::atomic::Ordering;
        let Some(source) = &self.source else {
            return;
        };
        if !self.source_running.load(Ordering::SeqCst) {
            return;
        }
        source.stop();
        self.source_running.store(false, Ordering::SeqCst);
        self.return_down.store(false, Ordering::SeqCst);
    }

    fn on_keystroke(self: &Arc<Self>, event: &crate::keystroke_source::KeystrokeEvent) {
        use crate::keystroke_source::KeystrokeKind;
        use std::sync::atomic::Ordering;
        if !self.source_running.load(Ordering::SeqCst) || !self.enabled.load(Ordering::SeqCst) {
            return;
        }
        if event.key != "return" {
            return;
        }
        match event.kind {
            KeystrokeKind::Release => {
                self.return_down.store(false, Ordering::SeqCst);
            }
            KeystrokeKind::Press => {
                // Press edge only: auto-repeat while held must not step
                // the flow through both corners in one hold.
                if self.return_down.swap(true, Ordering::SeqCst) {
                    return;
                }
                if !self.service.calibration_active() {
                    return;
                }
                // The completing press runs a capture + OCR validation
                // read; a short-lived thread keeps the dispatch cheap.
                // The service disarms this listener itself on every
                // Idle-reaching transition.
                let listener = self.clone();
                std::thread::spawn(move || {
                    listener.service.on_confirm();
                });
            }
        }
    }
}

/// A planet's coordinate window, supplied by the caller that knows the
/// selected map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordBounds {
    pub lon_min: i64,
    pub lon_max: i64,
    pub lat_min: i64,
    pub lat_max: i64,
}

impl CoordBounds {
    pub fn contains(&self, lon: i64, lat: i64) -> bool {
        (self.lon_min..=self.lon_max).contains(&lon) && (self.lat_min..=self.lat_max).contains(&lat)
    }
}

/// Split a frame at mid-height into its two stacked readout lines.
fn split_rows(frame: &BgrImage) -> (BgrImage, BgrImage) {
    let top_h = frame.h / 2;
    let stride = frame.w * 3;
    (
        BgrImage {
            data: frame.data[..top_h * stride].to_vec(),
            h: top_h,
            w: frame.w,
        },
        BgrImage {
            data: frame.data[top_h * stride..].to_vec(),
            h: frame.h - top_h,
            w: frame.w,
        },
    )
}

/// One readout line's value: the trailing digit run (the value follows
/// its label, so digits misread inside the label never win).
pub fn trailing_run(text: &str) -> Option<i64> {
    let mut last: Option<i64> = None;
    let mut current: Option<i64> = None;
    for ch in text.chars() {
        if let Some(digit) = digit_value(ch) {
            let digit = i64::from(digit);
            current = match current {
                Some(value) if value <= (i64::MAX - digit) / 10 => Some(value * 10 + digit),
                Some(_) => return None,
                None => Some(digit),
            };
        } else if let Some(done) = current.take() {
            last = Some(done);
        }
    }
    current.or(last)
}

/// Parse a coordinate readout: two or three integer runs in order
/// (longitude, latitude, optional altitude), with everything between
/// runs treated as separator noise. Fullwidth digits fold by value
/// (the recogniser's alphabet carries both forms). More or fewer runs
/// than a readout shows is unreadable, not guessable.
pub fn parse_coordinates(text: &str) -> Option<(i64, i64, Option<i64>)> {
    let mut runs: Vec<i64> = Vec::new();
    let mut current: Option<i64> = None;
    for ch in text.chars() {
        if let Some(digit) = digit_value(ch) {
            let digit = i64::from(digit);
            current = Some(match current {
                Some(value) if value <= (i64::MAX - digit) / 10 => value * 10 + digit,
                Some(_) => return None,
                None => digit,
            });
        } else if let Some(done) = current.take() {
            runs.push(done);
        }
    }
    if let Some(done) = current {
        runs.push(done);
    }
    match runs.as_slice() {
        [lon, lat] => Some((*lon, *lat, None)),
        [lon, lat, alt] => Some((*lon, *lat, Some(*alt))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A 2x2 capture frame whose rows carry distinct bytes, so a mock
    /// reader can tell the split top line (h 1, byte 1), the split
    /// bottom line (h 1, byte 2), and the whole frame (h 2) apart.
    fn frame() -> BgrImage {
        BgrImage {
            data: [vec![1u8; 6], vec![2u8; 6]].concat(),
            h: 2,
            w: 2,
        }
    }

    /// Providers whose reader answers per readout line: `top`/`bottom`
    /// for the split halves, `whole` for the whole-frame fallback.
    fn providers_lines(
        top: &'static str,
        bottom: &'static str,
        whole: &'static str,
    ) -> CoordCaptureProviders {
        CoordCaptureProviders {
            cursor_position: Arc::new(|| Some((10, 20))),
            region: Arc::new(|| {
                Some(CoordRegion {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 20,
                })
            }),
            capture_region: Arc::new(|_, _, _, _| Some(frame())),
            read_text: Arc::new(move |img| {
                let text = match (img.h, img.data[0]) {
                    (1, 1) => top,
                    (1, 2) => bottom,
                    _ => whole,
                };
                Some((text.to_string(), 0.93))
            }),
            persist_region: Arc::new(|_| Ok(())),
        }
    }

    /// Providers where only the whole-frame single-line read carries
    /// text (the stacked halves read empty), exercising the fallback.
    fn providers_reading(text: &'static str) -> CoordCaptureProviders {
        providers_lines("", "", text)
    }

    #[test]
    fn parses_two_and_three_run_readouts() {
        assert_eq!(
            parse_coordinates("61234, 75456"),
            Some((61234, 75456, None))
        );
        assert_eq!(
            parse_coordinates("61234, 75456, 103"),
            Some((61234, 75456, Some(103)))
        );
        // OCR noise between runs is separator, not failure.
        assert_eq!(
            parse_coordinates(" 61234 . 75456 ; 103m"),
            Some((61234, 75456, Some(103)))
        );
        // Fullwidth digits fold by value.
        assert_eq!(
            parse_coordinates("６１２３４, ７５４５６"),
            Some((61234, 75456, None))
        );
    }

    #[test]
    fn a_stacked_readout_reads_per_line() {
        // The game's actual layout: `Lon: <value>` above `Lat: <value>`,
        // labels included in the calibrated rectangle.
        let service = CoordCaptureService::new(providers_lines("Lon: 31915", "Lat: 19999", "junk"));
        assert!(matches!(
            service.scan(None),
            CoordScanOutcome::Read(CoordRead {
                lon: 31915,
                lat: 19999,
                altitude: None,
                ..
            })
        ));
    }

    #[test]
    fn a_label_misread_cannot_inject_a_value() {
        // OCR reading the label's O as a zero: the trailing run wins.
        let service = CoordCaptureService::new(providers_lines("L0n: 31915", "Lat: 19999", ""));
        assert!(matches!(
            service.scan(None),
            CoordScanOutcome::Read(CoordRead {
                lon: 31915,
                lat: 19999,
                ..
            })
        ));
    }

    #[test]
    fn trailing_run_takes_the_last_digit_run() {
        assert_eq!(trailing_run("Lon: 31915"), Some(31915));
        assert_eq!(trailing_run("L0n: 31915"), Some(31915));
        assert_eq!(trailing_run("31915"), Some(31915));
        assert_eq!(trailing_run("no digits"), None);
        assert_eq!(trailing_run(""), None);
    }

    #[test]
    fn refuses_run_counts_that_are_not_a_readout() {
        assert_eq!(parse_coordinates(""), None);
        assert_eq!(parse_coordinates("no digits"), None);
        assert_eq!(parse_coordinates("61234"), None);
        assert_eq!(parse_coordinates("1, 2, 3, 4"), None);
    }

    #[test]
    fn the_two_point_flow_normalises_and_persists() {
        let persisted = Arc::new(Mutex::new(None));
        let sink = persisted.clone();
        let cursor_calls = Arc::new(AtomicUsize::new(0));
        let counter = cursor_calls.clone();
        let mut providers = providers_reading("61234, 75456, 103");
        // Second corner arrives up-left of the first: normalisation duty.
        providers.cursor_position = Arc::new(move || {
            let call = counter.fetch_add(1, Ordering::SeqCst);
            Some(if call == 0 { (200, 100) } else { (50, 40) })
        });
        providers.persist_region = Arc::new(move |region| {
            *sink.lock().unwrap() = Some(region);
            Ok(())
        });
        let service = CoordCaptureService::new(providers);

        assert_eq!(service.calibration_start(), CalibrationPhase::AwaitTopLeft);
        assert!(matches!(
            service.on_confirm(),
            CalibrationPhase::AwaitBottomRight { .. }
        ));
        assert_eq!(service.on_confirm(), CalibrationPhase::Idle);

        assert_eq!(
            *persisted.lock().unwrap(),
            Some(CoordRegion {
                x: 50,
                y: 40,
                w: 150,
                h: 60,
            })
        );
        // The completion took a validation read and stored the echo.
        assert!(matches!(
            service.last_validation(),
            Some(CoordScanOutcome::Read(CoordRead { lon: 61234, .. }))
        ));
    }

    #[test]
    fn a_degenerate_rectangle_rearms_the_second_corner() {
        let mut providers = providers_reading("1, 2");
        providers.cursor_position = Arc::new(|| Some((100, 100)));
        let service = CoordCaptureService::new(providers);
        service.calibration_start();
        service.on_confirm();
        // Same cursor position for the second corner: zero-size rect.
        assert!(matches!(
            service.on_confirm(),
            CalibrationPhase::AwaitBottomRight { .. }
        ));
    }

    #[test]
    fn enter_outside_a_flow_is_ignored() {
        let service = CoordCaptureService::new(providers_reading("1, 2"));
        assert_eq!(service.on_confirm(), CalibrationPhase::Idle);
        assert!(!service.calibration_active());
    }

    #[test]
    fn scan_answers_each_failure_leg_typed() {
        // No region.
        let service = CoordCaptureService::new(CoordCaptureProviders {
            cursor_position: Arc::new(|| Some((0, 0))),
            ..Default::default()
        });
        assert_eq!(service.scan(None), CoordScanOutcome::NoRegion);

        // Capture fails.
        let mut providers = providers_reading("x");
        providers.capture_region = Arc::new(|_, _, _, _| None);
        let service = CoordCaptureService::new(providers);
        assert_eq!(service.scan(None), CoordScanOutcome::CaptureFailed);

        // Engine unavailable.
        let mut providers = providers_reading("x");
        providers.read_text = Arc::new(|_| None);
        let service = CoordCaptureService::new(providers);
        assert_eq!(service.scan(None), CoordScanOutcome::EngineUnavailable);

        // Unreadable text.
        let service = CoordCaptureService::new(providers_reading("loading..."));
        assert!(matches!(
            service.scan(None),
            CoordScanOutcome::Unreadable { .. }
        ));

        // Implausible against bounds.
        let service = CoordCaptureService::new(providers_reading("999999, 75456"));
        let bounds = CoordBounds {
            lon_min: 16384,
            lon_max: 90112,
            lat_min: 24576,
            lat_max: 98304,
        };
        assert!(matches!(
            service.scan(Some(bounds)),
            CoordScanOutcome::Implausible { lon: 999999, .. }
        ));

        // A clean read inside bounds.
        let service = CoordCaptureService::new(providers_reading("61234, 75456, 103"));
        assert!(matches!(
            service.scan(Some(bounds)),
            CoordScanOutcome::Read(CoordRead {
                lon: 61234,
                lat: 75456,
                altitude: Some(103),
                ..
            })
        ));
    }
}
