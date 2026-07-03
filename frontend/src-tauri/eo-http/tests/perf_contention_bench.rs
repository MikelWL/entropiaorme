//! Read-under-write contention harness: times the analytical read endpoints
//! while a continuous writer holds the database's write path, reporting read
//! latency **idle** versus **under load** on the same run, plus the writer's
//! achieved throughput.
//!
//! This is the instrument for the connection-topology work (the `Db` seam's
//! writer/reader split). Unlike [`perf_fixture_bench`], which times reads on a
//! quiescent database, this one measures the axis a topology change actually
//! moves: whether a live write stream stalls dashboard reads. On the
//! pool-of-one baseline a read and a write cannot be in flight together (one
//! pooled connection), so reads queue behind the writer; a dedicated writer
//! plus a reader pool lets WAL readers proceed concurrently. The idle-versus-
//! loaded delta on the same host is the before/after signal.
//!
//! It is a **stress** probe, not a field-cadence replay: the writer runs a
//! tight loop of small write transactions (far above real combat cadence) to
//! expose the worst-case contention headroom. Each transaction inserts and
//! deletes one synthetic `tracking_sessions` row, so it acquires the write
//! path and appends WAL frames without growing the database.
//!
//! Like [`perf_fixture_bench`] it is `#[ignore]`d AND gated on an environment
//! variable, so it compiles under the normal suite (it cannot rot) but only
//! runs when explicitly asked and pointed at a database. It copies the file it
//! is given into a temp dir before opening it (the source is read once, never
//! written), and prints only aggregate timings, never any row content.
//!
//! Run it with:
//!
//! ```text
//! EO_PERF_FIXTURE=/path/to/entropia_orme.db \
//!   cargo test -p eo-http --release --test perf_contention_bench -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use eo_http::hydration::HydrationState;
use eo_http::{dispatch_in_process, AppState};
use eo_services::clock::RealClock;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

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

/// Time `SAMPLES` sequential dispatches of one endpoint (after `WARMUPS`
/// untimed), returning (median_ms, p95_ms, min_ms, max_ms).
async fn time_endpoint(state: &Arc<AppState>, path: &str) -> (f64, f64, f64, f64) {
    for _ in 0..WARMUPS {
        let response = dispatch_in_process(state.clone(), "GET", path, &[], vec![])
            .await
            .unwrap_or_else(|err| panic!("{path} warm-up: {err}"));
        assert_eq!(response.status, 200, "{path} warm-up status");
    }
    let mut timings = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        let response = dispatch_in_process(state.clone(), "GET", path, &[], vec![])
            .await
            .unwrap_or_else(|err| panic!("{path}: {err}"));
        assert_eq!(response.status, 200, "{path} status");
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    timings.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
    (
        median(&timings),
        p95(&timings),
        timings[0],
        timings[timings.len() - 1],
    )
}

#[test]
#[ignore = "measurement harness; set EO_PERF_FIXTURE and run with --release --ignored --nocapture"]
fn read_latency_idle_versus_under_write_load() {
    let Some(fixture) = std::env::var_os("EO_PERF_FIXTURE").map(PathBuf::from) else {
        eprintln!(
            "EO_PERF_FIXTURE unset: skipping the read-under-write contention harness. \
             Point it at a copy of an application database to run it."
        );
        return;
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("runtime");
    let dir = tempfile::tempdir().expect("temp dir");

    // Copy the source database into the temp dir before opening it: the writer
    // loop and the WAL land on the throwaway copy, never the source.
    let working = dir.path().join("entropia_orme.db");
    std::fs::copy(&fixture, &working).expect("copy fixture into temp dir");
    let bytes_before = std::fs::metadata(&working).map(|m| m.len()).unwrap_or(0);

    let db = runtime
        .block_on(Db::open(&working))
        .expect("open+migrate the copied database");
    // The writer drives the pool directly; grab a handle before the read
    // surface takes ownership of the Db.
    let writer_pool = db.write().clone();

    let game_data =
        Arc::new(GameDataStore::new(&dir.path().join("empty")).expect("empty game-data store"));
    let hydration = Arc::new(HydrationState::new(
        db,
        game_data,
        Arc::new(RealClock::new()),
        dir.path().to_path_buf(),
    ));
    let state = Arc::new(AppState::new(0).with_hydration(hydration));

    // Reads timed under load. Keep the set small and representative: a heavy
    // aggregate (overview_all) and a cheap one (activity), so contention shows
    // across the cost range.
    let endpoints = [
        ("overview_all", "/api/analytics/overview?period=all"),
        ("activity", "/api/analytics/activity"),
        ("session_list", "/api/tracking/sessions"),
    ];

    // Pay the one-time rollup backfill up front so neither the idle nor the
    // loaded medians carry it.
    runtime.block_on(async {
        let response = dispatch_in_process(
            state.clone(),
            "GET",
            "/api/analytics/overview?period=all",
            &[],
            vec![],
        )
        .await
        .expect("first overview dispatch");
        assert_eq!(response.status, 200, "first overview status");
    });

    // Phase 1: idle baseline (no writer running).
    let idle: Vec<(f64, f64, f64, f64)> =
        runtime.block_on(async { collect(&state, &endpoints).await });

    // Phase 2: the same reads while a continuous writer holds the write path.
    let stop = Arc::new(AtomicBool::new(false));
    let writes = Arc::new(AtomicU64::new(0));
    let writer = {
        let stop = stop.clone();
        let writes = writes.clone();
        let pool = writer_pool.clone();
        runtime.spawn(async move {
            let mut seq: u64 = 0;
            while !stop.load(Ordering::Relaxed) {
                seq += 1;
                let id = format!("contention-probe-{seq}");
                // One write transaction: acquire the write path, append WAL
                // frames, net zero rows. A failure here should surface loudly
                // (the probe is worthless if the writer silently stalls).
                let mut tx = pool.begin().await.expect("writer begin");
                sqlx::query(
                    "INSERT INTO tracking_sessions (id, started_at, is_active) VALUES (?, ?, 0)",
                )
                .bind(&id)
                .bind(seq as f64)
                .execute(&mut *tx)
                .await
                .expect("writer insert");
                sqlx::query("DELETE FROM tracking_sessions WHERE id = ?")
                    .bind(&id)
                    .execute(&mut *tx)
                    .await
                    .expect("writer delete");
                tx.commit().await.expect("writer commit");
                writes.fetch_add(1, Ordering::Relaxed);
            }
        })
    };

    let loaded_started = Instant::now();
    let loaded: Vec<(f64, f64, f64, f64)> =
        runtime.block_on(async { collect(&state, &endpoints).await });
    let loaded_secs = loaded_started.elapsed().as_secs_f64();

    stop.store(true, Ordering::Relaxed);
    let total_writes = writes.load(Ordering::Relaxed);
    runtime.block_on(async { writer.await.expect("writer task join") });
    let write_rate = if loaded_secs > 0.0 {
        total_writes as f64 / loaded_secs
    } else {
        0.0
    };

    println!("\n=== read latency: idle vs under continuous write load ===");
    println!("copied file size: {bytes_before} bytes");
    println!(
        "writer: {total_writes} write transactions during the loaded phase \
         (~{write_rate:.0}/s, tight-loop stress cadence)"
    );
    println!("warmups: {WARMUPS}, samples: {SAMPLES}");
    println!(
        "{:<16} {:>12} {:>12} {:>12} {:>12} {:>10}",
        "endpoint", "idle_med_ms", "load_med_ms", "idle_p95_ms", "load_p95_ms", "med_x"
    );
    for (index, (id, _)) in endpoints.iter().enumerate() {
        let (idle_med, idle_p95, _, _) = idle[index];
        let (load_med, load_p95, _, _) = loaded[index];
        let ratio = if idle_med > 0.0 {
            load_med / idle_med
        } else {
            0.0
        };
        println!(
            "{id:<16} {idle_med:>12.1} {load_med:>12.1} {idle_p95:>12.1} {load_p95:>12.1} {ratio:>9.1}x"
        );
    }
    println!();
}

/// Time every endpoint once, in order, returning their (median, p95, min, max).
async fn collect(state: &Arc<AppState>, endpoints: &[(&str, &str)]) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::with_capacity(endpoints.len());
    for (_, path) in endpoints {
        out.push(time_endpoint(state, path).await);
    }
    out
}
