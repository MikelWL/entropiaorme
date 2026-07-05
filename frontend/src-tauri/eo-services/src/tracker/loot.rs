//! Loot-group and global/HoF handlers: dedup fingerprinting, kill
//! construction from the accumulator, and notable-event correlation.

use eo_wire::normalizer::round_half_even;

use crate::bus_events::{BusEvent, GlobalPayload};
use crate::loot_filter::is_tracked_loot;
use crate::tracking_models::Kill;

use super::time::{naive_to_epoch, parse_timestamp_str, python_total_seconds};
use super::{HuntTracker, GLOBAL_CORRELATION_WINDOW_SECONDS, LOOT_DEDUP_WINDOW_SECONDS};

impl HuntTracker {
    /// Handle a loot group from chat.log: creates a Kill record. The
    /// kill is built from the accumulator, the accumulator reset, and
    /// the kill appended to the session under the guard; the kill is
    /// a detached value by then, so the persisting DB write runs
    /// after release.
    pub(super) fn on_loot(&self, event: &BusEvent) {
        let BusEvent::LootGroup(group) = event else {
            return;
        };
        let kill = {
            let mut state = self.lock_state();
            if state.accumulator.is_none() || state.session.is_none() {
                return;
            }

            let total_ped = group.total_ped;
            let now = group
                .timestamp
                .as_deref()
                .and_then(parse_timestamp_str)
                .unwrap_or_else(|| self.clock.now());
            let now_epoch = naive_to_epoch(now);

            // Loot deduplication (same fingerprint within 2s window).
            let first_item = group
                .items
                .first()
                .map(|item| item.item_name.clone())
                .unwrap_or_default();
            let fingerprint = (round_half_even(total_ped, 4), group.items.len(), first_item);
            if state.last_loot_fingerprint.as_ref() == Some(&fingerprint) {
                if let Some(last) = state.last_loot_time {
                    if python_total_seconds(now - last) < LOOT_DEDUP_WINDOW_SECONDS {
                        return;
                    }
                }
            }
            state.last_loot_fingerprint = Some(fingerprint);
            state.last_loot_time = Some(now);
            // Past the dedup guard a Kill is always recorded, so the
            // readout changes.
            state.session_dirty = true;

            let mut items = Vec::new();
            for item in &group.items {
                if is_tracked_loot(&item.item_name, &state.loot_blacklist) {
                    items.push(item.clone());
                }
            }
            let filtered_total_ped = round_half_even(
                items
                    .iter()
                    .filter(|item| !item.is_enhancer_shrapnel)
                    .map(|item| item.value_ped)
                    .sum(),
                4,
            );

            // Snapshot mob/tag from manual configuration.
            let mob_name = if state.confirmed_mob_name.is_empty() {
                "Unknown".to_string()
            } else {
                state.confirmed_mob_name.clone()
            };

            let session_id = state.session.as_ref().expect("checked above").id.clone();
            let mob_species = state.confirmed_mob_species.clone();
            let mob_maturity = state.confirmed_mob_maturity.clone();
            let accumulator = state.accumulator.as_mut().expect("checked above");
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
            state.accumulator.as_mut().expect("checked above").reset();

            // Append the finalised kill to the session; the list tail
            // doubles as the original's `_last_kill` alias.
            state
                .session
                .as_mut()
                .expect("checked above")
                .kills
                .push(kill.clone());
            kill
        };

        // Persist outside the guard: `kill` is a detached value and
        // the lock is never held across SQLite.
        self.persist_kill(&kill);
    }
    /// Handle a global/HoF event from chat.log: tags the most
    /// recently created kill (globals arrive shortly after loot). The
    /// in-memory tag lands under the guard, capturing the values the
    /// DB writes need; the UPDATE/INSERT run after release.
    pub(super) fn on_global(&self, event: &BusEvent) {
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
            let mut state = self.lock_state();
            if state.session.is_none() {
                return;
            }

            // Filter for own player.
            if self.providers.player_name.is_empty()
                || player.to_lowercase() != self.providers.player_name.to_lowercase()
            {
                return;
            }

            state.session_dirty = true;
            let session_id = state.session.as_ref().expect("checked above").id.clone();
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
            let ts = parse_timestamp_str(raw_ts)
                .map(naive_to_epoch)
                .unwrap_or_else(|| naive_to_epoch(self.clock.now()));

            // Tag the most recently created kill (staleness check:
            // within 5s). The kills tail is the original's
            // `_last_kill` alias.
            let mut kill_id: Option<String> = None;
            let mut target_is_hof = false;
            if let Some(target) = state
                .session
                .as_mut()
                .expect("checked above")
                .kills
                .last_mut()
            {
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

        let result = self.block_on(async {
            let mut tx = self.db.write().begin().await?;
            if let Some(kill_id) = &kill_id {
                sqlx::query("UPDATE kills SET is_global = 1, is_hof = ? WHERE id = ?")
                    .bind(i64::from(target_is_hof))
                    .bind(kill_id)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query(
                "INSERT INTO notable_events \
                 (session_id, kill_id, event_type, mob_or_item, value_ped, timestamp) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&session_id)
            .bind(&kill_id)
            .bind(&event_type)
            .bind(&mob_or_item)
            .bind(value_ped)
            .bind(ts)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok::<(), sqlx::Error>(())
        });
        // A persistence failure is contained like the original's
        // handler exception: the in-memory tag stands.
        let _ = result;
    }
}
