//! Combat-stream handlers: shot recording with tool attribution and
//! cost phases, the per-kill accumulator, heal-tick dedup, hotbar
//! tool/heal-tool changes, and enhancer breaks.

use eo_wire::domain_events::{TrackingReason, TrackingStatus};

use crate::bus_events::{BusEvent, CombatPayload};
use crate::ped::Ped;
use crate::tracking_models::ToolStats;

use super::actor::TrackerActor;
use super::providers::Providers;
use super::session::ActiveSession;
use super::time::{naive_to_epoch, parse_timestamp_str, python_total_seconds};
use super::weapons::break_matches_active_weapon;
use super::HealTool;

/// Combat stats since the last kill (or session start).
#[derive(Default)]
pub(super) struct Accumulator {
    pub(super) shots_fired: i64,
    pub(super) damage_dealt: f64,
    pub(super) damage_taken: f64,
    pub(super) critical_hits: i64,
    pub(super) enhancer_cost: Ped,
    /// Keyed by phase key (the bare tool name, then `name#2`...), in
    /// first-seen order.
    pub(super) tool_stats: Vec<(String, ToolStats)>,
}

impl Accumulator {
    pub(super) fn reset(&mut self) {
        *self = Accumulator::default();
    }

    pub(super) fn weapon_cost(&self) -> Ped {
        self.tool_stats
            .iter()
            .map(|(_, stats)| stats.cost_per_shot * stats.shots_fired)
            .sum()
    }

    pub(super) fn total_cost(&self) -> Ped {
        self.weapon_cost() + self.enhancer_cost
    }
}

impl TrackerActor {
    /// The accumulator's stats entry for this tool at this cost: an
    /// existing phase within the cost tolerance, or a new phase keyed
    /// `name`, then `name#2`...
    pub(super) fn tool_stats_for_phase<'a>(
        accumulator: &'a mut Accumulator,
        tool_name: &str,
        cost_per_shot: Ped,
    ) -> &'a mut ToolStats {
        if let Some(index) = accumulator.tool_stats.iter().position(|(_, stats)| {
            stats.tool_name == tool_name
                && (stats.cost_per_shot.value() - cost_per_shot.value()).abs() < 1e-9
        }) {
            return &mut accumulator.tool_stats[index].1;
        }
        let phase_count = accumulator
            .tool_stats
            .iter()
            .filter(|(_, stats)| stats.tool_name == tool_name)
            .count();
        let key = if phase_count == 0 {
            tool_name.to_string()
        } else {
            format!("{tool_name}#{}", phase_count + 1)
        };
        accumulator
            .tool_stats
            .push((key, ToolStats::new(tool_name, cost_per_shot)));
        &mut accumulator.tool_stats.last_mut().expect("just pushed").1
    }

    /// Accumulate one player attack, including jam/dodge/evade
    /// countered shots.
    fn record_offensive_shot(
        providers: &Providers,
        active: &mut ActiveSession,
        amount: f64,
        is_crit: bool,
        allow_damage_inference: bool,
    ) {
        active.accumulator.shots_fired += 1;
        if amount > 0.0 {
            active.accumulator.damage_dealt += amount;
        }
        if is_crit {
            active.accumulator.critical_hits += 1;
        }

        let mut inferred_cost = Ped::ZERO;
        let mut tool: Option<String> = None;
        if providers.config.weapon_attribution_trifecta() {
            if allow_damage_inference {
                let attribution = active.weapons.attributor.match_damage(amount, is_crit);
                if attribution.is_none() && !active.trifecta_unmatched_warning_emitted {
                    active.warnings.push(
                        "Trifecta attribution: damage fell outside both weapon ranges".to_string(),
                    );
                    active.trifecta_unmatched_warning_emitted = true;
                }
                if let Some(attribution) = attribution {
                    tool = Some(attribution.tool_name);
                    inferred_cost = Ped(attribution.cost_per_shot);
                }
            } else {
                tool = active.weapons.last_offensive_tool.clone();
            }
        } else {
            tool = active.weapons.hotbar_tool.clone();
        }

        if let Some(tool) = &tool {
            active.weapons.last_offensive_tool = Some(tool.clone());
        }

        // `tool or "Unknown"`: the falsy coercion, so an empty name
        // also keys the fallback entry.
        let tool_key = tool
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or("Unknown")
            .to_string();
        let mut current_cost = Ped::ZERO;
        if let Some(tool) = &tool {
            current_cost =
                Self::current_cost_for_tool(providers, &mut active.weapons, tool, inferred_cost);
        }

        let stats: &mut ToolStats = if let (Some(tool), true) = (&tool, current_cost.is_positive())
        {
            Self::tool_stats_for_phase(&mut active.accumulator, tool, current_cost)
        } else {
            let accumulator = &mut active.accumulator;
            if !accumulator
                .tool_stats
                .iter()
                .any(|(key, _)| key == &tool_key)
            {
                accumulator
                    .tool_stats
                    .push((tool_key.clone(), ToolStats::new(&tool_key, Ped::ZERO)));
            }
            let index = accumulator
                .tool_stats
                .iter()
                .position(|(key, _)| key == &tool_key)
                .expect("just ensured");
            let entry = &mut accumulator.tool_stats[index].1;
            // The fallback cost resolves only for a still-costless
            // entry, so the provider is not re-read on every shot.
            if !entry.cost_per_shot.is_positive() {
                let fallback_cost = if inferred_cost.is_positive() {
                    inferred_cost
                } else {
                    Ped(providers.equipment.cost_per_shot(&tool_key))
                };
                if fallback_cost.is_positive() {
                    entry.cost_per_shot = fallback_cost;
                }
            }
            entry
        };
        stats.shots_fired += 1;
        if amount > 0.0 {
            stats.damage_dealt += amount;
        }
        if is_crit {
            stats.critical_hits += 1;
        }
    }

    /// Handle a parsed combat event from chat.log. The whole body
    /// mutates owned in-memory state, so it runs under the guard;
    /// there is no DB write or publish. Defensive incoming events
    /// stay out of the kills model.
    pub(super) fn on_combat(&mut self, event: &BusEvent) {
        let BusEvent::Combat(payload) = event else {
            return;
        };
        let Self {
            session,
            heal_tool,
            providers,
            ..
        } = self;
        let Some(active) = session.active_mut() else {
            return;
        };

        // Whether this event actually changed the live session
        // readout: the coalesced tracking.session.updated fires only
        // on a real mutation, so a duplicate self-heal tick or an
        // unhandled combat kind does not wake listeners for a no-op.
        let mut mutated = false;

        match payload {
            CombatPayload::DamageDealt { amount, .. } => {
                Self::record_offensive_shot(providers, active, *amount, false, true);
                mutated = true;
            }
            CombatPayload::CriticalHit { amount, .. } => {
                Self::record_offensive_shot(providers, active, *amount, true, true);
                mutated = true;
            }
            CombatPayload::TargetDodge { .. }
            | CombatPayload::TargetEvade { .. }
            | CombatPayload::TargetJam { .. } => {
                Self::record_offensive_shot(providers, active, 0.0, false, false);
                mutated = true;
            }
            CombatPayload::DamageReceived { amount, .. } => {
                active.accumulator.damage_taken += amount;
                mutated = true;
            }
            CombatPayload::SelfHeal { amount, timestamp } => {
                // Deduplicate: tool activations produce multiple heal
                // ticks in chat.log. Use the tool's reload time as the
                // dedup window.
                if let Some(timestamp) = parse_timestamp_str(timestamp) {
                    let is_new_heal_activation = match active.last_heal_time {
                        None => true,
                        Some(last) => {
                            python_total_seconds(timestamp - last) >= heal_tool.reload_seconds
                        }
                    };
                    if is_new_heal_activation {
                        if providers.config.weapon_attribution_trifecta()
                            && !heal_amount_matches_trifecta_tool(heal_tool, *amount)
                        {
                            return;
                        }
                        if heal_tool.name.is_none() && !active.heal_warning_emitted {
                            active.warnings.push(
                                "Healing detected: no heal tool equipped via hotbar".to_string(),
                            );
                            active.heal_warning_emitted = true;
                        }
                        if heal_tool.cost_per_use.is_positive() {
                            active.heal_cost += heal_tool.cost_per_use;
                        }
                        active.last_heal_time = Some(timestamp);
                        mutated = true;
                    }
                }
            }
            // The player-defence kinds are parsed and recorded on the
            // stream but do not move the session model, as before.
            CombatPayload::PlayerDodge { .. }
            | CombatPayload::PlayerEvade { .. }
            | CombatPayload::PlayerJam { .. }
            | CombatPayload::MobMiss { .. }
            | CombatPayload::Deflect { .. } => {}
        }

        if mutated {
            active.dirty = true;
        }
    }

    /// Handle hotbar-driven weapon tool change: merges any 'Unknown'
    /// tool stats into the real tool when first detected.
    pub(super) fn on_tool_changed(&mut self, event: &BusEvent) {
        let BusEvent::ActiveToolChanged(payload) = event else {
            return;
        };
        let nudge_session_id = {
            let Self {
                session, providers, ..
            } = &mut *self;
            if providers.config.weapon_attribution_trifecta() {
                return;
            }
            if payload.tool_name.is_empty() {
                return;
            }
            let Some(active) = session.active_mut() else {
                return;
            };
            let tool_name = payload.tool_name.clone();
            let tool_changed = active.weapons.hotbar_tool.as_deref() != Some(tool_name.as_str());
            active.weapons.hotbar_tool = Some(tool_name.clone());

            let current_cost =
                Self::current_cost_for_tool(providers, &mut active.weapons, &tool_name, Ped::ZERO);

            // Merge "Unknown" stats into the real tool on first
            // identification.
            let unknown = {
                let accumulator = &mut active.accumulator;
                accumulator
                    .tool_stats
                    .iter()
                    .position(|(key, _)| key == "Unknown")
                    .map(|index| accumulator.tool_stats.remove(index).1)
            };
            if let Some(unknown) = unknown {
                let real: &mut ToolStats = if current_cost.is_positive() {
                    Self::tool_stats_for_phase(&mut active.accumulator, &tool_name, current_cost)
                } else {
                    let accumulator = &mut active.accumulator;
                    if !accumulator
                        .tool_stats
                        .iter()
                        .any(|(key, _)| key == &tool_name)
                    {
                        accumulator
                            .tool_stats
                            .push((tool_name.clone(), ToolStats::new(&tool_name, Ped::ZERO)));
                    }
                    let index = accumulator
                        .tool_stats
                        .iter()
                        .position(|(key, _)| key == &tool_name)
                        .expect("just ensured");
                    &mut accumulator.tool_stats[index].1
                };
                real.shots_fired += unknown.shots_fired;
                real.damage_dealt += unknown.damage_dealt;
                real.critical_hits += unknown.critical_hits;
            }

            // A hotbar weapon-switch changes the overlay's active-weapon
            // readout. The coalesced session-update tick only flushes on
            // chat-log activity (the first attack), so a switch with no
            // combat would leave the overlay stale; emit a re-hydrate
            // nudge directly when the weapon actually changed during an
            // active session. The active tool is already in the snapshot,
            // so no new event or payload is needed. ActiveToolChanged
            // carries no instant, so the nudge is stamped from the
            // injected clock (matching the tick handler's fallback).
            if tool_changed {
                Some(active.session.id.clone())
            } else {
                None
            }
        };

        if let Some(session_id) = nudge_session_id {
            self.emit_session_event(
                TrackingReason::Updated,
                TrackingStatus::Active,
                naive_to_epoch(self.clock.now()),
                Some(&session_id),
            );
        }
    }

    /// Handle hotbar-driven heal tool equip. The equipped tool is
    /// hotbar-equipment state (it outlives the session); the
    /// re-hydrate nudge fires only against an active session.
    pub(super) fn on_heal_tool_changed(&mut self, event: &BusEvent) {
        let BusEvent::ActiveHealToolChanged(payload) = event else {
            return;
        };
        if self.providers.config.weapon_attribution_trifecta() {
            return;
        }
        let name = Some(payload.tool_name.clone());

        let nudge_session_id = {
            let heal_tool_changed = self.heal_tool.name != name;
            self.heal_tool = HealTool {
                name,
                cost_per_use: Ped(payload.cost_per_use_ped),
                reload_seconds: payload.reload_seconds,
                amount_min: None,
                amount_max: None,
            };
            let Some(active) = self.session.active_mut() else {
                return;
            };
            active.heal_warning_emitted = false;
            // Equipping a different heal tool changes the overlay readout;
            // emit a direct re-hydrate nudge (mirrors the weapon path).
            if heal_tool_changed {
                Some(active.session.id.clone())
            } else {
                None
            }
        };

        if let Some(session_id) = nudge_session_id {
            self.emit_session_event(
                TrackingReason::Updated,
                TrackingStatus::Active,
                naive_to_epoch(self.clock.now()),
                Some(&session_id),
            );
        }
    }

    /// Handle an enhancer break event: update enhancer state for
    /// future shots. There is no DB write or publish.
    pub(super) fn on_enhancer_break(&mut self, event: &BusEvent) {
        let BusEvent::EnhancerBreak(payload) = event else {
            return;
        };
        let Some(active) = self.session.active_mut() else {
            return;
        };

        let enhancer_name = payload.enhancer_name.as_str();
        let item_name = payload.item_name.as_str();
        // The payload's break count drives the stack update directly
        // (the parser guarantees an integer; the old missing-count
        // decrement-one fallback had no producer).
        let remaining = Some(payload.remaining);

        let applies = {
            let weapon = active
                .weapons
                .active_key
                .as_ref()
                .and_then(|key| active.weapons.enhancer_states.get(key));
            match weapon {
                Some(weapon) => {
                    !weapon.stacks.is_empty()
                        && enhancer_name.to_lowercase().contains("damage")
                        && break_matches_active_weapon(&active.weapons, item_name)
                }
                None => false,
            }
        };
        if !applies {
            return;
        }

        // The break applies to the active weapon, so the readout
        // reflects it; an ignored break (filtered out above) leaves
        // the session unchanged.
        active.dirty = true;
        let key = active.weapons.active_key.clone().expect("checked above");
        active
            .weapons
            .enhancer_states
            .get_mut(&key)
            .expect("checked above")
            .apply_break(remaining);
    }
}

/// Trifecta direct-heal attribution uses the configured heal
/// interval.
fn heal_amount_matches_trifecta_tool(heal_tool: &HealTool, amount: f64) -> bool {
    match (heal_tool.amount_min, heal_tool.amount_max) {
        (Some(min), Some(max)) => min <= amount && amount <= max,
        _ => true,
    }
}
