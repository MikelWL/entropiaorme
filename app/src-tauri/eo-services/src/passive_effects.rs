//! Persistent equipment effects and their shared evaluators.
//!
//! A source is the durable item or condition the user has declared, while
//! effects are typed capabilities carried by that source. Persistent clothing
//! is the only source lifetime supported today; time-bounded consumables can
//! later feed the same evaluator without changing its callers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveEffectKind {
    ReloadSpeed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveEffect {
    pub kind: PassiveEffectKind,
    pub magnitude_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassiveEffectSource {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub effects: Vec<PassiveEffect>,
}

pub fn reload_speed_percent(sources: &[PassiveEffectSource]) -> f64 {
    sources
        .iter()
        .filter(|source| source.enabled)
        .flat_map(|source| &source.effects)
        .filter(|effect| effect.kind == PassiveEffectKind::ReloadSpeed)
        .map(|effect| effect.magnitude_percent)
        .sum()
}

/// Convert a base reload duration into the duration under the declared speed
/// multiplier. Invalid legacy or hand-edited totals fail safe to the base
/// duration; the settings boundary prevents new totals at or below -100%.
pub fn effective_reload_seconds(base_seconds: f64, sources: &[PassiveEffectSource]) -> f64 {
    let multiplier = 1.0 + reload_speed_percent(sources) / 100.0;
    if !base_seconds.is_finite() || base_seconds < 0.0 || multiplier <= 0.0 {
        return base_seconds;
    }
    base_seconds / multiplier
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(enabled: bool, magnitude_percent: f64) -> PassiveEffectSource {
        PassiveEffectSource {
            id: "ares-perfect".into(),
            name: "Ares Ring, Perfected".into(),
            enabled,
            effects: vec![PassiveEffect {
                kind: PassiveEffectKind::ReloadSpeed,
                magnitude_percent,
            }],
        }
    }

    #[test]
    fn reload_speed_is_a_throughput_multiplier() {
        let effective = effective_reload_seconds(2.5, &[source(true, 14.0)]);
        assert!((effective - 2.192_982_456).abs() < 0.000_000_001);
    }

    #[test]
    fn disabled_and_negative_sources_compose_without_special_cases() {
        let sources = [source(true, 20.0), source(false, 50.0), source(true, -10.0)];
        assert!((reload_speed_percent(&sources) - 10.0).abs() < f64::EPSILON);
        assert!((effective_reload_seconds(2.5, &sources) - 2.272_727_273).abs() < 0.000_000_001);
    }
}
