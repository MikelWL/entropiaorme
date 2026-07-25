//! One whole-tree rule over the tracked frontend source, keeping the
//! in-development register and its consumers in step:
//!
//!   - Rule (no orphan on either side): every `id` declared in
//!     `IN_DEVELOPMENT_SURFACES` has at least one consumer, and every id a
//!     consumer references is declared. A marker with no entry ships an
//!     unexplained badge; an entry with no consumer is a surface that has
//!     since been finished (or removed) while still advertising itself as
//!     unfinished.
//!
//! Why this is a guard rather than a written convention: a surface lands
//! ahead of its capability precisely when attention is elsewhere, and
//! graduating it is the step most easily forgotten. Without a mechanical
//! check the register decays into scattered dead entries over a few release
//! cycles, which is why the convention ships with its own check.
//!
//!   - Rule (the published channel is stamped): every step in the release
//!     workflow that builds the frontend sets `ENTROPIAORME_STABLE_CHANNEL`.
//!     That stamp is what hides in-development surfaces from people who did
//!     not build the app. Losing it is silent in CI and visible only to
//!     whoever downloads the release, so it gets a mechanical check.
//!
//! Whole-tree rather than diff-scoped, like the sibling frontend lints: the
//! guarantee is "no orphan anywhere", not "this diff added none".

use crate::git;

const REGISTRY: &str = "app/src/lib/inDevelopment/registry.ts";
const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";
/// Build invocations in the release workflow that produce frontend assets.
const FRONTEND_BUILD_MARKERS: &[&str] = &["tauri -- build", "build-installer.ps1"];
/// The stamp must appear in the step's own `env:` block, which by GitHub
/// Actions convention precedes its `run:`. A line window is a heuristic, not
/// a YAML parse: it catches the realistic regression (the stamp deleted, or a
/// new build path added without it) and would miss a step whose env block is
/// unusually far from its run line.
const ENV_LOOKBACK_LINES: usize = 20;
const CHANNEL_STAMP: &str = "ENTROPIAORME_STABLE_CHANNEL";

/// A single lint violation: file, 1-based line number, detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub lineno: usize,
    pub detail: String,
}

/// Ids declared in the register, as `(lineno, id)`. Reads the `id:` field of
/// each entry literal; the register is a flat `as const` array by design, so
/// a line scan is sufficient and keeps the guard dependency-free.
pub fn declared_ids(text: &str) -> Vec<(usize, String)> {
    let mut ids = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("id:") else {
            continue;
        };
        let rest = rest.trim();
        let quote = match rest.chars().next() {
            Some(c @ ('\'' | '"')) => c,
            _ => continue,
        };
        if let Some(end) = rest[1..].find(quote) {
            ids.push((idx + 1, rest[1..1 + end].to_string()));
        }
    }
    ids
}

/// Ids referenced by a consumer: `<InDevelopmentMark id="..." />` in markup
/// and `inDevelopmentSurface('...')` in logic.
pub fn referenced_ids(text: &str) -> Vec<(usize, String)> {
    let mut ids = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        for (marker, opener) in [("InDevelopmentMark", "id="), ("inDevelopmentSurface", "(")] {
            let Some(at) = line.find(marker) else {
                continue;
            };
            let tail = &line[at + marker.len()..];
            let Some(open_at) = tail.find(opener) else {
                continue;
            };
            let rest = tail[open_at + opener.len()..].trim_start();
            let quote = match rest.chars().next() {
                Some(c @ ('\'' | '"')) => c,
                _ => continue,
            };
            if let Some(end) = rest[1..].find(quote) {
                ids.push((idx + 1, rest[1..1 + end].to_string()));
            }
        }
    }
    ids
}

/// Flag any frontend-producing build step in the release workflow that does
/// not stamp the published channel.
pub fn channel_stamp_findings(text: &str) -> Vec<Finding> {
    let lines: Vec<&str> = text.lines().collect();
    let mut findings = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        // Comments name these scripts when explaining a step; only an actual
        // invocation matters.
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some(marker) = FRONTEND_BUILD_MARKERS.iter().find(|m| line.contains(**m)) else {
            continue;
        };
        let start = idx.saturating_sub(ENV_LOOKBACK_LINES);
        if lines[start..=idx].iter().any(|l| l.contains(CHANNEL_STAMP)) {
            continue;
        }
        findings.push(Finding {
            path: RELEASE_WORKFLOW.to_string(),
            lineno: idx + 1,
            detail: format!(
                "builds the frontend ({marker}) without setting {CHANNEL_STAMP}; a published \
artefact would carry surfaces still registered as in-development"
            ),
        });
    }
    findings
}

/// Repo-relative tracked frontend sources, excluding the register itself and
/// test files (a test may exercise an unregistered id deliberately).
fn tracked_sources(repo_root: &std::path::Path) -> Result<Vec<String>, String> {
    let out = git::run(&["ls-files", "--", "app/src"], repo_root)?;
    Ok(out
        .lines()
        .filter(|line| line.ends_with(".ts") || line.ends_with(".svelte"))
        .filter(|line| *line != REGISTRY)
        .filter(|line| !line.ends_with(".test.ts") && !line.ends_with(".spec.ts"))
        .map(str::to_string)
        .collect())
}

fn evaluate(repo_root: &std::path::Path) -> Result<Vec<Finding>, String> {
    let registry_path = repo_root.join(REGISTRY);
    let registry_text = std::fs::read_to_string(&registry_path)
        .map_err(|e| format!("in-development: cannot read {REGISTRY}: {e}"))?;
    let declared = declared_ids(&registry_text);

    let mut findings = Vec::new();
    let mut referenced: Vec<String> = Vec::new();

    for path in tracked_sources(repo_root)? {
        let full = repo_root.join(&path);
        let text = match std::fs::read_to_string(&full) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("in-development: cannot read {path}: {e}")),
        };
        for (lineno, id) in referenced_ids(&text) {
            if !declared.iter().any(|(_, d)| *d == id) {
                findings.push(Finding {
                    path: path.replace('\\', "/"),
                    lineno,
                    detail: format!(
                        "references in-development surface {id:?}, which is not declared in {REGISTRY}"
                    ),
                });
            }
            referenced.push(id);
        }
    }

    for (lineno, id) in &declared {
        if !referenced.contains(id) {
            findings.push(Finding {
                path: REGISTRY.to_string(),
                lineno: *lineno,
                detail: format!(
                    "declares in-development surface {id:?} with no consumer; if its capability \
landed, delete the entry and its marker"
                ),
            });
        }
    }

    let workflow_path = repo_root.join(RELEASE_WORKFLOW);
    match std::fs::read_to_string(&workflow_path) {
        Ok(text) => findings.extend(channel_stamp_findings(&text)),
        Err(e) => {
            return Err(format!(
                "in-development: cannot read {RELEASE_WORKFLOW}: {e}"
            ))
        }
    }

    findings.sort_by(|a, b| (&a.path, a.lineno).cmp(&(&b.path, b.lineno)));
    Ok(findings)
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let warn_only = args.iter().any(|a| a == "--warn-only");
    let repo_root = git::repo_root()?;
    let findings = evaluate(&repo_root)?;

    if findings.is_empty() {
        println!("in-development: register and consumers agree.");
        return Ok(0);
    }

    eprintln!(
        "in-development: register/consumer mismatch.\n\nA surface that ships ahead of its \
capability is declared once in {REGISTRY} and marked where it renders. Graduating it means \
deleting the entry and the marker together. Offenders:\n"
    );
    for f in &findings {
        eprintln!("  {}:{}: {}", f.path, f.lineno, f.detail);
    }

    if warn_only {
        eprintln!("\nin-development: --warn-only set; exiting 0 despite the findings above.");
        return Ok(0);
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_declared_ids_from_the_register() {
        let text = "export const IN_DEVELOPMENT_SURFACES = [\n\t{\n\t\tid: 'alpha',\n\t\tsummary: 'x',\n\t},\n];\n";
        assert_eq!(declared_ids(text), vec![(3, "alpha".to_string())]);
    }

    #[test]
    fn reads_a_marker_reference_from_markup() {
        let text = "<InDevelopmentMark id=\"alpha\" />\n";
        assert_eq!(referenced_ids(text), vec![(1, "alpha".to_string())]);
    }

    #[test]
    fn reads_a_helper_reference_from_logic() {
        let text = "const s = inDevelopmentSurface('beta');\n";
        assert_eq!(referenced_ids(text), vec![(1, "beta".to_string())]);
    }

    #[test]
    fn ignores_a_marker_import_without_an_id() {
        let text = "import { InDevelopmentMark } from '$lib/inDevelopment';\n";
        assert!(referenced_ids(text).is_empty());
    }

    #[test]
    fn flags_a_frontend_build_step_missing_the_channel_stamp() {
        let yml = "      - name: Build\n        run: npm run tauri -- build --bundles deb\n";
        let f = channel_stamp_findings(yml);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].lineno, 2);
        assert!(f[0].detail.contains("ENTROPIAORME_STABLE_CHANNEL"));
    }

    #[test]
    fn accepts_a_build_step_whose_env_block_stamps_the_channel() {
        let yml = "      - name: Build\n        env:\n          ENTROPIAORME_STABLE_CHANNEL: '1'\n        run: ./scripts/build-installer.ps1\n";
        assert!(channel_stamp_findings(yml).is_empty());
    }

    #[test]
    fn flags_the_windows_installer_path_too() {
        let yml = "        run: ./scripts/build-installer.ps1\n";
        assert_eq!(channel_stamp_findings(yml).len(), 1);
    }

    #[test]
    fn ignores_a_comment_naming_a_build_script() {
        let yml = "        # build-installer.ps1 produces the per-user MSI payload\n";
        assert!(channel_stamp_findings(yml).is_empty());
    }

    #[test]
    fn ignores_a_stamp_too_far_above_the_build_step() {
        let mut yml = String::from("        env:\n          ENTROPIAORME_STABLE_CHANNEL: '1'\n");
        for _ in 0..ENV_LOOKBACK_LINES {
            yml.push_str("        # filler\n");
        }
        yml.push_str("        run: ./scripts/build-installer.ps1\n");
        assert_eq!(channel_stamp_findings(&yml).len(), 1);
    }

    #[test]
    fn tolerates_either_quote_style() {
        assert_eq!(
            referenced_ids("<InDevelopmentMark id='alpha' />\n"),
            vec![(1, "alpha".to_string())]
        );
        assert_eq!(
            declared_ids("\t\tid: \"beta\",\n"),
            vec![(1, "beta".to_string())]
        );
    }
}
