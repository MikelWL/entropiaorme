//! Fixture-driven read-path latency harness: opens a COPY of a real
//! application database and times the analytical read endpoints through the
//! real in-process dispatch path, reporting median / p95 / min / max in
//! milliseconds per endpoint.
//!
//! Unlike [`router_microbench`] (which replays a tiny scripted scenario), this
//! harness measures against a large real database, so it is the tool for the
//! DB-scalability read-path work: the same endpoint set is timed before and
//! after an index or read-model change to get a same-host before/after.
//!
//! It is `#[ignore]`d AND gated on an environment variable, so it compiles
//! under the normal suite (it cannot rot) but only runs when explicitly asked
//! and pointed at a database. It carries no database path and prints only
//! aggregate timings, never any row content, so nothing gameplay-specific
//! reaches the tree. Point it at a copy you are free to open (never the live
//! file); this harness copies the file it is given into a temp dir before
//! opening it, so the source is read once and never written.
//!
//! Run it with:
//!
//! ```text
//! EO_PERF_FIXTURE=/path/to/entropia_orme.db \
//!   cargo test -p eo-http --release --test perf_fixture_bench -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use eo_http::hydration::HydrationState;
use eo_http::{dispatch_in_process, AppState};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;
use serde_json::Value;

const WARMUPS: usize = 3;
const SAMPLES: usize = 15;

/// `statistics.median` over a sorted slice (average the two middle elements on
/// an even count).
fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

/// p95 index: `round(0.95 * (n - 1))`, clamped into range.
fn p95(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    let index = (0.95 * (n - 1) as f64).round() as usize;
    sorted[index.min(n - 1)]
}

#[test]
#[ignore = "measurement harness; set EO_PERF_FIXTURE and run with --release --ignored --nocapture"]
fn read_path_latency_against_a_real_database() {
    let Some(fixture) = std::env::var_os("EO_PERF_FIXTURE").map(PathBuf::from) else {
        eprintln!(
            "EO_PERF_FIXTURE unset: skipping the real-database read-path harness. \
             Point it at a copy of an application database to run it."
        );
        return;
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let dir = tempfile::tempdir().expect("temp dir");

    // Copy the source database into the temp dir before opening it: the source
    // is read once and never written, and the WAL/optimize the open triggers
    // land on the throwaway copy. Only the main database file is copied (a
    // quiesced database has no live WAL frames to carry).
    let working = dir.path().join("entropia_orme.db");
    std::fs::copy(&fixture, &working).expect("copy fixture into temp dir");
    let bytes_before = std::fs::metadata(&working).map(|m| m.len()).unwrap_or(0);

    let db = runtime
        .block_on(Db::open(&working))
        .expect("open+migrate the copied database");

    // Whether ANALYZE has ever produced planner statistics on this database.
    let has_stat1: bool = runtime.block_on(async {
        use sqlx::Row as _;
        sqlx::query("SELECT 1 FROM sqlite_master WHERE type='table' AND name='sqlite_stat1'")
            .fetch_optional(db.pool())
            .await
            .expect("stat1 probe")
            .map(|row| row.get::<i64, _>(0) == 1)
            .unwrap_or(false)
    });

    let game_data =
        Arc::new(GameDataStore::new(&dir.path().join("empty")).expect("empty game-data store"));
    let hydration = Arc::new(HydrationState::new(
        db,
        game_data,
        Arc::new(RealClock::new()),
        dir.path().to_path_buf(),
    ));
    // Only the read surface is exercised, so hydration alone composes the
    // state; the producer services (tracker, scan, hotbar) are unused here.
    let state = Arc::new(AppState::new(0).with_hydration(hydration));

    // A real session id for the detail endpoint, taken from the list response.
    let session_id = runtime.block_on(async {
        let response =
            dispatch_in_process(state.clone(), "GET", "/api/tracking/sessions", &[], vec![])
                .await
                .expect("sessions dispatch");
        assert_eq!(response.status, 200, "sessions list");
        let sessions: Value = serde_json::from_slice(&response.body).expect("sessions json");
        sessions
            .as_array()
            .and_then(|list| list.first())
            .and_then(|session| session["id"].as_str())
            .map(str::to_string)
    });

    let detail = session_id
        .as_ref()
        .map(|id| format!("/api/tracking/session/{id}"));
    let mut endpoints: Vec<(&str, String)> = vec![
        ("overview_all", "/api/analytics/overview?period=all".to_string()),
        ("activity", "/api/analytics/activity".to_string()),
        ("session_list", "/api/tracking/sessions".to_string()),
    ];
    if let Some(detail) = &detail {
        endpoints.push(("session_detail", detail.clone()));
    }

    println!("\n=== read-path latency over a real database ===");
    println!("copied file size: {} bytes", bytes_before);
    println!("sqlite_stat1 present (ANALYZE has run): {has_stat1}");
    println!(
        "warmups: {WARMUPS}, samples: {SAMPLES}\n{:<16} {:>10} {:>10} {:>10} {:>10}",
        "endpoint", "median_ms", "p95_ms", "min_ms", "max_ms"
    );

    runtime.block_on(async {
        for (id, path) in &endpoints {
            for _ in 0..WARMUPS {
                let response = dispatch_in_process(state.clone(), "GET", path, &[], vec![])
                    .await
                    .unwrap_or_else(|err| panic!("{id} warm-up: {err}"));
                assert_eq!(response.status, 200, "{id} warm-up status");
            }
            let mut timings = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let started = Instant::now();
                let response = dispatch_in_process(state.clone(), "GET", path, &[], vec![])
                    .await
                    .unwrap_or_else(|err| panic!("{id}: {err}"));
                assert_eq!(response.status, 200, "{id} status");
                timings.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            timings.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
            println!(
                "{:<16} {:>10.1} {:>10.1} {:>10.1} {:>10.1}",
                id,
                median(&timings),
                p95(&timings),
                timings[0],
                timings[timings.len() - 1],
            );
        }
    });
    println!();
}
