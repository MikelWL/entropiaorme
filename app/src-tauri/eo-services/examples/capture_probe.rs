//! Stand-alone probe for the Linux screen-capture engine: acquires the
//! portal stream (reusing the restore token from `EO_CAPTURE_TOKEN_PATH`
//! when present), waits for the first frame, grabs a small rectangle,
//! and reports each stage. Run with:
//!
//! ```sh
//! EO_CAPTURE_TOKEN_PATH=<data-dir>/capture-restore-token \
//!   cargo run -p eo-services --features linux-capture --example capture_probe
//! ```

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[cfg(all(target_os = "linux", feature = "linux-capture"))]
    {
        eprintln!("probe: requesting a 200x50 rectangle at (2100, 100), on the shared monitor");
        match eo_services::screen_capture::capture_region_png(2100, 100, 200, 50) {
            Some(png) => {
                let out = std::env::temp_dir().join("eo-capture-probe.png");
                std::fs::write(&out, &png).expect("probe png writes");
                eprintln!(
                    "probe: capture OK ({} bytes) -> {}",
                    png.len(),
                    out.display()
                );
            }
            None => {
                eprintln!("probe: capture returned None (see log lines above)");
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "linux-capture")))]
    eprintln!("probe: linux-capture feature not enabled on this build");
}
