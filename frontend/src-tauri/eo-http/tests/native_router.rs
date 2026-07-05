//! Hermetic coverage for the composed native state.
//!
//! The live tracking route families have migrated onto typed IPC commands
//! (`eo_api`), and the guide-mode demo read namespace is covered by
//! `crate::demo`'s own golden tests. What remains here is the composed-state
//! lifecycle check: the shutdown `PRAGMA optimize` over a temp-database
//! hydration state, and its no-op on a bare substrate.

use std::sync::Arc;

use eo_http::cors::CorsConfig;
use eo_http::hydration::HydrationState;
use eo_http::AppState;
use eo_services::db::Db;

async fn serve_substrate() -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Db::open(&dir.path().join("entropia_orme.db"))
        .await
        .expect("temp db opens");
    let hydration = Arc::new(HydrationState::new(db));
    let state = Arc::new(
        AppState::new(0)
            .with_hydration(hydration)
            .with_cors(CorsConfig::new(5173, None)),
    );
    (state, dir)
}

#[tokio::test]
async fn optimize_on_shutdown_runs_over_a_composed_state_and_no_ops_without_one() {
    // A composed hydration state has a pool to optimise.
    let (state, _dir) = serve_substrate().await;
    assert!(
        state.optimize_on_shutdown().await,
        "PRAGMA optimize runs against the composed hydration pool"
    );
    // A bare substrate with no hydration has nothing to optimise.
    let bare = Arc::new(AppState::new(0));
    assert!(
        !bare.optimize_on_shutdown().await,
        "no hydration state means nothing to optimise"
    );
}
