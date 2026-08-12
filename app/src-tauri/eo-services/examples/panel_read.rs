//! Run the bundled recogniser over a calibrated panel and print what it
//! reads, field by field. Answers the only question calibration leaves
//! open: whether the rectangles contain text the engine can actually
//! resolve, as opposed to text a person can.
//!
//! Reads a saved panel image when given one, so a capture can be
//! re-examined offline as the parsing changes, and captures live
//! otherwise.
//!
//! ```sh
//! cargo run -p eo-services --features linux-capture --example panel_read -- --panel shots/panel.png
//! cargo run -p eo-services --features linux-capture --example panel_read
//! ```

use std::path::{Path, PathBuf};

use eo_services::ocr_engine::{load_bgr_png, OcrEngine};
use eo_services::scan_presets::ScanPresets;
use eo_services::skill_panel::BgrImage;
use eo_services::{eu_window, screen_capture};

fn resource(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../entropia-orme/resources")
        .join(relative)
}

/// The panel pixels: a saved capture, or a fresh one from the calibrated
/// region.
fn panel_image(saved: Option<&Path>, presets: &ScanPresets) -> Result<BgrImage, String> {
    if let Some(path) = saved {
        let bytes = std::fs::read(path).map_err(|error| format!("{path:?}: {error}"))?;
        let (data, h, w) = load_bgr_png(&bytes).map_err(|error| format!("{path:?}: {error}"))?;
        return Ok(BgrImage { data, h, w });
    }
    let (top_left, bottom_right) = eu_window::sale_window_region(presets)
        .ok_or("no sale-window region: the game must be running and the panel calibrated")?;
    let (x, y) = (top_left[0], top_left[1]);
    let (w, h) = (bottom_right[0] - x, bottom_right[1] - y);
    screen_capture::capture_region_bgr(x, y, w, h).ok_or("the screen capture returned nothing".into())
}

fn main() {
    let mut saved: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--panel" => saved = args.next().map(PathBuf::from),
            other => {
                eprintln!("read: unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let presets = ScanPresets::new(&resource("panel_geometry.json"));
    let anchor = &presets.sale_window;
    if anchor.cells.is_empty() {
        eprintln!("read: the sale window has no calibrated fields");
        std::process::exit(1);
    }

    let panel = panel_image(saved.as_deref(), &presets).unwrap_or_else(|error| {
        eprintln!("read: {error}");
        std::process::exit(1);
    });
    eprintln!("panel: {}x{}", panel.w, panel.h);

    let started = std::time::Instant::now();
    // The production provider ladder, so what this prints is what the
    // app would read rather than what a different session would.
    let engine = OcrEngine::new_with_providers(
        &resource("models/svtrv2_rec.onnx"),
        &resource("models/ppocr_keys_v1.txt"),
    )
    .unwrap_or_else(|error| {
        eprintln!("read: the recogniser did not load: {error}");
        std::process::exit(1);
    });
    eprintln!("engine loaded in {:?}", started.elapsed());

    for (name, cell) in &anchor.cells {
        let (x, y, w, h) = cell.single_rect();
        let crop = panel.crop(y, y + h, x, x + w);
        let at = std::time::Instant::now();
        match engine.recognize_bgr(&crop.data, crop.h, crop.w) {
            Ok((text, confidence)) => {
                eprintln!(
                    "  {name:14} {confidence:>5.3}  {text:?}   [{:>4}x{:<3} {:?}]",
                    crop.w,
                    crop.h,
                    at.elapsed()
                )
            }
            Err(error) => eprintln!("  {name:14}    -    <{error}>"),
        }
    }
}
