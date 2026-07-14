//! The activity recommender: cross-activity skilling arbitrage.
//!
//! A profession's weight vector plays two roles at once: it is the
//! acquisition distribution (how skilling TT value splits across skills
//! while the profession's activity is performed) and the contribution
//! weighting (how each skill moves the profession level). Grinding a
//! target directly therefore grows it by the square of each weight, so
//! an activity structurally starves its own low-weight skills. This
//! module projects, for every performable activity, what a fixed budget
//! of skilling TT buys toward a chosen target (one or more professions,
//! or HP), reading each skill's gain off the TT value curve from its
//! current calibrated level so diminishing returns and the
//! existing-skill discount are priced in.
//!
//! Inputs arrive in the catalogue's nested JSON shapes, exactly as
//! `character_calc` consumes them; the maths composes that module's
//! primitives and the TT curve, nothing new.

use eo_wire::normalizer::round_half_even;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::character_calc::{is_attribute, iter_hp_skills, iter_profession_skills};
use crate::tt_value_curve::levels_for_tt_value;

/// The projection budget: how much skilling TT value ("PES poured in")
/// the simulation covers. Also the chart's x-range.
pub const RECOMMENDER_PES_CAP: f64 = 1000.0;

/// Sample spacing of the projected series along the budget axis.
pub const RECOMMENDER_SAMPLE_STEP: f64 = 20.0;

fn round4(x: f64) -> f64 {
    round_half_even(x, 4)
}

/// What the projection optimises toward.
#[derive(Debug, Clone, PartialEq)]
pub enum RecommenderTarget {
    /// Maximise HP gained (via each skill's `hp_increase` divisor).
    Hp,
    /// Maximise the summed level gain across these professions (one
    /// entry for a single profession, several for a family).
    Professions(Vec<String>),
}

/// A Hit-panel profession's damage-panel sibling where the pairing is
/// not recoverable by name: the ranged damage panels are shared across
/// weapon subtypes by ammo type (pistoleer/sniper/mounted all feed the
/// same `Ranged X (Dmg)` panel). Same-base-name pairs (`Swordsman
/// (Hit)` / `Swordsman (Dmg)`) resolve by suffix and stay out of this
/// table.
const EXPLICIT_DMG_PANELS: [(&str, &str); 10] = [
    ("BLP Pistoleer (Hit)", "Ranged BLP (Dmg)"),
    ("BLP Sniper (Hit)", "Ranged BLP (Dmg)"),
    ("Mounted BLP (Hit)", "Ranged BLP (Dmg)"),
    ("Gauss Sniper (Hit)", "Ranged Gauss (Dmg)"),
    ("Laser Pistoleer (Hit)", "Ranged Laser (Dmg)"),
    ("Laser Sniper (Hit)", "Ranged Laser (Dmg)"),
    ("Mounted Laser (Hit)", "Ranged Laser (Dmg)"),
    ("Plasma Pistoleer (Hit)", "Ranged Plasma (Dmg)"),
    ("Plasma Sniper (Hit)", "Ranged Plasma (Dmg)"),
    ("Mounted Grenadier (Hit)", "Grenadier (Dmg)"),
];

const HIT_SUFFIX: &str = " (Hit)";
const DMG_SUFFIX: &str = " (Dmg)";

/// One skill's share of an activity's projected gain at the budget cap.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContribution {
    pub name: String,
    pub current_level: f64,
    /// Skill levels gained at the budget cap.
    pub level_gain: f64,
    /// Target metric gained at the budget cap (profession levels or HP).
    pub target_gain: f64,
}

/// One activity's projection toward the target.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityProjection {
    /// Display name: the Hit panel's base name for weapon activities,
    /// the profession name verbatim otherwise.
    pub activity: String,
    /// The profession panel(s) the activity trains (Hit + Dmg for
    /// weapons, the profession itself otherwise).
    pub professions: Vec<String>,
    /// Skilling TT needed to gain +1 on the target metric, when reached
    /// within the budget cap.
    pub pes_to_plus_one: Option<f64>,
    /// Target metric gained at the budget cap.
    pub gain_at_cap: f64,
    /// Target metric at each sample point (0, step, 2*step, ... cap).
    pub series: Vec<f64>,
    /// Per-skill decomposition at the cap, largest target share first.
    pub contributors: Vec<SkillContribution>,
}

/// The recommender's full breakdown: ranked candidates plus the
/// direct-grind reference when the target defines one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommenderBreakdown {
    pub pes_cap: f64,
    pub sample_step: f64,
    /// The target's own activity (the faded reference line); absent for
    /// HP and multi-profession targets, which define no single direct
    /// activity.
    pub direct: Option<ActivityProjection>,
    pub candidates: Vec<ActivityProjection>,
}

/// A performable activity: one or two profession panels trained
/// simultaneously (Hit + Dmg split skilling 1:1, per the validated
/// weighting model).
struct Activity {
    name: String,
    panels: Vec<String>,
}

/// One skill row of an activity's projection: acquisition share per
/// PES poured, starting level, and per-level target weight.
struct ProjectionRow {
    name: String,
    share: f64,
    start_level: f64,
    target_weight: f64,
}

fn profession_names(professions: &[Value]) -> Vec<&str> {
    professions
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .filter(|n| !n.is_empty())
        .collect()
}

fn find_profession<'a>(professions: &'a [Value], name: &str) -> Option<&'a Value> {
    professions
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(name))
}

/// The performable activities, in catalogue order: every Hit panel
/// folded with its Dmg sibling into one weapon activity, Dmg panels
/// consumed by that fold, every other profession standing alone.
fn build_activities(professions: &[Value]) -> Vec<Activity> {
    let names = profession_names(professions);
    let mut activities = Vec::new();
    for name in &names {
        if name.ends_with(DMG_SUFFIX) {
            continue;
        }
        if let Some(base) = name.strip_suffix(HIT_SUFFIX) {
            let explicit = EXPLICIT_DMG_PANELS
                .iter()
                .find(|(hit, _)| hit == name)
                .map(|(_, dmg)| *dmg);
            let by_base = format!("{base}{DMG_SUFFIX}");
            let dmg = explicit
                .filter(|dmg| names.contains(dmg))
                .or_else(|| names.iter().find(|n| **n == by_base).copied());
            let mut panels = vec![name.to_string()];
            if let Some(dmg) = dmg {
                panels.push(dmg.to_string());
            }
            activities.push(Activity {
                name: base.to_string(),
                panels,
            });
        } else {
            activities.push(Activity {
                name: name.to_string(),
                panels: vec![name.to_string()],
            });
        }
    }
    activities
}

/// Per-skill target weight: profession-level gain (or HP) per skill
/// level. Attributes are excluded throughout: projections treat them as
/// static (they crawl orders of magnitude slower than skills).
fn target_weights(
    target: &RecommenderTarget,
    professions: &[Value],
    skills_data: &[Value],
) -> Map<String, Value> {
    let mut weights: Map<String, Value> = Map::new();
    match target {
        RecommenderTarget::Hp => {
            for (name, hp_increase) in iter_hp_skills(skills_data) {
                if is_attribute(&name) {
                    continue;
                }
                weights.insert(name, Value::from(1.0 / hp_increase));
            }
        }
        RecommenderTarget::Professions(targets) => {
            for target_name in targets {
                let Some(entity) = find_profession(professions, target_name) else {
                    continue;
                };
                for (name, weight) in iter_profession_skills(entity) {
                    if is_attribute(&name) || weight <= 0.0 {
                        continue;
                    }
                    let prior = weights.get(&name).and_then(Value::as_f64).unwrap_or(0.0);
                    weights.insert(name, Value::from(prior + weight / 10000.0));
                }
            }
        }
    }
    weights
}

/// The activity's projection rows: skills it trains that are unlocked
/// (present in the calibrations), non-attribute, and carry target
/// weight. Acquisition share is the mean panel weight (Hit and Dmg
/// split the poured PES 1:1).
fn projection_rows(
    activity: &Activity,
    professions: &[Value],
    skill_levels: &Map<String, Value>,
    target_weights: &Map<String, Value>,
) -> Vec<ProjectionRow> {
    let mut shares: Map<String, Value> = Map::new();
    let panel_count = activity.panels.len() as f64;
    for panel in &activity.panels {
        let Some(entity) = find_profession(professions, panel) else {
            continue;
        };
        for (name, weight) in iter_profession_skills(entity) {
            if weight <= 0.0 {
                continue;
            }
            let prior = shares.get(&name).and_then(Value::as_f64).unwrap_or(0.0);
            shares.insert(name, Value::from(prior + weight / 100.0 / panel_count));
        }
    }
    let mut rows = Vec::new();
    for (name, share) in &shares {
        let share = share.as_f64().unwrap_or(0.0);
        if is_attribute(name) {
            continue;
        }
        // Absent from the calibrations means the skill is locked (or
        // never trained): it earns nothing until it exists.
        let Some(start_level) = skill_levels.get(name).and_then(Value::as_f64) else {
            continue;
        };
        let Some(target_weight) = target_weights.get(name).and_then(Value::as_f64) else {
            continue;
        };
        rows.push(ProjectionRow {
            name: name.clone(),
            share,
            start_level,
            target_weight,
        });
    }
    rows
}

/// Target metric gained after pouring `pes` of skilling TT into the
/// activity. Closed-form per skill (no path dependence without unlock
/// milestones): each skill's level is read off the curve at its
/// accumulated share.
fn evaluate(rows: &[ProjectionRow], pes: f64) -> f64 {
    rows.iter()
        .map(|row| levels_for_tt_value(row.start_level, row.share * pes) * row.target_weight)
        .sum()
}

/// PES to reach +1 on the target metric: locate the bracketing sample,
/// then bisect the exact evaluation inside it.
fn pes_to_plus_one(rows: &[ProjectionRow], series: &[f64]) -> Option<f64> {
    // A zero budget buys zero gain, so the first sample is always 0.0
    // and any crossing sits at index 1 or later.
    let crossing = series.iter().position(|gain| *gain >= 1.0)?;
    let mut lo = (crossing as f64 - 1.0) * RECOMMENDER_SAMPLE_STEP;
    let mut hi = crossing as f64 * RECOMMENDER_SAMPLE_STEP;
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if evaluate(rows, mid) < 1.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(round4(hi))
}

fn project(activity: &Activity, rows: &[ProjectionRow]) -> ActivityProjection {
    let sample_count = (RECOMMENDER_PES_CAP / RECOMMENDER_SAMPLE_STEP) as usize + 1;
    let series: Vec<f64> = (0..sample_count)
        .map(|i| round4(evaluate(rows, i as f64 * RECOMMENDER_SAMPLE_STEP)))
        .collect();
    let gain_at_cap = *series.last().expect("series has at least the zero sample");
    let mut contributors: Vec<SkillContribution> = rows
        .iter()
        .filter_map(|row| {
            let level_gain = levels_for_tt_value(row.start_level, row.share * RECOMMENDER_PES_CAP);
            let target_gain = level_gain * row.target_weight;
            (target_gain > 0.0).then(|| SkillContribution {
                name: row.name.clone(),
                current_level: row.start_level,
                level_gain: round4(level_gain),
                target_gain: round4(target_gain),
            })
        })
        .collect();
    contributors.sort_by(|a, b| {
        b.target_gain
            .partial_cmp(&a.target_gain)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    ActivityProjection {
        activity: activity.name.clone(),
        professions: activity.panels.clone(),
        pes_to_plus_one: pes_to_plus_one(rows, &series),
        gain_at_cap,
        series,
        contributors,
    }
}

/// The full recommendation: every performable activity projected
/// against the target and ranked by PES-to-+1 (quickest first; the
/// low-hanging-fruit metric), activities that never reach +1 within the
/// cap trailing by gain-at-cap. Activities training none of the
/// target's skills are dropped. For a single-profession target the
/// target's own activity is projected separately as the direct-grind
/// reference and excluded from the candidates.
pub fn activity_recommender(
    skill_levels: &Map<String, Value>,
    professions: &[Value],
    skills_data: &[Value],
    target: &RecommenderTarget,
) -> RecommenderBreakdown {
    let weights = target_weights(target, professions, skills_data);
    let activities = build_activities(professions);
    let target_panels: Vec<&str> = match target {
        RecommenderTarget::Hp => Vec::new(),
        RecommenderTarget::Professions(names) => names.iter().map(String::as_str).collect(),
    };
    let is_direct = |activity: &Activity| {
        activity
            .panels
            .iter()
            .any(|p| target_panels.contains(&p.as_str()))
    };

    let direct = match target {
        RecommenderTarget::Professions(names) if names.len() == 1 => activities
            .iter()
            .find(|activity| is_direct(activity))
            .map(|activity| {
                let rows = projection_rows(activity, professions, skill_levels, &weights);
                project(activity, &rows)
            }),
        _ => None,
    };

    let mut candidates: Vec<ActivityProjection> = activities
        .iter()
        .filter(|activity| !is_direct(activity))
        .filter_map(|activity| {
            let rows = projection_rows(activity, professions, skill_levels, &weights);
            let projection = project(activity, &rows);
            (projection.gain_at_cap > 0.0).then_some(projection)
        })
        .collect();
    candidates.sort_by(|a, b| {
        use std::cmp::Ordering;
        let by_gain = |x: &ActivityProjection, y: &ActivityProjection| {
            y.gain_at_cap
                .partial_cmp(&x.gain_at_cap)
                .unwrap_or(Ordering::Equal)
        };
        match (a.pes_to_plus_one, b.pes_to_plus_one) {
            (Some(x), Some(y)) => x
                .partial_cmp(&y)
                .unwrap_or(Ordering::Equal)
                .then_with(|| by_gain(a, b))
                .then_with(|| a.activity.cmp(&b.activity)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => by_gain(a, b).then_with(|| a.activity.cmp(&b.activity)),
        }
    });

    RecommenderBreakdown {
        pes_cap: RECOMMENDER_PES_CAP,
        sample_step: RECOMMENDER_SAMPLE_STEP,
        direct,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn levels(pairs: &[(&str, f64)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(name, level)| (name.to_string(), json!(level)))
            .collect()
    }

    fn profession(name: &str, skills: &[(&str, f64)]) -> Value {
        json!({
            "name": name,
            "skills": skills
                .iter()
                .map(|(skill, weight)| json!({"skill": {"name": skill}, "weight": weight}))
                .collect::<Vec<_>>(),
        })
    }

    /// The fixture catalogue: a same-base-name weapon pair, an
    /// explicit-table weapon pair, a single-panel activity, an activity
    /// with no target overlap, and the target profession itself.
    fn catalogue() -> Vec<Value> {
        vec![
            profession(
                "Alpha (Hit)",
                &[("Strength", 10.0), ("Aim", 60.0), ("Anatomy", 30.0)],
            ),
            profession("Alpha (Dmg)", &[("Anatomy", 50.0), ("Wounding", 50.0)]),
            profession("BLP Pistoleer (Hit)", &[("Aim", 100.0)]),
            profession("Ranged BLP (Dmg)", &[("Anatomy", 100.0)]),
            profession("Harvest", &[("Anatomy", 100.0)]),
            profession("Unrelated", &[("Bravado", 100.0)]),
            profession(
                "Target",
                &[("Anatomy", 40.0), ("Wounding", 40.0), ("Aim", 20.0)],
            ),
        ]
    }

    fn skills_data() -> Vec<Value> {
        vec![
            json!({"name": "Anatomy", "hp_increase": 50.0}),
            json!({"name": "Aim", "hp_increase": null}),
        ]
    }

    fn calibrations() -> Map<String, Value> {
        // Wounding is absent: locked, earns nothing.
        levels(&[
            ("Anatomy", 100.0),
            ("Aim", 2000.0),
            ("Strength", 50.0),
            ("Bravado", 10.0),
        ])
    }

    fn run(target: RecommenderTarget) -> RecommenderBreakdown {
        activity_recommender(&calibrations(), &catalogue(), &skills_data(), &target)
    }

    fn candidate<'a>(breakdown: &'a RecommenderBreakdown, name: &str) -> &'a ActivityProjection {
        breakdown
            .candidates
            .iter()
            .find(|c| c.activity == name)
            .unwrap_or_else(|| panic!("candidate '{name}' present"))
    }

    #[test]
    fn activities_fold_hit_and_dmg_panels() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        assert_eq!(
            candidate(&breakdown, "Alpha").professions,
            vec!["Alpha (Hit)", "Alpha (Dmg)"]
        );
        // The explicit table pairs the ranged panels no name rule reaches.
        assert_eq!(
            candidate(&breakdown, "BLP Pistoleer").professions,
            vec!["BLP Pistoleer (Hit)", "Ranged BLP (Dmg)"]
        );
        // Dmg panels never stand alone; zero-overlap activities drop.
        let names: Vec<&str> = breakdown
            .candidates
            .iter()
            .map(|c| c.activity.as_str())
            .collect();
        assert!(!names.iter().any(|n| n.contains("(Dmg)")));
        assert!(!names.contains(&"Unrelated"));
    }

    #[test]
    fn gains_compose_shares_curve_and_target_weights() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        // Harvest pours everything into Anatomy (weight 40 in Target).
        let expected = round4(levels_for_tt_value(100.0, 1000.0) * 40.0 / 10000.0);
        assert_eq!(candidate(&breakdown, "Harvest").gain_at_cap, expected);
        // Alpha splits Hit and Dmg 1:1; the attribute (Strength) and the
        // locked skill (Wounding) earn nothing.
        let aim = levels_for_tt_value(2000.0, 60.0 / 2.0 / 100.0 * 1000.0) * 20.0 / 10000.0;
        let anatomy =
            levels_for_tt_value(100.0, (30.0 + 50.0) / 2.0 / 100.0 * 1000.0) * 40.0 / 10000.0;
        assert_eq!(
            candidate(&breakdown, "Alpha").gain_at_cap,
            round4(aim + anatomy)
        );
    }

    #[test]
    fn series_starts_at_zero_and_never_decreases() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        for projection in breakdown.candidates.iter().chain(breakdown.direct.as_ref()) {
            let expected_len = (RECOMMENDER_PES_CAP / RECOMMENDER_SAMPLE_STEP) as usize + 1;
            assert_eq!(projection.series.len(), expected_len);
            assert_eq!(projection.series[0], 0.0);
            assert!(projection.series.windows(2).all(|pair| pair[1] >= pair[0]));
            assert_eq!(projection.gain_at_cap, *projection.series.last().unwrap());
        }
    }

    #[test]
    fn ranking_is_quickest_to_plus_one_first() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        let thresholds: Vec<Option<f64>> = breakdown
            .candidates
            .iter()
            .map(|c| c.pes_to_plus_one)
            .collect();
        // Every Some precedes every None, and the Somes ascend.
        let somes: Vec<f64> = thresholds.iter().flatten().copied().collect();
        assert!(somes.windows(2).all(|pair| pair[0] <= pair[1]));
        let first_none = thresholds.iter().position(Option::is_none);
        if let Some(first_none) = first_none {
            assert!(thresholds[first_none..].iter().all(Option::is_none));
        }
        // The threshold sits inside the sample bracket where the series
        // first crosses 1.0.
        for candidate in &breakdown.candidates {
            let Some(threshold) = candidate.pes_to_plus_one else {
                continue;
            };
            let crossing = candidate
                .series
                .iter()
                .position(|gain| *gain >= 1.0)
                .expect("a threshold implies a crossing");
            let hi = crossing as f64 * RECOMMENDER_SAMPLE_STEP;
            assert!(threshold <= hi && threshold >= hi - RECOMMENDER_SAMPLE_STEP);
        }
    }

    #[test]
    fn single_profession_target_gets_a_direct_reference() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        let direct = breakdown.direct.as_ref().expect("direct reference present");
        assert_eq!(direct.activity, "Target");
        assert!(breakdown.candidates.iter().all(|c| c.activity != "Target"));
        // Direct grinding of Target reaches Aim (weight 20, level 2000)
        // and Anatomy (weight 40, level 100); Wounding stays locked.
        let aim = levels_for_tt_value(2000.0, 0.20 * 1000.0) * 20.0 / 10000.0;
        let anatomy = levels_for_tt_value(100.0, 0.40 * 1000.0) * 40.0 / 10000.0;
        assert_eq!(direct.gain_at_cap, round4(aim + anatomy));
    }

    #[test]
    fn family_targets_sum_members_and_carry_no_direct_line() {
        let breakdown = run(RecommenderTarget::Professions(vec![
            "Target".to_string(),
            "Harvest".to_string(),
        ]));
        assert!(breakdown.direct.is_none());
        // Both member activities are excluded from the candidates.
        assert!(breakdown
            .candidates
            .iter()
            .all(|c| c.activity != "Target" && c.activity != "Harvest"));
        // Anatomy now carries weight 40 + 100 across the family.
        let aim = levels_for_tt_value(2000.0, 0.30 * 1000.0) * 20.0 / 10000.0;
        let anatomy = levels_for_tt_value(100.0, 0.40 * 1000.0) * 140.0 / 10000.0;
        assert_eq!(
            candidate(&breakdown, "Alpha").gain_at_cap,
            round4(aim + anatomy)
        );
    }

    #[test]
    fn hp_target_uses_hp_divisors_and_no_direct_line() {
        let breakdown = run(RecommenderTarget::Hp);
        assert!(breakdown.direct.is_none());
        let expected = round4(levels_for_tt_value(100.0, 1000.0) / 50.0);
        assert_eq!(candidate(&breakdown, "Harvest").gain_at_cap, expected);
        // Aim has no hp_increase: an Aim-only activity would gain zero
        // HP, so BLP Pistoleer's HP gain comes from Anatomy alone.
        let anatomy = levels_for_tt_value(100.0, 0.50 * 1000.0) / 50.0;
        assert_eq!(
            candidate(&breakdown, "BLP Pistoleer").gain_at_cap,
            round4(anatomy)
        );
    }

    #[test]
    fn contributors_decompose_the_cap_gain_largest_first() {
        let breakdown = run(RecommenderTarget::Professions(vec!["Target".to_string()]));
        let alpha = candidate(&breakdown, "Alpha");
        let total: f64 = alpha.contributors.iter().map(|c| c.target_gain).sum();
        assert!((total - alpha.gain_at_cap).abs() < 1e-3);
        assert!(alpha
            .contributors
            .windows(2)
            .all(|pair| pair[0].target_gain >= pair[1].target_gain));
        // The locked skill and the attribute never appear.
        assert!(alpha
            .contributors
            .iter()
            .all(|c| c.name != "Wounding" && c.name != "Strength"));
    }

    /// Build the sampled series the way [`project`] does, so a
    /// [`pes_to_plus_one`] threshold can be checked against the exact
    /// evaluation it is meant to invert.
    fn series_for(rows: &[ProjectionRow]) -> Vec<f64> {
        let step = RECOMMENDER_SAMPLE_STEP;
        let count = (RECOMMENDER_PES_CAP / step) as usize + 1;
        (0..count)
            .map(|i| round4(evaluate(rows, i as f64 * step)))
            .collect()
    }

    /// A single row calibrated so the gain equals exactly +1 at
    /// `crossing_pes` (target weight is the reciprocal of the curve gain
    /// there), placing the true crossing at a known interior point.
    fn row_crossing_at(crossing_pes: f64) -> Vec<ProjectionRow> {
        vec![ProjectionRow {
            name: "X".to_string(),
            share: 1.0,
            start_level: 0.0,
            target_weight: 1.0 / levels_for_tt_value(0.0, crossing_pes),
        }]
    }

    #[test]
    fn pes_to_plus_one_lands_mid_first_bracket() {
        // Crossing calibrated to the middle of the (0, step] bracket. A
        // correct bisection reports a threshold there; every mangled
        // variant (a return-value swap, a collapsed bracket bound, or a
        // broken bisection step) instead reports 0, a sample edge, or a
        // negative, all outside the interior band asserted here.
        let step = RECOMMENDER_SAMPLE_STEP;
        let rows = row_crossing_at(step / 2.0);
        let series = series_for(&rows);
        assert_eq!(series[0], 0.0);
        assert!(series[1] >= 1.0, "crossing must land at index 1");

        let threshold = pes_to_plus_one(&rows, &series).expect("a crossing exists");
        assert!(
            threshold > step * 0.25 && threshold < step * 0.75,
            "threshold {threshold} outside the interior band"
        );
    }

    #[test]
    fn pes_to_plus_one_lands_mid_later_bracket() {
        // Crossing calibrated to the middle of the (step, 2*step] bracket:
        // the gain stays below +1 through the first sample and crosses in
        // the second. This pins the upper bracket bound specifically; a
        // bound that collapses toward the lower sample reports a threshold
        // well below the true interior crossing.
        let step = RECOMMENDER_SAMPLE_STEP;
        let rows = row_crossing_at(step * 1.5);
        let series = series_for(&rows);
        assert!(
            series[1] < 1.0 && series[2] >= 1.0,
            "crossing must land at index 2"
        );

        let threshold = pes_to_plus_one(&rows, &series).expect("a crossing exists");
        assert!(
            threshold > step * 1.25 && threshold < step * 1.75,
            "threshold {threshold} outside the interior band"
        );
    }
}
