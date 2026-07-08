//! One whole-tree rule over the tracked frontend source, enforcing the
//! runes-native state ruling (ADR-0022) so the legacy-store idiom cannot
//! grow while its remaining modules migrate:
//!
//!   - Rule (frozen legacy-store surface): an import from `svelte/store`
//!     may appear ONLY in the files listed in `LEGACY_STORE_MODULES`, the
//!     modules (and their tests / direct consumers) that predate the
//!     ruling. New shared state is authored as runes-based `.svelte.ts`
//!     modules; as legacy modules migrate, their entries leave this list
//!     and it only ever shrinks.
//!
//! Whole-tree rather than diff-scoped, like the sibling polling lint: the
//! allowlist pins the exact tracked surface, so the guarantee is "no new
//! importer anywhere". The source set is the `git ls-files`-tracked
//! compiled-source files under `app/src`.

use crate::git;

/// The frozen legacy surface: every file importing `svelte/store` at the
/// time of the ruling. Migration removes entries; nothing adds one.
const LEGACY_STORE_MODULES: &[&str] = &[
    "app/src/lib/activityArchive.test.ts",
    "app/src/lib/activityArchive.ts",
    "app/src/lib/components/dashboard/CustomiseStatsWidget.svelte",
    "app/src/lib/motion/testMotion.test.ts",
    "app/src/lib/news.test.ts",
    "app/src/lib/news.ts",
    "app/src/lib/newsFetch.ts",
    "app/src/lib/statsCustomisation.test.ts",
    "app/src/lib/statsCustomisation.ts",
    "app/src/lib/stores/trackingStore.test.ts",
    "app/src/lib/stores/trackingStore.ts",
    "app/src/lib/theme.ts",
    "app/src/lib/updater.test.ts",
    "app/src/lib/updater.ts",
    "app/src/routes/+page.svelte",
];

const SCAN_ROOT: &str = "app/src";
const SCAN_SUFFIXES: &[&str] = &[".svelte", ".ts", ".js", ".mjs", ".cjs", ".jsx", ".tsx"];

/// The import marker. Both quote styles are matched so a formatter change
/// cannot smuggle an import past the guard.
const IMPORT_MARKERS: &[&str] = &["from 'svelte/store'", "from \"svelte/store\""];

/// A single lint violation: file, 1-based line number, detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub lineno: usize,
    pub detail: String,
}

/// Repo-relative tracked compiled-source paths under `app/src`.
fn tracked_sources(repo_root: &std::path::Path) -> Result<Vec<String>, String> {
    let out = git::run(&["ls-files", "--", SCAN_ROOT], repo_root)?;
    Ok(out
        .lines()
        .filter(|line| SCAN_SUFFIXES.iter().any(|s| line.ends_with(s)))
        .map(|line| line.to_string())
        .collect())
}

/// Apply the rule to one file's text.
pub fn scan_text(path: &str, text: &str) -> Vec<Finding> {
    let posix = path.replace('\\', "/");
    if LEGACY_STORE_MODULES.contains(&posix.as_str()) {
        return Vec::new();
    }
    let mut findings: Vec<Finding> = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if IMPORT_MARKERS.iter().any(|marker| line.contains(marker)) {
            findings.push(Finding {
                path: posix.clone(),
                lineno: idx + 1,
                detail: "svelte/store import outside the frozen legacy surface; \
author shared state as a runes-based .svelte.ts module (ADR-0022)"
                    .to_string(),
            });
        }
    }
    findings
}

/// Scan the tracked frontend source and return every finding, plus the
/// allowlist entries that no longer exist (stale entries must be pruned so
/// the frozen surface only shrinks).
fn evaluate(repo_root: &std::path::Path) -> Result<(Vec<Finding>, Vec<String>), String> {
    let sources = tracked_sources(repo_root)?;
    let mut findings: Vec<Finding> = Vec::new();
    for path in &sources {
        let full = repo_root.join(path);
        match std::fs::read_to_string(&full) {
            Ok(text) => findings.extend(scan_text(path, &text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("check-no-new-writable: cannot read {path}: {e}")),
        }
    }
    let stale: Vec<String> = LEGACY_STORE_MODULES
        .iter()
        .filter(|entry| !sources.iter().any(|s| s == *entry))
        .map(|entry| entry.to_string())
        .collect();
    Ok((findings, stale))
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let warn_only = args.iter().any(|a| a == "--warn-only");
    let repo_root = git::repo_root()?;
    let (findings, stale) = evaluate(&repo_root)?;

    if findings.is_empty() && stale.is_empty() {
        println!(
            "check-no-new-writable: no svelte/store imports outside the frozen \
legacy surface."
        );
        return Ok(0);
    }

    if !findings.is_empty() {
        eprintln!(
            "check-no-new-writable: runes-native state ruling violations \
(ADR-0022).\n\nNew shared state is authored with runes in a .svelte.ts module; \
the svelte/store legacy surface is frozen and only shrinks. Offenders:\n"
        );
        for f in &findings {
            eprintln!("  {}:{}: {}", f.path, f.lineno, f.detail);
        }
    }
    if !stale.is_empty() {
        eprintln!(
            "\ncheck-no-new-writable: stale allowlist entries (file no longer \
tracked); remove them from LEGACY_STORE_MODULES so the frozen surface shrinks:\n"
        );
        for entry in &stale {
            eprintln!("  {entry}");
        }
    }

    if warn_only {
        eprintln!(
            "\ncheck-no-new-writable: --warn-only set; exiting 0 despite the findings above."
        );
        return Ok(0);
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_an_import_outside_the_frozen_surface() {
        let f = scan_text(
            "app/src/lib/newStore.ts",
            "import { writable } from 'svelte/store';",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].lineno, 1);
    }

    #[test]
    fn allows_the_frozen_legacy_modules() {
        let f = scan_text(
            "app/src/lib/stores/trackingStore.ts",
            "import { writable } from 'svelte/store';",
        );
        assert!(f.is_empty());
    }

    #[test]
    fn the_check_is_path_normalised() {
        let win = "app/src/lib/theme.ts".replace('/', "\\");
        let f = scan_text(&win, "import { writable } from 'svelte/store';");
        assert!(f.is_empty());
    }

    #[test]
    fn matches_double_quoted_imports() {
        let f = scan_text(
            "app/src/lib/fresh.ts",
            "import { get } from \"svelte/store\";",
        );
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn reports_correct_line_numbers() {
        let text = "// header\nimport { derived } from 'svelte/store';\n";
        let f = scan_text("app/src/lib/fresh.ts", text);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].lineno, 2);
    }

    #[test]
    fn ignores_unrelated_imports() {
        let f = scan_text(
            "app/src/lib/fresh.ts",
            "import { listen } from '@tauri-apps/api/event';",
        );
        assert!(f.is_empty());
    }
}
