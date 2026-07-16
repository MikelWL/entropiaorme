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
use super::session::ActiveSession;
use super::time::{instant_to_epoch, parse_timestamp_instant, resolve_local};
use super::HarvestTool;

/// Whether a loot item is harvesting output (the wood taxonomy: per
/// species board types plus the shavings by-product). Used to route a
/// loot group whose swing had no hotbar signal.
pub(super) fn is_harvest_loot_item(name: &str) -> bool {
    name == "Wood Shavings" || name.ends_with(" Board")
}

/// Whether a whole loot group is harvesting output: non-empty and all
/// wood (a harvest bundle never mixes wood with mob loot).
pub(super) fn is_harvest_loot_group(items: &[LootItem]) -> bool {
    !items.is_empty() && items.iter().all(|item| is_harvest_loot_item(&item.item_name))
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
            if changed || hand_changed {
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
