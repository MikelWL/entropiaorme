//! Weapon runtime state: trifecta damage-signature profiles, the
//! per-weapon damage-enhancer stacks, cost resolution through the
//! memoised profile caches, and enhancer-break matching.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::cost_engine::cost_per_shot_from_props;

use super::{HuntTracker, TrackerState};

/// Per-weapon damage-enhancer state within the current session.
pub(super) struct DamageEnhancerState {
    pub(super) tool_name: String,
    pub(super) props: Arc<Value>,
    pub(super) stacks: Vec<i64>,
    pub(super) cached_cost_ped: Option<f64>,
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
            cached_cost_ped: None,
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
        self.cached_cost_ped = None;
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
                        self.cached_cost_ped = None;
                        break;
                    }
                }
            }
        }
        old_active != self.active_slots()
    }

    pub(super) fn current_cost_ped(&mut self) -> f64 {
        if self.cached_cost_ped.is_none() {
            let result = cost_per_shot_from_props(&self.props, Some(self.active_slots()));
            let total = result
                .get("totalCostPerUse")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            self.cached_cost_ped = Some(total / 100.0);
        }
        self.cached_cost_ped.expect("just cached")
    }
}

impl HuntTracker {
    pub(super) fn reset_weapon_runtime_state(state: &mut TrackerState) {
        state.trifecta_weapon_profiles.clear();
        state.weapon_enhancer_states.clear();
        state.active_weapon_state_key = None;
        state.active_weapon_observed_name = None;
        state.last_offensive_tool_name = None;
        state.profile_match_cache.clear();
        state.static_tool_cost_cache.clear();
    }

    /// Load damage signatures + heal tool from the resolved trifecta
    /// configuration. The weapon fields read with inert defaults
    /// where the original indexes (the resolver supplies complete
    /// weapon objects by contract).
    pub(super) fn load_trifecta_weapon_profiles(
        state: &mut TrackerState,
        trifecta: Option<&Map<String, Value>>,
    ) {
        state.damage_attributor.clear();
        state.active_heal_tool_name = None;
        state.heal_cost_per_use_ped = 0.0;
        state.heal_reload_seconds = 2.5;
        state.heal_amount_min = None;
        state.heal_amount_max = None;
        state.heal_warning_emitted = false;
        state.trifecta_weapon_profiles.clear();
        state.active_weapon_state_key = None;
        state.active_weapon_observed_name = None;

        let Some(trifecta) = trifecta.filter(|map| !map.is_empty()) else {
            return;
        };
        for key in ["small_weapon", "big_weapon"] {
            let Some(weapon) = trifecta.get(key).filter(|value| value_truthy(value)) else {
                continue;
            };
            let name = weapon.get("name").and_then(Value::as_str).unwrap_or("");
            state.damage_attributor.add_weapon_profile(
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
                state
                    .trifecta_weapon_profiles
                    .insert(name.to_string(), Arc::new(props.clone()));
            }
        }
        if let Some(heal) = trifecta
            .get("heal_tool")
            .filter(|value| value_truthy(value))
        {
            state.active_heal_tool_name =
                heal.get("name").and_then(Value::as_str).map(str::to_string);
            state.heal_cost_per_use_ped = heal
                .get("cost_per_use_ped")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            state.heal_reload_seconds = heal
                .get("reload_seconds")
                .and_then(Value::as_f64)
                .unwrap_or(2.5);
            state.heal_amount_min = heal.get("heal_min").and_then(Value::as_f64);
            state.heal_amount_max = heal.get("heal_max").and_then(Value::as_f64);
        }
    }

    /// Resolve a tool name to its canonical profile: the trifecta
    /// table first, then the memoised equipment-library lookup.
    fn match_weapon_profile(
        &self,
        state: &mut TrackerState,
        tool_name: &str,
    ) -> Option<(String, Arc<Value>)> {
        // The trifecta table only stores truthy props, so a hit is the
        // original's `if profile:` taken branch.
        if let Some(profile) = state.trifecta_weapon_profiles.get(tool_name) {
            return Some((tool_name.to_string(), profile.clone()));
        }

        if let Some(cached) = state.profile_match_cache.get(tool_name) {
            return cached.clone();
        }

        let resolved = (self.providers.equipment_profile_lookup)(tool_name)
            .filter(|profile| !profile.is_empty());
        let Some(profile) = resolved else {
            state
                .profile_match_cache
                .insert(tool_name.to_string(), None);
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
        state
            .profile_match_cache
            .insert(tool_name.to_string(), matched.clone());
        matched
    }

    /// Resolve (creating if first seen) the enhancer state for a
    /// matched weapon, stamping the active-weapon markers either way.
    fn ensure_weapon_state<'a>(
        &self,
        state: &'a mut TrackerState,
        tool_name: &str,
    ) -> Option<&'a mut DamageEnhancerState> {
        let Some((canonical_name, profile)) = self.match_weapon_profile(state, tool_name) else {
            state.active_weapon_state_key = None;
            state.active_weapon_observed_name = Some(tool_name.to_string());
            return None;
        };
        state
            .weapon_enhancer_states
            .entry(canonical_name.clone())
            .or_insert_with(|| DamageEnhancerState::from_props(&canonical_name, profile));
        state.active_weapon_state_key = Some(canonical_name.clone());
        state.active_weapon_observed_name = Some(tool_name.to_string());
        state.weapon_enhancer_states.get_mut(&canonical_name)
    }

    pub(super) fn current_cost_for_tool(
        &self,
        state: &mut TrackerState,
        tool_name: &str,
        inferred_cost: f64,
    ) -> f64 {
        if let Some(weapon) = self.ensure_weapon_state(state, tool_name) {
            return weapon.current_cost_ped();
        }
        if inferred_cost > 0.0 {
            return inferred_cost;
        }
        if let Some(cached) = state.static_tool_cost_cache.get(tool_name) {
            return *cached;
        }
        let cost = (self.providers.equipment_cost_lookup)(tool_name);
        state
            .static_tool_cost_cache
            .insert(tool_name.to_string(), cost);
        cost
    }
}

/// Whether a break's item name names the active weapon (either the
/// canonical or the observed hotbar spelling), compared on lowercased
/// alphanumerics in either containment direction.
pub(super) fn break_matches_active_weapon(state: &TrackerState, item_name: &str) -> bool {
    let Some(weapon) = state
        .active_weapon_state_key
        .as_ref()
        .and_then(|key| state.weapon_enhancer_states.get(key))
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
    let observed_norm = normalise(state.active_weapon_observed_name.as_deref().unwrap_or(""));
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
