//! Stand-alone probe for the Linux screen-capture engine: acquires the
//! portal stream (reusing the restore token from `EO_CAPTURE_TOKEN_PATH`
//! when present), then captures the same rectangle three times across
//! twelve seconds. Each capture must observe a newer frame generation,
//! so a cached first frame cannot masquerade as a live stream.
//!
//! ```sh
//! EO_CAPTURE_TOKEN_PATH=<data-dir>/capture-restore-token \
//!   cargo run -p eo-services --features linux-capture --example capture_probe -- \
//!   <x> <y> <width> <height>
//! ```

#[cfg(all(target_os = "linux", feature = "linux-capture"))]
fn capture_region_from_args() -> Result<(i64, i64, i64, i64), String> {
    let values = std::env::args().skip(1).collect::<Vec<_>>();
    if values.len() != 4 {
        return Err("expected capture rectangle: <x> <y> <width> <height>".to_string());
    }
    let mut parsed = Vec::with_capacity(4);
    for value in values {
        parsed.push(
            value
                .parse::<i64>()
                .map_err(|_| format!("capture rectangle value is not an integer: {value}"))?,
        );
    }
    if parsed[2] <= 0 || parsed[3] <= 0 {
        return Err("capture width and height must be positive".to_string());
    }
    Ok((parsed[0], parsed[1], parsed[2], parsed[3]))
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[cfg(all(target_os = "linux", feature = "linux-capture"))]
    {
        let (x, y, width, height) = capture_region_from_args().unwrap_or_else(|error| {
            eprintln!("probe: {error}");
            eprintln!("usage: capture_probe <x> <y> <width> <height>");
            std::process::exit(2);
        });
        let mut failed = false;
        let mut previous_generation = None;
        for (label, delay_ms) in [("t+0s", 0u64), ("t+3s", 3000), ("t+12s", 9000)] {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            match eo_services::screen_capture::capture_region_png(x, y, width, height) {
                Some(png) => {
                    let generation = eo_services::screen_capture::capture_frame_generation();
                    let out = std::env::temp_dir().join(format!("eo-capture-probe-{label}.png"));
                    let _ = std::fs::write(&out, &png);
                    eprintln!(
                        "probe {label}: OK ({} bytes, frame {generation:?}) -> {}",
                        png.len(),
                        out.display()
                    );
                    if generation.is_none() || generation <= previous_generation {
                        eprintln!(
                            "probe {label}: stream did not advance beyond frame {previous_generation:?}"
                        );
                        failed = true;
                    }
                    previous_generation = generation;
                }
                None => {
                    eprintln!("probe {label}: capture returned None");
                    failed = true;
                }
            }
        }
        if failed {
            std::process::exit(1);
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "linux-capture")))]
    eprintln!("probe: linux-capture feature not enabled on this build");
}
