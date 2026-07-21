//! Harvesting (tree cutting) handlers: the hand-tool equip, the
//! explicit failed-swing event, and the harvest-event construction
//! shared with the loot router.
//!
//! The swing model: every swing is directly countable (a successful
//! swing arrives as its wood loot group, a failed swing as the
//! explicit "Harvest attempt failed" line), so cost is per-swing tool
//! decay times the counted swings, with no inferred-attempt heuristic.
//! A failed swing decays the tool like a successful one; the success
//! flag is stored per row, so if that assumption is ever corrected the
//! historical costs are recomputable from the recorded facts.

use crate::bus_events::BusEvent;
use crate::ped::Ped;
use crate::tracking_models::{HarvestEvent, LootItem};

use super::actor::TrackerActor;
use super::providers::{HarvestGuardrailTools, TreeSize};
use super::session::ActiveSession;
use super::time::{instant_to_epoch, parse_timestamp_instant, resolve_local};
use super::HarvestTool;

/// A live disagreement between the guardrail's loot evidence and the
/// hotbar-equipped tool. Held on the session until resolved: agreeing
/// evidence or any fresh hotbar equip (a press re-syncs the belief)
/// clears it; the overlay reads it as the wrong-tool cue.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct GuardrailMismatch {
    pub(super) expected_tool: String,
    pub(super) observed_tool: Option<String>,
    pub(super) tree_size: TreeSize,
    pub(super) at_epoch: f64,
}

/// Whether a loot item is harvesting output (the wood taxonomy: per
/// species board types plus the shavings by-product). Used to route a
/// loot group whose swing had no hotbar signal.
pub(super) fn is_harvest_loot_item(name: &str) -> bool {
    name == "Wood Shavings" || name.ends_with(" Board")
}

/// Whether a whole loot group is harvesting output: non-empty and all
/// wood (a harvest bundle never mixes wood with mob loot).
pub(super) fn is_harvest_loot_group(items: &[LootItem]) -> bool {
    !items.is_empty()
        && items
            .iter()
            .all(|item| is_harvest_loot_item(&item.item_name))
}

/// The tree size a board name testifies to: the "Short "/"Long "
/// prefixes name the short and huge trees, a bare board the long tree.
/// Non-board items (the shavings by-product) testify to nothing.
pub(super) fn tree_size_for_board(name: &str) -> Option<TreeSize> {
    if !name.ends_with(" Board") {
        return None;
    }
    if name.starts_with("Short ") {
        Some(TreeSize::Short)
    } else if name.starts_with("Long ") {
        Some(TreeSize::Huge)
    } else {
        Some(TreeSize::Long)
    }
}

/// The tree size a loot group testifies to: its board item's size (a
/// harvest bundle never carries two board types; shavings-only groups
/// testify to nothing).
pub(super) fn tree_size_for_group(items: &[LootItem]) -> Option<TreeSize> {
    items
        .iter()
        .find_map(|item| tree_size_for_board(&item.item_name))
}

impl TrackerActor {
    /// Handle a hotbar harvesting-tool equip: the tool becomes the
    /// hand item and its per-use cost prices subsequent swings. Like
    /// the heal tool, the equipped tool is hotbar-equipment state that
    /// outlives the session; the re-hydrate nudge fires only against
    /// an active session.
    pub(super) fn on_harvest_tool_changed(&mut self, event: &BusEvent) {
        let BusEvent::ActiveHarvestToolChanged(payload) = event else {
            return;
        };
        if payload.tool_name.is_empty() {
            return;
        }
        let changed = self
            .harvest_tool
            .as_ref()
            .map(|tool| tool.name != payload.tool_name)
            .unwrap_or(true);
        self.harvest_tool = Some(HarvestTool {
            name: payload.tool_name.clone(),
            cost_per_use: Ped(payload.cost_per_use_ped),
        });
        let hand_changed = !self.hand_is_harvest;
        self.hand_is_harvest = true;

        let nudge_session_id = {
            let Some(active) = self.session.active_mut() else {
                return;
            };
            active.harvest_warning_emitted = false;
            // A hotbar press re-syncs the app's belief with the game;
            // any standing wrong-tool cue is resolved by it. Clearing
            // one is a readout change, so it nudges even for a re-press
            // of the same tool.
            let cleared_mismatch = active.guardrail_mismatch.take().is_some();
            if changed || hand_changed || cleared_mismatch {
                Some(active.session.id.clone())
            } else {
                None
            }
        };
        if let Some(session_id) = nudge_session_id {
            self.emit_session_event(
                eo_wire::domain_events::TrackingReason::Updated,
                eo_wire::domain_events::TrackingStatus::Active,
                instant_to_epoch(resolve_local(self.clock.now())),
                Some(&session_id),
            );
        }
    }

    /// Handle the explicit failed-swing line: one harvest event with
    /// no loot, costed like a successful swing.
    pub(super) async fn on_harvest_fail(&mut self, event: &BusEvent) {
        let BusEvent::HarvestFail(payload) = event else {
            return;
        };
        let harvest = {
            let Self {
                session,
                harvest_tool,
                clock,
                ..
            } = &mut *self;
            let Some(active) = session.active_mut() else {
                return;
            };
            let now_epoch = parse_timestamp_instant(&payload.timestamp)
                .map(instant_to_epoch)
                .unwrap_or_else(|| instant_to_epoch(resolve_local(clock.now())));
            let (tool_name, cost) = Self::harvest_swing_cost(active, harvest_tool.as_ref());
            active.dirty = true;
            let harvest = HarvestEvent {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: active.session.id.clone(),
                timestamp: now_epoch,
                success: false,
                tool_name,
                cost_ped: cost,
                loot_total_ped: Ped::ZERO,
                loot_items: Vec::new(),
            };
            active.session.harvests.push(harvest.clone());
            harvest
        };
        self.persist_harvest(&harvest).await;
    }

    /// The swing's tool identity and cost under the guardrail: the
    /// board evidence names the tree size, the configured intent names
    /// the tool, and the attribution follows the intent even when the
    /// hotbar disagrees (a desynced hotbar belief poisons every swing
    /// until the next press; the evidence is per-swing and unforgeable).
    /// A disagreement is surfaced, never silently recorded: it sets the
    /// session's mismatch state (the overlay cue) and a one-shot
    /// session warning. Swings with no board evidence, and sizes with
    /// no configured intent, fall back to the hotbar belief.
    pub(super) fn guarded_harvest_swing_cost(
        active: &mut ActiveSession,
        guardrail: Option<&HarvestGuardrailTools>,
        harvest_tool: Option<&HarvestTool>,
        items: &[LootItem],
        at_epoch: f64,
    ) -> (Option<String>, Ped) {
        let intended = guardrail
            .zip(tree_size_for_group(items))
            .and_then(|(tools, size)| tools.for_size(size).map(|tool| (size, tool)));
        let Some((size, tool)) = intended else {
            return Self::harvest_swing_cost(active, harvest_tool);
        };
        let observed = harvest_tool.map(|equipped| equipped.name.clone());
        if observed.as_deref() == Some(tool.name.as_str()) {
            active.guardrail_mismatch = None;
        } else {
            if !active.guardrail_warning_emitted {
                active.warnings.push(format!(
                    "Harvest guardrail: {} tree loot arrived while {} was equipped; \
                     costs are attributed to {}",
                    size.as_str(),
                    observed.as_deref().unwrap_or("no tool"),
                    tool.name
                ));
                active.guardrail_warning_emitted = true;
            }
            active.guardrail_mismatch = Some(GuardrailMismatch {
                expected_tool: tool.name.clone(),
                observed_tool: observed,
                tree_size: size,
                at_epoch,
            });
        }
        (Some(tool.name.clone()), Ped(tool.cost_per_use_ped))
    }

    /// The swing's tool identity and cost: the equipped harvesting
    /// tool's per-use cost, or zero with a one-shot session warning
    /// when no harvesting tool has been equipped via the hotbar (never
    /// guess a cost).
    pub(super) fn harvest_swing_cost(
        active: &mut ActiveSession,
        harvest_tool: Option<&HarvestTool>,
    ) -> (Option<String>, Ped) {
        match harvest_tool {
            Some(tool) => (Some(tool.name.clone()), tool.cost_per_use),
            None => {
                if !active.harvest_warning_emitted {
                    active.warnings.push(
                        "Harvesting detected: no harvesting tool equipped via hotbar".to_string(),
                    );
                    active.harvest_warning_emitted = true;
                }
                (None, Ped::ZERO)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_taxonomy_matches_boards_and_shavings_only() {
        assert!(is_harvest_loot_item("Wood Shavings"));
        assert!(is_harvest_loot_item("Short Moonleaf Board"));
        assert!(is_harvest_loot_item("Long Kaisenbrandt Board"));
        assert!(!is_harvest_loot_item("Shrapnel"));
        assert!(!is_harvest_loot_item("Animal Muscle Oil"));
        assert!(!is_harvest_loot_item("Boardwalk Trophy"));
    }

    fn item(name: &str) -> LootItem {
        LootItem {
            item_name: name.to_string(),
            quantity: 1,
            value_ped: 0.01,
            is_enhancer_shrapnel: false,
        }
    }

    #[test]
    fn a_group_is_harvest_only_when_every_item_is_wood() {
        assert!(is_harvest_loot_group(&[
            item("Short Moonleaf Board"),
            item("Wood Shavings"),
        ]));
        assert!(is_harvest_loot_group(&[item("Short Moonleaf Board")]));
        assert!(!is_harvest_loot_group(&[
            item("Short Moonleaf Board"),
            item("Shrapnel"),
        ]));
        assert!(!is_harvest_loot_group(&[]));
    }
}
