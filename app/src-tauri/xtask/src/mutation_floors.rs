//! Reads one or more cargo-mutants `outcomes.json` files (the flag repeats,
//! once per campaign shard; per-file counts are summed across them) and
//! enforces the per-file mutation score floors below. Scoring: a mutant
//! counts as caught when a test failed on it OR the mutated build timed out;
//! missed mutants count against the score; unviable mutants (the mutation
//! does not compile) leave the denominator entirely.
//!
//! The floor map is the explicit register of surfaces under mutation
//! coverage. A file WITH an adopted floor must hold its score, and floors
//! only ever ratchet up. A file WITHOUT a floor is unadopted: it is still
//! measured and counted in the aggregate badge, and reported as awaiting
//! coverage, but it is NON-BLOCKING (its surviving mutants do not fail the
//! gate). Bringing a file into the map (killing its survivors, excluding the
//! provable equivalents, recording its floor) is the job of a periodic,
//! user-invoked upkeep pass, not of the feature PR that first added the file.
//! This keeps day-to-day feature work off the slow mutation campaign while
//! the aggregate score stays honest and the backlog stays visible.

use std::collections::BTreeMap;
use std::path::Path;

/// file (workspace-relative, as cargo-mutants reports it) -> floor %.
///
/// Per-file mutation-score floors: the minimum caught-mutant percentage each
/// listed source file must hold, with each floor set to just below its
/// measured score so a regression trips the gate while justified residual
/// survivors do not.
const FLOORS: &[(&str, f64)] = &[
    // The original campaign's adoptions (the tracker.rs and quests.rs entries
    // moved to their split module files when those services became module
    // directories).
    ("eo-services/src/cost_engine.rs", 92.0),
    ("eo-services/src/tt_value_curve.rs", 92.0),
    ("eo-services/src/character_calc.rs", 92.0),
    ("eo-services/src/chatlog_parser.rs", 92.0),
    ("eo-services/src/chatlog_watcher.rs", 92.0),
    ("eo-services/src/fuzzy_match.rs", 92.0),
    ("eo-services/src/ocr_engine.rs", 82.0),
    ("eo-services/src/skill_panel.rs", 92.0),
    ("eo-services/src/codex.rs", 92.0),
    ("eo-services/src/difflib.rs", 92.0),
    ("eo-wire/src/normalizer.rs", 81.0),
    ("eo-wire/src/http_fingerprint.rs", 97.0),
    // 2026-07 coverage-recovery adoptions. Each floor's gap to 100 encodes
    // that file's reviewed residual survivors: provable equivalences (guard
    // clauses whose mutants compute identical results, threshold comparisons at
    // unreachable boundaries) or environment-bounded mutants a hermetic CI test
    // cannot reach (evdev device handles, database-corruption and interrupt
    // races, host-timezone DST gaps, shutdown thread-join ordering). Where an
    // equivalence is cleanly expressible as a mutant-name pattern it is instead
    // filtered out of the campaign in .cargo/mutants.toml (so it never enters
    // the denominator); the gap then covers only the residuals that resist a
    // stable pattern. analytics.rs carries three such campaign-level
    // exclusions, leaving its gap as headroom rather than a one-miss margin;
    // tracking_reads.rs keeps two gap-encoded residuals (the unobservable
    // sub-second nanos in list_ts_to_iso, discarded by the whole-second
    // format).
    ("eo-services/src/analytics.rs", 98.8),
    ("eo-services/src/session_summary.rs", 96.0),
    ("eo-services/src/tracking_reads.rs", 98.5),
    ("eo-services/src/equipment_pricing.rs", 96.0),
    ("eo-services/src/db/mod.rs", 95.7),
    ("eo-services/src/db/pool.rs", 65.0),
    ("eo-services/src/daily_rollup.rs", 97.0),
    ("eo-services/src/keystroke_source.rs", 79.0),
    ("eo-services/src/time.rs", 95.0),
    ("eo-services/src/spacebar_capture_listener.rs", 91.0),
    ("eo-services/src/quests/actor.rs", 84.0),
    ("eo-services/src/quests/analytics.rs", 93.0),
    ("eo-services/src/tracker/providers.rs", 85.0),
    ("eo-services/src/tracker/combat.rs", 96.0),
    ("eo-services/src/tracker/mob.rs", 92.0),
    ("eo-services/src/tracker/weapons.rs", 97.0),
];

/// Map a score to a shields.io colour band (identical floors to
/// coverage-badge.sh, so the product badges read consistently).
fn colour_band(score: f64) -> &'static str {
    if score >= 90.0 {
        "brightgreen"
    } else if score >= 80.0 {
        "green"
    } else if score >= 70.0 {
        "yellowgreen"
    } else if score >= 60.0 {
        "yellow"
    } else if score >= 50.0 {
        "orange"
    } else {
        "red"
    }
}

#[derive(Default, Clone, Copy)]
struct Counts {
    caught: u32,
    missed: u32,
    timeout: u32,
    unviable: u32,
}

/// Per-file caught/missed/timeout/unviable counts from outcomes.json.
///
/// Returns the counts keyed by file. Errors (Err) on an unreadable file, invalid
/// JSON, a missing `outcomes` array, or an unrecognised outcome summary, so a
/// malformed campaign output fails closed rather than scoring a partial run.
fn score_outcomes(text: &str) -> Result<BTreeMap<String, Counts>, String> {
    let data: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("cannot parse outcomes.json: {e}"))?;
    let outcomes = data
        .get("outcomes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "outcomes.json has no 'outcomes' array".to_string())?;

    let mut per_file: BTreeMap<String, Counts> = BTreeMap::new();
    for outcome in outcomes {
        let scenario = &outcome["scenario"];
        // The baseline (unmutated) build is reported as the string "Baseline".
        if scenario.as_str() == Some("Baseline") {
            continue;
        }
        let file = scenario
            .get("Mutant")
            .and_then(|m| m.get("file"))
            .and_then(|f| f.as_str())
            .ok_or_else(|| "an outcome has no scenario.Mutant.file".to_string())?;
        let summary = outcome
            .get("summary")
            .and_then(|s| s.as_str())
            .ok_or_else(|| "an outcome has no summary".to_string())?;
        let counts = per_file.entry(file.to_string()).or_default();
        match summary {
            "CaughtMutant" => counts.caught += 1,
            "MissedMutant" => counts.missed += 1,
            "Timeout" => counts.timeout += 1,
            "Unviable" => counts.unviable += 1,
            other => return Err(format!("unrecognised outcome summary: {other:?}")),
        }
    }
    Ok(per_file)
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let mut outcomes_paths = crate::flag_values(args, "--outcomes")?;
    if outcomes_paths.is_empty() {
        outcomes_paths.push("mutants.out/outcomes.json".to_string());
    }
    let mut per_file: BTreeMap<String, Counts> = BTreeMap::new();
    for outcomes_path in &outcomes_paths {
        let text = std::fs::read_to_string(Path::new(outcomes_path))
            .map_err(|e| format!("cannot read {outcomes_path}: {e}"))?;
        for (file, counts) in score_outcomes(&text)? {
            let merged = per_file.entry(file).or_default();
            merged.caught += counts.caught;
            merged.missed += counts.missed;
            merged.timeout += counts.timeout;
            merged.unviable += counts.unviable;
        }
    }

    if per_file.is_empty() {
        println!("no mutants in the campaign output; nothing to score");
        return Ok(1);
    }

    let floors: BTreeMap<&str, f64> = FLOORS.iter().copied().collect();
    let mut failures: Vec<String> = Vec::new();
    let mut awaiting: Vec<String> = Vec::new();
    let mut total_caught: u32 = 0;
    let mut total_considered: u32 = 0;

    println!(
        "{:45} {:>6} {:>6} {:>7} {:>9}",
        "file", "caught", "missed", "score", "floor"
    );
    for (file, counts) in &per_file {
        let caught = counts.caught + counts.timeout;
        let denominator = caught + counts.missed;
        total_caught += caught;
        total_considered += denominator;
        let score = if denominator > 0 {
            100.0 * caught as f64 / denominator as f64
        } else {
            100.0
        };
        let floor = floors.get(file.as_str()).copied();
        let bar = match floor {
            Some(f) => format!("{f:.1}"),
            None => "unadopted".to_string(),
        };
        println!(
            "{file:45} {caught:6} {missed:6} {score:7.1} {bar:>9}",
            missed = counts.missed
        );
        match floor {
            Some(f) => {
                if score < f {
                    failures.push(format!("{file}: score {score:.1} below floor {f:.1}"));
                }
            }
            None => {
                // Unadopted files are non-blocking: their survivors are a
                // coverage backlog for the next upkeep pass, never a gate
                // failure. Report them so the backlog stays visible.
                if counts.missed > 0 {
                    awaiting.push(format!(
                        "{file}: {} surviving mutant(s), score {score:.1}",
                        counts.missed
                    ));
                }
            }
        }
    }

    // A floor whose file produced no scored mutants is a silently vacuous gate
    // (a rename or deletion would otherwise pass unnoticed). Walked in sorted
    // file order.
    for (file, floor) in &floors {
        if !per_file.contains_key(*file) {
            failures.push(format!(
                "{file}: adopted floor {floor:.1} but no scored mutants \
(renamed or removed? update the floor map)"
            ));
        }
    }

    // Badge-only mode: emit the shields.io endpoint badge for the aggregate
    // score and return without enforcing, so the published badge always reflects
    // reality (the separate enforce invocation, with no --badge-out, is the
    // gate). The shape and colour bands mirror coverage-badge.sh so the two
    // product badges read consistently.
    if let Some(badge_out) = crate::flag_value(args, "--badge-out")? {
        let aggregate = if total_considered > 0 {
            100.0 * total_caught as f64 / total_considered as f64
        } else {
            0.0
        };
        let colour = colour_band(aggregate);
        let badge = serde_json::json!({
            "schemaVersion": 1,
            "label": "mutation score",
            "message": format!("{aggregate:.1}%"),
            "color": colour,
        });
        let rendered = serde_json::to_string(&badge)
            .map_err(|e| format!("cannot render mutation badge: {e}"))?;
        std::fs::write(&badge_out, rendered)
            .map_err(|e| format!("cannot write mutation badge to {badge_out}: {e}"))?;
        println!("wrote mutation badge ({aggregate:.1}%, {colour}) to {badge_out}");
        return Ok(0);
    }

    // The unadopted backlog is informational, not a verdict: print it (in the
    // enforce invocation; badge-only mode has already returned) so an upkeep
    // pass has its worklist, then enforce only the adopted floors.
    if !awaiting.is_empty() {
        println!(
            "\nawaiting mutation upkeep (non-blocking): {} unadopted file(s) with surviving mutants",
            awaiting.len()
        );
        for item in &awaiting {
            println!("  - {item}");
        }
    }

    if !failures.is_empty() {
        eprintln!("\nadopted mutation floors violated:");
        // Per-file failures are appended during the sorted per_file walk
        // (BTreeMap iterates sorted), then the vacuous-floor failures during
        // the sorted-by-file FLOORS walk.
        for failure in &failures {
            eprintln!("  - {failure}");
        }
        return Ok(1);
    }
    println!("\nall adopted mutation floors hold");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(file: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({
            "scenario": {"Mutant": {"file": file}},
            "summary": summary,
        })
    }

    #[test]
    fn timeout_counts_as_caught_and_unviable_leaves_denominator() {
        // The baseline is an outcome object whose `scenario` field is the
        // string "Baseline" (as cargo-mutants reports it); it is skipped.
        let data = serde_json::json!({
            "outcomes": [
                {"scenario": "Baseline", "summary": "Success"},
                outcome("a.rs", "CaughtMutant"),
                outcome("a.rs", "Timeout"),
                outcome("a.rs", "Unviable"),
                outcome("a.rs", "MissedMutant"),
            ]
        });
        let per = score_outcomes(&data.to_string()).unwrap();
        let c = per.get("a.rs").unwrap();
        // caught(1)+timeout(1)=2 caught; missed=1; unviable out of denominator.
        let caught = c.caught + c.timeout;
        let denom = caught + c.missed;
        let score = 100.0 * caught as f64 / denom as f64;
        assert_eq!(caught, 2);
        assert_eq!(denom, 3);
        assert!((score - 66.666_666).abs() < 0.01);
    }

    #[test]
    fn unrecognised_summary_fails_closed() {
        let data = serde_json::json!({"outcomes": [outcome("a.rs", "Bogus")]});
        assert!(score_outcomes(&data.to_string()).is_err());
    }

    #[test]
    fn full_denominator_zero_scores_hundred() {
        let data = serde_json::json!({"outcomes": [outcome("a.rs", "Unviable")]});
        let per = score_outcomes(&data.to_string()).unwrap();
        let c = per.get("a.rs").unwrap();
        let caught = c.caught + c.timeout;
        let denom = caught + c.missed;
        assert_eq!(denom, 0);
    }

    #[test]
    fn sharded_outcomes_merge_per_file_counts() {
        // Two shards each scoring the same file; the merged badge must reflect
        // the summed counts (3 caught of 4 considered = 75.0%).
        let dir =
            std::env::temp_dir().join(format!("xtask-mutation-shards-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shard_a = dir.join("a.json");
        let shard_b = dir.join("b.json");
        let badge = dir.join("badge.json");
        std::fs::write(
            &shard_a,
            serde_json::json!({"outcomes": [
                {"scenario": "Baseline", "summary": "Success"},
                outcome("a.rs", "CaughtMutant"),
                outcome("a.rs", "MissedMutant"),
            ]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            &shard_b,
            serde_json::json!({"outcomes": [
                {"scenario": "Baseline", "summary": "Success"},
                outcome("a.rs", "CaughtMutant"),
                outcome("a.rs", "Timeout"),
            ]})
            .to_string(),
        )
        .unwrap();
        let args: Vec<String> = [
            "--outcomes",
            shard_a.to_str().unwrap(),
            "--outcomes",
            shard_b.to_str().unwrap(),
            "--badge-out",
            badge.to_str().unwrap(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let code = run(&args).unwrap();
        assert_eq!(code, 0);
        let rendered: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&badge).unwrap()).unwrap();
        assert_eq!(rendered["message"], "75.0%");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Write an outcomes.json in a fresh temp dir and run the enforce gate
    /// (no --badge-out) over it, returning the exit code. Every adopted floor
    /// is given a caught mutant first so the vacuous-floor guard is satisfied
    /// and the caller can isolate the file under test.
    fn enforce_with(extra: &[serde_json::Value], label: &str) -> i32 {
        let dir =
            std::env::temp_dir().join(format!("xtask-mutation-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("outcomes.json");
        let mut outcomes = vec![serde_json::json!({"scenario": "Baseline", "summary": "Success"})];
        for (file, _) in FLOORS {
            outcomes.push(outcome(file, "CaughtMutant"));
        }
        outcomes.extend_from_slice(extra);
        std::fs::write(&path, serde_json::json!({"outcomes": outcomes}).to_string()).unwrap();
        let args = vec!["--outcomes".to_string(), path.to_str().unwrap().to_string()];
        let code = run(&args).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        code
    }

    #[test]
    fn unadopted_file_with_survivors_is_non_blocking() {
        // A brand-new file with no adopted floor and a surviving mutant is a
        // coverage backlog item, not a gate failure: the run passes.
        let code = enforce_with(
            &[
                outcome("eo-services/src/brand_new_feature.rs", "CaughtMutant"),
                outcome("eo-services/src/brand_new_feature.rs", "MissedMutant"),
            ],
            "unadopted",
        );
        assert_eq!(
            code, 0,
            "an unadopted file's survivors must not fail the gate"
        );
    }

    #[test]
    fn adopted_file_below_its_floor_still_fails() {
        // Regression teeth on adopted surface: cost_engine.rs carries a 92.0
        // floor. The helper gives it one caught mutant; this adds one missed,
        // scoring it 50.0, well below the floor, so the gate fails.
        let code = enforce_with(
            &[outcome("eo-services/src/cost_engine.rs", "MissedMutant")],
            "below-floor",
        );
        assert_eq!(
            code, 1,
            "an adopted file below its floor must fail the gate"
        );
    }

    #[test]
    fn floor_map_has_the_expected_entry_count() {
        // Guards against an accidental drop when editing the map.
        assert_eq!(FLOORS.len(), 28);
    }
}
