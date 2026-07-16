//! Loot-group and global/HoF handlers: dedup fingerprinting, kill
//! construction from the accumulator, and notable-event correlation.

use eo_wire::normalizer::round_half_even;

use crate::bus_events::{BusEvent, GlobalPayload};
use crate::loot_filter::is_tracked_loot;
use crate::ped::Ped;
use crate::tracking_models::{HarvestEvent, Kill};

use super::actor::TrackerActor;
use super::harvest::is_harvest_loot_group;
use super::time::{instant_to_epoch, parse_timestamp_instant, python_total_seconds, resolve_local};
use super::{GLOBAL_CORRELATION_WINDOW_SECONDS, LOOT_DEDUP_WINDOW_SECONDS};

/// Where one loot group lands: a mob kill, or a harvesting swing.
enum RoutedLoot {
    Kill(Kill),
    Harvest(HarvestEvent),
}

impl TrackerActor {
    /// Handle a loot group from chat.log. A wood group (the harvest
    /// taxonomy) records a harvesting swing; anything else creates a
    /// Kill record from the accumulator, which then resets. Either
    /// record is a detached value by the end of the guard block, so
    /// the persisting DB write runs after release.
    pub(super) async fn on_loot(&mut self, event: &BusEvent) {
        let BusEvent::LootGroup(group) = event else {
            return;
        };
        let routed = {
            let Self {
                session,
                loot_blacklist,
                harvest_tool,
                clock,
                ..
            } = &mut *self;
            let Some(active) = session.active_mut() else {
                return;
            };

            let total_ped = group.total_ped;
            let now = group
                .timestamp
                .as_deref()
                .and_then(parse_timestamp_instant)
                .unwrap_or_else(|| resolve_local(clock.now()));
            let now_epoch = instant_to_epoch(now);

            // Loot deduplication (same fingerprint within 2s window).
            let first_item = group
                .items
                .first()
                .map(|item| item.item_name.clone())
                .unwrap_or_default();
            let fingerprint = (round_half_even(total_ped, 4), group.items.len(), first_item);
            if let Some((last_fingerprint, last_time)) = &active.last_loot {
                if *last_fingerprint == fingerprint
                    && python_total_seconds(now - *last_time) < LOOT_DEDUP_WINDOW_SECONDS
                {
                    return;
                }
            }
            active.last_loot = Some((fingerprint, now));
            // Past the dedup guard a Kill is always recorded, so the
            // readout changes.
            active.dirty = true;

            let mut items = Vec::new();
            for item in &group.items {
                if is_tracked_loot(&item.item_name, loot_blacklist) {
                    items.push(item.clone());
                }
            }
            let filtered_total_ped = Ped(round_half_even(
                items
                    .iter()
                    .filter(|item| !item.is_enhancer_shrapnel)
                    .map(|item| item.value_ped)
                    .sum(),
                4,
            ));

            // A wood group is a harvesting swing, not a kill. The
            // combat accumulator is untouched: pending shots stay
            // pending toward the next kill (or dangling cost).
            if is_harvest_loot_group(&items) {
                let (tool_name, cost) = Self::harvest_swing_cost(active, harvest_tool.as_ref());
                let harvest = HarvestEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: active.session.id.clone(),
                    timestamp: now_epoch,
                    success: true,
                    tool_name,
                    cost_ped: cost,
                    loot_total_ped: filtered_total_ped,
                    loot_items: items,
                };
                active.session.harvests.push(harvest.clone());
                RoutedLoot::Harvest(harvest)
            } else {
                // Snapshot the mob/tag stamp from the selection (the
                // variant carries the source, so the stamp cannot drift
                // from where it came from).
                let mob_name = active.stamped_mob_name().unwrap_or("Unknown").to_string();
                let (mob_species, mob_maturity) = active.mob.species_maturity();
                let (mob_species, mob_maturity) =
                    (mob_species.to_string(), mob_maturity.to_string());

                let session_id = active.session.id.clone();
                let accumulator = &mut active.accumulator;
                let kill = Kill {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    mob_name,
                    mob_species,
                    mob_maturity,
                    timestamp: now_epoch,
                    shots_fired: accumulator.shots_fired,
                    damage_dealt: accumulator.damage_dealt,
                    damage_taken: accumulator.damage_taken,
                    critical_hits: accumulator.critical_hits,
                    cost_ped: accumulator.weapon_cost(),
                    enhancer_cost: accumulator.enhancer_cost,
                    loot_total_ped: filtered_total_ped,
                    loot_items: items,
                    tool_stats: std::mem::take(&mut accumulator.tool_stats),
                    is_global: false,
                    is_hof: false,
                };

                // Reset accumulator for next kill (tool_stats moved into
                // the kill above, exactly the original's shallow copy
                // followed by a fresh dict).
                accumulator.reset();

                // Append the finalised kill to the session; the list tail
                // doubles as the original's `_last_kill` alias.
                active.session.kills.push(kill.clone());
                RoutedLoot::Kill(kill)
            }
        };

        // The routed record is a detached value by here; the borrow of
        // the live session ended with the block above.
        match routed {
            RoutedLoot::Kill(kill) => self.persist_kill(&kill).await,
            RoutedLoot::Harvest(harvest) => self.persist_harvest(&harvest).await,
        }
    }

    /// Handle a global/HoF event from chat.log: tags the most
    /// recently created kill (globals arrive shortly after loot). The
    /// in-memory tag lands under the guard, capturing the values the
    /// DB writes need; the UPDATE/INSERT run after release.
    pub(super) async fn on_global(&mut self, event: &BusEvent) {
        let BusEvent::Global(payload) = event else {
            return;
        };
        let (event_type, player, subject, raw_value, raw_ts) = match payload {
            GlobalPayload::GlobalKill {
                timestamp,
                player,
                creature,
                value,
            } => ("global_kill", player, creature, *value, timestamp),
            GlobalPayload::HofKill {
                timestamp,
                player,
                creature,
                value,
            } => ("hof_kill", player, creature, *value, timestamp),
            GlobalPayload::GlobalItem {
                timestamp,
                player,
                item,
                value,
            } => ("global_item", player, item, *value, timestamp),
            GlobalPayload::HofItem {
                timestamp,
                player,
                item,
                value,
            } => ("hof_item", player, item, *value, timestamp),
        };
        let (session_id, kill_id, target_is_hof, event_type, mob_or_item, value_ped, ts) = {
            let Self {
                session,
                providers,
                clock,
                ..
            } = &mut *self;
            let Some(active) = session.active_mut() else {
                return;
            };

            // Filter for own player.
            if providers.player_name.is_empty()
                || player.to_lowercase() != providers.player_name.to_lowercase()
            {
                return;
            }

            active.dirty = true;
            let session_id = active.session.id.clone();
            let event_type = event_type.to_string();
            // The original's falsy chain on creature/item: an empty
            // subject falls through to "Unknown".
            let mob_or_item = if subject.is_empty() {
                "Unknown".to_string()
            } else {
                subject.clone()
            };
            let value_ped = raw_value;
            let is_hof = matches!(event_type.as_str(), "hof_kill" | "hof_item");
            let ts = parse_timestamp_instant(raw_ts)
                .map(instant_to_epoch)
                .unwrap_or_else(|| instant_to_epoch(resolve_local(clock.now())));

            // Tag the most recently created kill (staleness check:
            // within 5s). The kills tail is the original's
            // `_last_kill` alias.
            let mut kill_id: Option<String> = None;
            let mut target_is_hof = false;
            if let Some(target) = active.session.kills.last_mut() {
                if (ts - target.timestamp).abs() < GLOBAL_CORRELATION_WINDOW_SECONDS {
                    target.is_global = true;
                    if is_hof {
                        target.is_hof = true;
                    }
                    kill_id = Some(target.id.clone());
                    target_is_hof = target.is_hof;
                }
            }
            (
                session_id,
                kill_id,
                target_is_hof,
                event_type,
                mob_or_item,
                value_ped,
                ts,
            )
        };

        let result = self
            .db
            .with_writer(move |conn| {
                let tx = conn.transaction()?;
                if let Some(kill_id) = &kill_id {
                    tx.execute(
                        "UPDATE kills SET is_global = 1, is_hof = ? WHERE id = ?",
                        rusqlite::params![i64::from(target_is_hof), kill_id],
                    )?;
                }
                tx.execute(
                    "INSERT INTO notable_events \
                     (session_id, kill_id, event_type, mob_or_item, value_ped, timestamp) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    rusqlite::params![session_id, kill_id, event_type, mob_or_item, value_ped, ts],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await;
        // A persistence failure is contained like the original's
        // handler exception: the in-memory tag stands.
        let _ = result;
    }
}
