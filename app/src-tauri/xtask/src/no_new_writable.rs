//! One whole-tree rule over the tracked frontend source, enforcing the
//! runes-native state ruling (ADR-0022):
//!
//!   - Rule (no legacy-store imports): no tracked frontend source file may
//!     import from `svelte/store`. Shared state is authored as runes-based
//!     `.svelte.ts` modules. The allowlist below froze the legacy surface
//!     while it migrated; the migration is complete and the list is empty.
//!     It stays wired in so the guarantee keeps its shape: an entry can
//!     only ever be removed, never added, and the stale-entry check fails
//!     on any listed file that is no longer tracked.
//!
//! Whole-tree rather than diff-scoped, like the sibling polling lint: the
//! guarantee is "no importer anywhere". The source set is the
//! `git ls-files`-tracked compiled-source files under `app/src`.

use crate::git;

/// The frozen legacy surface: every file importing `svelte/store` at the
/// time of the ruling. Migration removed every entry; nothing adds one.
const LEGACY_STORE_MODULES: &[&str] = &[];

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
                detail: "svelte/store import; author shared state as a \
runes-based .svelte.ts module (ADR-0022)"
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
        println!("check-no-new-writable: no svelte/store imports in the tracked frontend source.");
        return Ok(0);
    }

    if !findings.is_empty() {
        eprintln!(
            "check-no-new-writable: runes-native state ruling violations \
(ADR-0022).\n\nShared state is authored with runes in a .svelte.ts module; \
the svelte/store surface is fully migrated and stays empty. Offenders:\n"
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
    fn flags_an_import_anywhere_in_the_tree() {
        let f = scan_text(
            "app/src/lib/newStore.ts",
            "import { writable } from 'svelte/store';",
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].lineno, 1);
    }

    #[test]
    fn flags_the_formerly_frozen_modules() {
        // The legacy surface is fully migrated: a once-allowlisted path gets
        // no special treatment.
        let f = scan_text(
            "app/src/lib/stores/trackingStore.ts",
            "import { writable } from 'svelte/store';",
        );
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn findings_are_path_normalised() {
        let win = "app/src/lib/theme.ts".replace('/', "\\");
        let f = scan_text(&win, "import { writable } from 'svelte/store';");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].path, "app/src/lib/theme.ts");
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
