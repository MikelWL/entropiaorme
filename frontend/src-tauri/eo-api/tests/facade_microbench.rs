//! Typed-facade micro-benchmark: the dispatch latency of the migrated
//! equipment reads, the matched after-leg of the HTTP-path measurement
//! the router micro-benchmark captured before the family moved (same
//! host, same empty-library state, same warm-up and sample discipline,
//! reported as median (p50) / p95 / min / max in milliseconds).
//!
//! What the pair isolates is the transport: the HTTP leg carried a
//! per-dispatch router build, the guard/CORS/observe stack, and an HTTP
//! envelope; this leg is the facade call the typed command wraps. The
//! handler work (an empty-table read, an empty catalogue search) is
//! identical either side.
//!
//! `#[ignore]`d on purpose: a measurement harness, not a correctness
//! gate. Run it with:
//!
//! ```text
//! cargo test -p eo-api --release --test facade_microbench -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::Instant;

use eo_api::equipment::SearchKind;
use eo_api::Api;
use eo_services::db::Db;
use eo_services::game_data_store::GameDataStore;

mod common;

const WARMUPS: usize = 3;
const SAMPLES: usize = 30;

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

fn p95(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    let index = (0.95 * (n - 1) as f64).round() as usize;
    sorted[index.min(n - 1)]
}

#[test]
#[ignore = "measurement harness, not a correctness gate; run with --release --ignored --nocapture"]
fn facade_microbench() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let dir = tempfile::tempdir().expect("temp dir");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data dir");

    let db = runtime
        .block_on(Db::open(&data_dir.join("entropia_orme.db")))
        .expect("migrated database");
    let game_data =
        Arc::new(GameDataStore::new(&dir.path().join("empty")).expect("empty game-data store"));
    let clock = Arc::new(eo_services::clock::RealClock::new());
    let (config_service, tracker, hotbar, watcher) =
        common::producer_handles(&db, &data_dir, runtime.handle().clone());
    let api = Api::new(
        db,
        game_data,
        clock,
        data_dir,
        config_service,
        tracker,
        hotbar,
        watcher,
    );

    let mut rows: Vec<(&str, f64, f64, f64, f64)> = Vec::new();
    runtime.block_on(async {
        for _ in 0..WARMUPS {
            api.equipment_library().await.expect("library warm-up");
        }
        let mut timings = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            api.equipment_library().await.expect("library read");
            timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        timings.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
        rows.push((
            "equipment_library",
            median(&timings),
            p95(&timings),
            timings[0],
            timings[timings.len() - 1],
        ));

        for _ in 0..WARMUPS {
            api.equipment_search("herb", SearchKind::Weapon)
                .await
                .expect("search warm-up");
        }
        let mut timings = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let started = Instant::now();
            api.equipment_search("herb", SearchKind::Weapon)
                .await
                .expect("search");
            timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        timings.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
        rows.push((
            "equipment_search",
            median(&timings),
            p95(&timings),
            timings[0],
            timings[timings.len() - 1],
        ));

        // The character family's no-parameter reads, the after-leg of the
        // HTTP dispatch measurement the router micro-benchmark captured
        // before the family moved (same empty game-data + fresh-DB state).
        macro_rules! bench {
            ($label:literal, $call:expr) => {{
                for _ in 0..WARMUPS {
                    $call.await.expect($label);
                }
                let mut timings = Vec::with_capacity(SAMPLES);
                for _ in 0..SAMPLES {
                    let started = Instant::now();
                    $call.await.expect($label);
                    timings.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                timings.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
                rows.push((
                    $label,
                    median(&timings),
                    p95(&timings),
                    timings[0],
                    timings[timings.len() - 1],
                ));
            }};
        }
        bench!("character_calibration", api.character_calibration());
        bench!("character_stats", api.character_stats());
        bench!("character_skills", api.character_skills());
        bench!("character_professions", api.character_professions());
        bench!(
            "character_prospect_options",
            api.character_prospect_options()
        );
        bench!("character_hp_optimizer", api.character_hp_optimizer());
    });

    println!("\ntyped-facade micro-bench (AFTER: equipment + character over typed commands)");
    println!(
        "{SAMPLES} samples per operation after {WARMUPS} warm-ups, empty library and catalogue \
         (matching the HTTP leg's state); facade call only, no dispatch stack.\n"
    );
    println!("| Operation | p50 ms | p95 ms | min ms | max ms |");
    println!("| --- | --- | --- | --- | --- |");
    for (id, p50, p95v, min, max) in &rows {
        println!("| `{id}` | {p50:.4} | {p95v:.4} | {min:.4} | {max:.4} |");
    }
    println!();
}
