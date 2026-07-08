//! Weapon runtime state: the hotbar/trifecta weapon identity, the
//! per-weapon damage-enhancer stacks, cost resolution through the
//! memoised profile caches, and enhancer-break matching.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::cost_engine::cost_per_shot_from_props;
use crate::ped::Ped;
use crate::tool_inference::DamageAttributor;

use super::actor::TrackerActor;
use super::providers::Providers;
use super::session::ActiveSession;
use super::HealTool;

/// The session-scoped weapon runtime: which weapon is live (hotbar or
/// trifecta-attributed), its enhancer stacks, and the memoised
/// profile/cost lookups. Built fresh at session start; the
/// non-identity fields also reset on a mid-session config reload.
#[derive(Default)]
pub(super) struct WeaponRuntime {
    /// The hotbar-reported active tool (hotbar mode only).
    pub(super) hotbar_tool: Option<String>,
    /// Trifecta weapon props by canonical name (truthy props only).
    pub(super) trifecta_profiles: BTreeMap<String, Arc<Value>>,
    /// Damage-enhancer stack state per canonical weapon name.
    pub(super) enhancer_states: BTreeMap<String, DamageEnhancerState>,
    /// The canonical name of the weapon whose enhancer state is live.
    pub(super) active_key: Option<String>,
    /// The tool name as the hotbar/attribution observed it (which may
    /// differ in spelling from the canonical name).
    pub(super) observed_name: Option<String>,
    /// The last tool an offensive shot attributed to (countered shots
    /// re-use it in trifecta mode).
    pub(super) last_offensive_tool: Option<String>,
    /// Damage-signature attribution for trifecta mode.
    pub(super) attributor: DamageAttributor,
    /// Memoised equipment-library profile lookups.
    pub(super) profile_cache: BTreeMap<String, Option<(String, Arc<Value>)>>,
    /// Memoised static per-shot costs for tools without enhancer state.
    pub(super) static_cost_cache: BTreeMap<String, Ped>,
}

impl WeaponRuntime {
    /// The mid-session reset (a config reload leaving trifecta mode):
    /// everything except the hotbar tool identity and the attributor,
    /// exactly the field list the original reset carried (the
    /// attributor is cleared separately at each call site).
    pub(super) fn reset_runtime(&mut self) {
        self.trifecta_profiles.clear();
        self.enhancer_states.clear();
        self.active_key = None;
        self.observed_name = None;
        self.last_offensive_tool = None;
        self.profile_cache.clear();
        self.static_cost_cache.clear();
    }
}

/// Per-weapon damage-enhancer state within the current session.
pub(super) struct DamageEnhancerState {
    pub(super) tool_name: String,
    pub(super) props: Arc<Value>,
    pub(super) stacks: Vec<i64>,
    pub(super) cached_cost: Option<Ped>,
}

impl DamageEnhancerState {
    pub(super) fn from_props(tool_name: &str, props: Arc<Value>) -> Self {
        // `max(0, int(props.get("damage_enhancers", 0) or 0))`.
        let configured = props
            .get("damage_enhancers")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as i64;
        let configured = configured.max(0);
        Self {
            tool_name: tool_name.to_string(),
            props,
            stacks: vec![100; configured as usize],
            cached_cost: None,
        }
    }

    pub(super) fn active_slots(&self) -> i64 {
        self.stacks.iter().filter(|stack| **stack > 0).count() as i64
    }

    /// Redistribute a known total across the slots, front-loading the
    /// remainder.
    pub(super) fn set_total(&mut self, total: i64) {
        let total = total.max(0);
        let slot_count = self.stacks.len() as i64;
        if slot_count == 0 {
            return;
        }
        let per_slot = total / slot_count;
        let remainder = total % slot_count;
        self.stacks = (0..slot_count)
            .map(|index| per_slot + i64::from(index < remainder))
            .collect();
        self.cached_cost = None;
    }

    /// Apply one break; true when a slot fully depleted.
    pub(super) fn apply_break(&mut self, remaining: Option<i64>) -> bool {
        let old_active = self.active_slots();
        match remaining {
            Some(total) if !self.stacks.is_empty() => self.set_total(total),
            _ => {
                for index in (0..self.stacks.len()).rev() {
                    if self.stacks[index] > 0 {
                        self.stacks[index] -= 1;
                        self.cached_cost = None;
                        break;
                    }
                }
            }
        }
        old_active != self.active_slots()
    }

    pub(super) fn current_cost(&mut self) -> Ped {
        if self.cached_cost.is_none() {
            let result = cost_per_shot_from_props(&self.props, Some(self.active_slots()));
            let total = result
                .get("totalCostPerUse")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            // The cost engine prices in PEC; the tracker accounts in PED.
            self.cached_cost = Some(Ped(total / 100.0));
        }
        self.cached_cost.expect("just cached")
    }
}

impl TrackerActor {
    /// Load damage signatures + heal tool from the resolved trifecta
    /// configuration. The weapon fields read with inert defaults
    /// where the original indexes (the resolver supplies complete
    /// weapon objects by contract).
    pub(super) fn load_trifecta_weapon_profiles(
        active: &mut ActiveSession,
        heal_tool: &mut HealTool,
        trifecta: Option<&Map<String, Value>>,
    ) {
        active.weapons.attributor.clear();
        heal_tool.name = None;
        heal_tool.cost_per_use = Ped::ZERO;
        heal_tool.reload_seconds = 2.5;
        heal_tool.amount_min = None;
        heal_tool.amount_max = None;
        active.heal_warning_emitted = false;
        active.weapons.trifecta_profiles.clear();
        active.weapons.active_key = None;
        active.weapons.observed_name = None;

        let Some(trifecta) = trifecta.filter(|map| !map.is_empty()) else {
            return;
        };
        for key in ["small_weapon", "big_weapon"] {
            let Some(weapon) = trifecta.get(key).filter(|value| value_truthy(value)) else {
                continue;
            };
            let name = weapon.get("name").and_then(Value::as_str).unwrap_or("");
            active.weapons.attributor.add_weapon_profile(
                name,
                weapon
                    .get("damage_min")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                weapon
                    .get("damage_max")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                weapon
                    .get("total_damage")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                weapon
                    .get("cost_per_shot_ped")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                weapon.get("role").and_then(Value::as_str),
            );
            if let Some(props) = weapon
                .get("weapon_props")
                .filter(|value| value_truthy(value))
            {
                active
                    .weapons
                    .trifecta_profiles
                    .insert(name.to_string(), Arc::new(props.clone()));
            }
        }
        if let Some(heal) = trifecta
            .get("heal_tool")
            .filter(|value| value_truthy(value))
        {
            heal_tool.name = heal.get("name").and_then(Value::as_str).map(str::to_string);
            heal_tool.cost_per_use = Ped(heal
                .get("cost_per_use_ped")
                .and_then(Value::as_f64)
                .unwrap_or(0.0));
            heal_tool.reload_seconds = heal
                .get("reload_seconds")
                .and_then(Value::as_f64)
                .unwrap_or(2.5);
            heal_tool.amount_min = heal.get("heal_min").and_then(Value::as_f64);
            heal_tool.amount_max = heal.get("heal_max").and_then(Value::as_f64);
        }
    }

    /// Resolve a tool name to its canonical profile: the trifecta
    /// table first, then the memoised equipment-library lookup.
    fn match_weapon_profile(
        providers: &Providers,
        weapons: &mut WeaponRuntime,
        tool_name: &str,
    ) -> Option<(String, Arc<Value>)> {
        // The trifecta table only stores truthy props, so a hit is the
        // original's `if profile:` taken branch.
        if let Some(profile) = weapons.trifecta_profiles.get(tool_name) {
            return Some((tool_name.to_string(), profile.clone()));
        }

        if let Some(cached) = weapons.profile_cache.get(tool_name) {
            return cached.clone();
        }

        let resolved = providers
            .equipment
            .weapon_profile(tool_name)
            .filter(|profile| !profile.is_empty());
        let Some(profile) = resolved else {
            weapons.profile_cache.insert(tool_name.to_string(), None);
            return None;
        };
        // `profile.get("weapon_entity", {}).get("name") or tool_name`.
        let canonical_name = profile
            .get("weapon_entity")
            .and_then(Value::as_object)
            .and_then(|entity| entity.get("name"))
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(tool_name)
            .to_string();
        let matched = Some((canonical_name, Arc::new(Value::Object(profile))));
        weapons
            .profile_cache
            .insert(tool_name.to_string(), matched.clone());
        matched
    }

    /// Resolve (creating if first seen) the enhancer state for a
    /// matched weapon, stamping the active-weapon markers either way.
    pub(super) fn ensure_weapon_state<'a>(
        providers: &Providers,
        weapons: &'a mut WeaponRuntime,
        tool_name: &str,
    ) -> Option<&'a mut DamageEnhancerState> {
        let Some((canonical_name, profile)) =
            Self::match_weapon_profile(providers, weapons, tool_name)
        else {
            weapons.active_key = None;
            weapons.observed_name = Some(tool_name.to_string());
            return None;
        };
        weapons
            .enhancer_states
            .entry(canonical_name.clone())
            .or_insert_with(|| DamageEnhancerState::from_props(&canonical_name, profile));
        weapons.active_key = Some(canonical_name.clone());
        weapons.observed_name = Some(tool_name.to_string());
        weapons.enhancer_states.get_mut(&canonical_name)
    }

    pub(super) fn current_cost_for_tool(
        providers: &Providers,
        weapons: &mut WeaponRuntime,
        tool_name: &str,
        inferred_cost: Ped,
    ) -> Ped {
        if let Some(weapon) = Self::ensure_weapon_state(providers, weapons, tool_name) {
            return weapon.current_cost();
        }
        if inferred_cost.is_positive() {
            return inferred_cost;
        }
        if let Some(cached) = weapons.static_cost_cache.get(tool_name) {
            return *cached;
        }
        let cost = Ped(providers.equipment.cost_per_shot(tool_name));
        weapons
            .static_cost_cache
            .insert(tool_name.to_string(), cost);
        cost
    }
}

/// Whether a break's item name names the active weapon (either the
/// canonical or the observed hotbar spelling), compared on lowercased
/// alphanumerics in either containment direction.
pub(super) fn break_matches_active_weapon(weapons: &WeaponRuntime, item_name: &str) -> bool {
    let Some(weapon) = weapons
        .active_key
        .as_ref()
        .and_then(|key| weapons.enhancer_states.get(key))
    else {
        return false;
    };
    if item_name.is_empty() {
        return false;
    }
    let normalise = |raw: &str| -> String {
        raw.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let item_norm = normalise(item_name);
    let tool_norm = normalise(&weapon.tool_name);
    let observed_norm = normalise(weapons.observed_name.as_deref().unwrap_or(""));
    !item_norm.is_empty()
        && (tool_norm.contains(&item_norm)
            || item_norm.contains(&tool_norm)
            || (!observed_norm.is_empty()
                && (observed_norm.contains(&item_norm) || item_norm.contains(&observed_norm))))
}

/// Python truthiness for the wire values the original's falsy checks
/// guard (null/false/0/""/[]/{} are falsy).
pub(super) fn value_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|inner| inner != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reset_runtime_clears_the_non_identity_state_and_keeps_the_hotbar_tool() {
        let mut runtime = WeaponRuntime {
            hotbar_tool: Some("Opalo".to_string()),
            active_key: Some("opalo".to_string()),
            observed_name: Some("Opalo (L)".to_string()),
            last_offensive_tool: Some("opalo".to_string()),
            ..WeaponRuntime::default()
        };
        runtime
            .trifecta_profiles
            .insert("opalo".to_string(), Arc::new(json!({})));
        runtime.enhancer_states.insert(
            "opalo".to_string(),
            DamageEnhancerState::from_props("Opalo", Arc::new(json!({"damage_enhancers": 2}))),
        );
        runtime.profile_cache.insert("opalo".to_string(), None);
        runtime
            .static_cost_cache
            .insert("opalo".to_string(), Ped(0.5));

        runtime.reset_runtime();

        // The hotbar tool identity survives the reload.
        assert_eq!(runtime.hotbar_tool.as_deref(), Some("Opalo"));
        // Everything else is cleared.
        assert!(runtime.active_key.is_none());
        assert!(runtime.observed_name.is_none());
        assert!(runtime.last_offensive_tool.is_none());
        assert!(runtime.trifecta_profiles.is_empty());
        assert!(runtime.enhancer_states.is_empty());
        assert!(runtime.profile_cache.is_empty());
        assert!(runtime.static_cost_cache.is_empty());
    }
}
