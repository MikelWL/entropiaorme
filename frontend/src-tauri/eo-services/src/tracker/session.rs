//! The session lifecycle (start, stop, demo priming, config reload),
//! the `ActiveSession` typestate payload, the aggregated tracking
//! readout, and the coalesced tick flush.

use chrono::{DateTime, Utc};
use eo_wire::domain_events::{TrackingReason, TrackingStatus};
use eo_wire::normalizer::round_half_even;
use sqlx::Row;

use crate::bus_events::{BusEvent, SessionLifecyclePayload};
use crate::db::{decoded_f64, DbError};
use crate::mob_lookup_service::python_whitespace;
use crate::ped::Ped;
use crate::tracking_models::{ActiveSessionView, TrackingReadout, TrackingSession};

use super::actor::TrackerActor;
use super::combat::Accumulator;
use super::mob::{MobSelection, MobSource, TrackingMode};
use super::time::{instant_to_epoch, local_isoformat, resolve_local};
use super::weapons::WeaponRuntime;
use super::{HealTool, HuntTracker, SessionState};

/// A loot group's dedup identity: (total, item count, first item name).
pub(super) type LootFingerprint = (f64, usize, String);

/// Everything that exists exactly while a session runs. Constructed at
/// `start_session`, dropped wholesale when the session stops, so no
/// session-scoped field can leak into idle state or arrive stale in
/// the next session.
pub(super) struct ActiveSession {
    pub(super) session: TrackingSession,
    pub(super) accumulator: Accumulator,
    /// Whether an event since the last flushed tick changed the live
    /// readout (the coalesced update fires only on a real mutation).
    pub(super) dirty: bool,
    /// Heal cost accrued this session (the equipped tool's per-use
    /// cost per counted activation).
    pub(super) heal_cost: Ped,
    pub(super) heal_warning_emitted: bool,
    pub(super) warnings: Vec<String>,
    /// The mob/tag selection kills stamp from.
    pub(super) mob: MobSelection,
    /// The input mode snapshotted at session start (not live config).
    pub(super) mode: TrackingMode,
    /// The tag-mode free-text tag (empty outside tag mode).
    pub(super) tag: String,
    pub(super) last_heal_time: Option<DateTime<Utc>>,
    /// The last recorded loot group's dedup identity and instant,
    /// always stamped together.
    pub(super) last_loot: Option<(LootFingerprint, DateTime<Utc>)>,
    pub(super) trifecta_unmatched_warning_emitted: bool,
    pub(super) weapons: WeaponRuntime,
}

impl ActiveSession {
    pub(super) fn new(session: TrackingSession, mode: TrackingMode, tag: String) -> Self {
        Self {
            session,
            accumulator: Accumulator::default(),
            dirty: false,
            heal_cost: Ped::ZERO,
            heal_warning_emitted: false,
            warnings: Vec::new(),
            mob: MobSelection::Unset,
            mode,
            tag,
            last_heal_time: None,
            last_loot: None,
            trifecta_unmatched_warning_emitted: false,
            weapons: WeaponRuntime::default(),
        }
    }

    /// The mob name a kill stamps or the readout shows: the selection's
    /// display name when it is set AND non-empty (an empty manual name
    /// behaves as unset, the original's falsy check).
    pub(super) fn stamped_mob_name(&self) -> Option<&str> {
        self.mob.name().filter(|name| !name.is_empty())
    }
}

/// The in-memory half of the tracking readout, computed by the actor
/// against its owned state and returned detached. The caller finishes
/// the readout with the two session-scoped database reads, off the
/// actor's task.
pub(super) struct SessionAggregate {
    pub(super) session_id: String,
    pub(super) started_at: String,
    pub(super) elapsed: i64,
    pub(super) kill_count: i64,
    pub(super) cost: Ped,
    pub(super) returns: Ped,
    pub(super) damage_total: f64,
    pub(super) shots_total: i64,
    pub(super) crits_total: i64,
    pub(super) max_damage: f64,
    pub(super) live_weapon_damage: f64,
    pub(super) weapon_cost: Ped,
    pub(super) globals_count: i64,
    pub(super) hofs_count: i64,
    pub(super) latest_kill_loot: Option<Ped>,
    pub(super) multiplier_last: Option<f64>,
    pub(super) multiplier_avg: Option<f64>,
    pub(super) multiplier_max: Option<f64>,
    pub(super) multiplier_history: Vec<f64>,
    pub(super) cumulative_net: Vec<f64>,
    pub(super) mob_name: Option<String>,
    pub(super) mob_source: Option<MobSource>,
    pub(super) mob_entry_mode: TrackingMode,
    pub(super) warnings: Vec<String>,
}

impl TrackerActor {
    /// The in-memory aggregation over the live session: the detected
    /// tool plus the aggregate (None when idle).
    pub(super) fn aggregate(&self) -> (Option<String>, Option<SessionAggregate>) {
        let Some(active) = self.session.active() else {
            return (None, None);
        };
        let current_tool = active.weapons.hotbar_tool.clone();

        let kills = &active.session.kills;
        let mut weapon_cost: Ped = kills
            .iter()
            .flat_map(|kill| kill.tool_stats.iter())
            .map(|(_, stats)| stats.cost_per_shot * stats.shots_fired)
            .sum();
        let mut enhancer_cost: Ped = kills.iter().map(|kill| kill.enhancer_cost).sum();
        weapon_cost += active.accumulator.weapon_cost();
        enhancer_cost += active.accumulator.enhancer_cost;
        let heal_cost = active.heal_cost;
        let cost = weapon_cost + heal_cost + enhancer_cost;
        let returns: Ped = kills.iter().map(|kill| kill.loot_total_ped).sum();

        let damage_total: f64 = kills.iter().map(|kill| kill.damage_dealt).sum();
        let live_weapon_damage = damage_total + active.accumulator.damage_dealt;

        // Multipliers use kill.cost_ped (weapon cost only) per EU
        // convention; a ratio of two PED amounts is dimensionless.
        let mult_per_kill: Vec<f64> = kills
            .iter()
            .filter(|kill| kill.cost_ped.is_positive())
            .map(|kill| kill.loot_total_ped / kill.cost_ped)
            .collect();
        let multiplier_avg = if mult_per_kill.is_empty() {
            None
        } else {
            Some(mult_per_kill.iter().sum::<f64>() / mult_per_kill.len() as f64)
        };
        let multiplier_max = mult_per_kill
            .iter()
            .copied()
            .fold(None, |acc: Option<f64>, value| {
                Some(acc.map_or(value, |best| best.max(value)))
            });
        let multiplier_last = kills
            .last()
            .filter(|kill| kill.cost_ped.is_positive())
            .map(|kill| kill.loot_total_ped / kill.cost_ped);
        let multiplier_history: Vec<f64> = mult_per_kill
            .iter()
            .rev()
            .take(120)
            .rev()
            .map(|value| round_half_even(*value, 4))
            .collect();

        // Cumulative-net history (per kill), distributing the
        // session-level heal cost pro-rata across kills by their
        // weapon-cost share so the curve's final point reconciles
        // with the displayed Net stat (returns - cost).
        let per_kill_weapon: Vec<Ped> = kills
            .iter()
            .map(|kill| {
                kill.tool_stats
                    .iter()
                    .map(|(_, stats)| stats.cost_per_shot * stats.shots_fired)
                    .sum()
            })
            .collect();
        let total_weapon: Ped = per_kill_weapon.iter().copied().sum();
        let mut cumulative_net = Vec::new();
        let mut running = Ped::ZERO;
        for (kill, weapon) in kills.iter().zip(per_kill_weapon.iter()) {
            let heal_share = if total_weapon.is_positive() {
                heal_cost * (*weapon / total_weapon)
            } else {
                Ped::ZERO
            };
            running += kill.loot_total_ped - *weapon - kill.enhancer_cost - heal_share;
            cumulative_net.push(running.round_half_even(2).value());
        }
        let cumulative_net: Vec<f64> = cumulative_net
            .iter()
            .rev()
            .take(120)
            .rev()
            .copied()
            .collect();

        let start_ts = instant_to_epoch(active.session.start_time);
        let aggregate = SessionAggregate {
            session_id: active.session.id.clone(),
            started_at: local_isoformat(active.session.start_time),
            elapsed: (instant_to_epoch(resolve_local(self.clock.now())) - start_ts) as i64,
            kill_count: kills.len() as i64,
            cost,
            returns,
            damage_total,
            shots_total: kills.iter().map(|kill| kill.shots_fired).sum(),
            crits_total: kills.iter().map(|kill| kill.critical_hits).sum(),
            max_damage: kills
                .iter()
                .map(|kill| kill.damage_dealt)
                .fold(0.0, f64::max),
            live_weapon_damage,
            weapon_cost,
            globals_count: kills.iter().filter(|kill| kill.is_global).count() as i64,
            hofs_count: kills.iter().filter(|kill| kill.is_hof).count() as i64,
            latest_kill_loot: kills.last().map(|kill| kill.loot_total_ped),
            multiplier_last,
            multiplier_avg,
            multiplier_max,
            multiplier_history,
            cumulative_net,
            mob_name: active.stamped_mob_name().map(str::to_string),
            mob_source: active.mob.source(),
            mob_entry_mode: active.mode,
            warnings: active.warnings.clone(),
        };
        (current_tool, Some(aggregate))
    }

    /// Prime the tracker with a fully-formed demo session, bypassing
    /// the normal `start_session` lifecycle (no handlers subscribe,
    /// nothing persists). It exists solely for guide-mode demo playback
    /// over a throwaway database and must never run on the live
    /// tracker.
    pub(super) fn prime_demo(
        &mut self,
        session: TrackingSession,
        mob: MobSelection,
        mode: TrackingMode,
    ) {
        let mut active = ActiveSession::new(session, mode, String::new());
        active.mob = mob;
        self.session = SessionState::Active(Box::new(active));
        self.publish_status();
    }

    /// Refresh trifecta-attribution state after config changes. The
    /// providers may read the database; the actor simply runs them
    /// inline (nothing else can touch its state meanwhile).
    pub(super) fn reload_config(&mut self) {
        let trifecta_mode = self.providers.config.weapon_attribution_trifecta();
        let trifecta = if trifecta_mode {
            self.providers.equipment.resolve_trifecta()
        } else {
            None
        };
        self.refresh_loot_filter();
        let Self {
            session,
            heal_tool,
            providers,
            ..
        } = self;
        let Some(active) = session.active_mut() else {
            return;
        };
        if trifecta_mode {
            Self::load_trifecta_weapon_profiles(active, heal_tool, trifecta.as_ref());
        } else {
            active.weapons.attributor.clear();
            *heal_tool = HealTool::default();
            active.heal_warning_emitted = false;
            active.weapons.reset_runtime();
        }

        if active.mode == TrackingMode::Tag {
            return;
        }

        if providers.config.manual_mob_entry_enabled() {
            let Some((species, maturity)) = providers.config.manual_mob() else {
                if active.mob.source() == Some(MobSource::Manual) {
                    active.mob = MobSelection::Unset;
                }
                return;
            };
            active.mob = MobSelection::manual_from_parts(species, maturity);
            return;
        }

        if active.mob.source() == Some(MobSource::Manual) {
            active.mob = MobSelection::Unset;
        }
    }

    /// Start a new tracking session; any prior session stops first, so
    /// its own stop events publish cleanly before the start events.
    pub(super) async fn start_session(&mut self) -> Result<TrackingSession, DbError> {
        if self.session.active().is_some() {
            self.stop_session().await?;
        }

        let mode = TrackingMode::from_config(&self.providers.config.mob_tracking_mode());
        let tag = self
            .providers
            .config
            .mob_tracking_tag()
            .trim_matches(python_whitespace)
            .to_string();
        let session_id = uuid::Uuid::new_v4().to_string();
        let trifecta_mode = self.providers.config.weapon_attribution_trifecta();
        let trifecta = if trifecta_mode {
            self.providers.equipment.resolve_trifecta()
        } else {
            None
        };

        self.refresh_loot_filter();
        let session = TrackingSession {
            id: session_id.clone(),
            // The one wall-clock read of the start path, resolved to
            // its instant at the boundary.
            start_time: resolve_local(self.clock.now()),
            end_time: None,
            kills: Vec::new(),
            dangling_cost: Ped::ZERO,
        };
        let start_ts = instant_to_epoch(session.start_time);

        // Persist session start BEFORE activating in memory: a failed
        // insert leaves the tracker idle rather than a phantom session
        // with no row for its kills. `mob_tracking_mode` records the
        // input mode the session was captured under so post-hoc UI
        // surfaces can choose label vocabulary; the value never mutates
        // after session start.
        sqlx::query(
            "INSERT INTO tracking_sessions \
             (id, started_at, is_active, mob_tracking_mode) \
             VALUES (?, ?, 1, ?)",
        )
        .bind(&session_id)
        .bind(start_ts)
        .bind(mode.as_str())
        .execute(self.db.write())
        .await?;

        // The fresh ActiveSession IS the session reset: every
        // session-scoped field starts at its documented initial
        // state by construction. (The equipped heal tool
        // deliberately persists; it lives outside the typestate.)
        let mut active = ActiveSession::new(session.clone(), mode, tag.clone());

        if trifecta_mode {
            Self::load_trifecta_weapon_profiles(
                &mut active,
                &mut self.heal_tool,
                trifecta.as_ref(),
            );
        }

        if mode == TrackingMode::Tag && !tag.is_empty() {
            active.mob = MobSelection::Tag(tag.clone());
        } else if self.providers.config.manual_mob_entry_enabled() {
            if let Some((species, maturity)) = self.providers.config.manual_mob() {
                active.mob = MobSelection::manual_from_parts(species, maturity);
            }
        }

        self.session = SessionState::Active(Box::new(active));
        self.subscribe_handlers();
        self.publish_status();

        self.bus
            .publish(&BusEvent::SessionStarted(SessionLifecyclePayload {
                session_id: session_id.clone(),
            }));
        self.emit_session_event(
            TrackingReason::Started,
            TrackingStatus::Active,
            start_ts,
            Some(&session_id),
        );
        Ok(session)
    }

    /// Stop the active session: dangling cost, the handler
    /// unsubscribes and the end stamp; then persistence, ledger gains,
    /// summary, and the stop events; then the in-memory clear
    /// (dropping the whole `ActiveSession`).
    pub(super) async fn stop_session(&mut self) -> Result<Option<TrackingSession>, DbError> {
        let (session, session_id, end_time, heal_cost, dangling_cost) = {
            let Some(active) = self.session.active_mut() else {
                return Ok(None);
            };
            let dangling_cost = active.accumulator.total_cost();
            active.session.end_time = Some(resolve_local(self.clock.now()));
            active.session.dangling_cost = dangling_cost;
            let snapshot = active.session.clone();
            let session_id = snapshot.id.clone();
            let end_time = snapshot.end_time.expect("just stamped");
            let heal_cost = active.heal_cost;
            (snapshot, session_id, end_time, heal_cost, dangling_cost)
        };
        // One transaction over the whole stop sequence, matching the
        // original's single commit: a failure (or crash) mid-way leaves
        // no half-stopped session, no orphaned ledger gains, and no
        // summary computed from a partially persisted stop. The bus
        // forwarders stay subscribed until it commits, so a failed stop
        // leaves the session fully live (still tracking, still hearing
        // events) rather than active-but-deaf; no event can interleave
        // meanwhile because the actor processes one message at a time.
        let mut tx = self.db.write().begin().await?;
        sqlx::query(
            "UPDATE tracking_sessions SET ended_at = ?, is_active = 0, \
             heal_cost = ?, dangling_cost = ? WHERE id = ?",
        )
        .bind(instant_to_epoch(end_time))
        .bind(heal_cost.value())
        .bind(dangling_cost.value())
        .bind(&session_id)
        .execute(&mut *tx)
        .await?;
        // Auto-generate ledger gains derived from persisted loot rows.
        Self::create_enhancer_rebate_ledger_entry(&mut tx, &session_id, end_time).await?;
        Self::create_shrapnel_ledger_entry(&mut tx, &session_id, end_time).await?;
        crate::session_summary::write_session_summary(&mut tx, &session_id).await?;
        crate::daily_rollup::refresh_session_days(&mut tx, &session_id).await?;
        tx.commit().await?;
        self.unsubscribe_handlers();

        // Session end is a quiescent boundary: checkpoint and truncate the WAL
        // so its growth over a tracked session is bounded. Best-effort: the
        // stop's data is already committed, and TRUNCATE can be briefly blocked
        // by an in-flight reader (it simply retries at the next session end), so
        // a failure here must not fail the stop. A failure is logged rather than
        // swallowed, so a persistently failing checkpoint (a stuck reader) leaves
        // a diagnostic trail instead of silently unbounded WAL growth.
        if let Err(error) = self.db.checkpoint_truncate().await {
            tracing::warn!(
                target: "eo::tracker",
                %session_id,
                %error,
                "WAL checkpoint at session end failed; log growth is not bounded this stop",
            );
        }

        self.bus
            .publish(&BusEvent::SessionStopped(SessionLifecyclePayload {
                session_id: session_id.clone(),
            }));
        // `end_time` was stamped from the injected clock above, so the
        // required `occurred_at` always carries the stop instant.
        self.emit_session_event(
            TrackingReason::Stopped,
            TrackingStatus::Idle,
            instant_to_epoch(end_time),
            Some(&session_id),
        );

        // Dropping the ActiveSession IS the in-memory clear: the
        // accumulator, weapon runtime, and mob selection cannot
        // survive the session because they live inside it. (The
        // equipped heal tool deliberately does.)
        self.session = SessionState::Idle;
        self.publish_status();
        Ok(Some(session))
    }

    /// Coalesce a settled tick's mutations into one domain event.
    /// Subscribed only while a session is active; fires only when the
    /// tick actually changed the live session readout, stamped with
    /// the tick's own timestamp (already on the tick's loot/combat
    /// events) or the injected clock when the tick carries none.
    pub(super) fn on_tick_flushed(&mut self, event: &BusEvent) {
        let BusEvent::TickFlushed(payload) = event else {
            return;
        };
        let session_id = {
            let Some(active) = self.session.active_mut() else {
                return;
            };
            if !active.dirty {
                return;
            }
            active.dirty = false;
            active.session.id.clone()
        };
        // The original's three-way stamp: a datetime-equivalent string
        // takes its instant, an epoch-float string goes through
        // `float()` (an unparseable value raises there, contained with
        // the dirty flag already consumed: no event), and an absent
        // timestamp falls back to the injected clock.
        let occurred_ts = match &payload.timestamp {
            None => instant_to_epoch(resolve_local(self.clock.now())),
            Some(text) => match super::time::parse_timestamp_instant(text) {
                Some(instant) => instant_to_epoch(instant),
                None => match text.trim().parse::<f64>() {
                    Ok(numeric) => numeric,
                    Err(_) => return,
                },
            },
        };
        self.emit_session_event(
            TrackingReason::Updated,
            TrackingStatus::Active,
            occurred_ts,
            Some(&session_id),
        );
    }
}

impl HuntTracker {
    /// An owned, immutable view of the current tracking readout:
    /// `active` is None when idle. The in-memory aggregation runs on
    /// the actor; the two session-scoped reads (skill-gain total,
    /// notable-event feed) run here, keyed on the captured session id.
    pub async fn snapshot(&self) -> Result<TrackingReadout, DbError> {
        let (current_tool, aggregate) = self.aggregate().await;
        let Some(aggregated) = aggregate else {
            return Ok(TrackingReadout {
                current_tool,
                active: None,
            });
        };

        let skill_row =
            sqlx::query("SELECT COALESCE(SUM(ped_value), 0) FROM skill_gains WHERE session_id = ?")
                .bind(&aggregated.session_id)
                .fetch_one(self.db.read())
                .await?;
        let skill_tt = decoded_f64(&skill_row, 0);

        // Latest-session notable-event feed (top 20). The live
        // session is the latest session, so this single read
        // serves the activity feed.
        let rows = sqlx::query(
            "SELECT event_type, mob_or_item, value_ped, timestamp \
             FROM notable_events WHERE session_id = ? \
             ORDER BY timestamp DESC LIMIT 20",
        )
        .bind(&aggregated.session_id)
        .fetch_all(self.db.read())
        .await?;
        let mut notable_rows = Vec::new();
        for row in rows {
            notable_rows.push((
                row.try_get::<String, _>(0)?,
                row.try_get::<String, _>(1)?,
                decoded_f64(&row, 2),
                row.try_get::<Option<f64>, _>(3)?,
            ));
        }

        let round_opt =
            |value: Option<f64>, places: usize| value.map(|inner| round_half_even(inner, places));
        let active = ActiveSessionView {
            session_id: aggregated.session_id,
            started_at: aggregated.started_at,
            kill_count: aggregated.kill_count,
            elapsed: aggregated.elapsed,
            cost: aggregated.cost.round_half_even(2).value(),
            returns: aggregated.returns.round_half_even(2).value(),
            pes: round_half_even(skill_tt, 2),
            net: (aggregated.returns - aggregated.cost)
                .round_half_even(2)
                .value(),
            return_rate: if aggregated.cost.is_positive() {
                round_half_even(aggregated.returns / aggregated.cost, 4)
            } else {
                0.0
            },
            damage_dealt_total: round_half_even(aggregated.damage_total, 1),
            weapon_damage_dealt: round_half_even(aggregated.live_weapon_damage, 1),
            weapon_cost: aggregated.weapon_cost.round_half_even(6).value(),
            shots_fired_total: aggregated.shots_total,
            critical_hits_total: aggregated.crits_total,
            max_damage: round_half_even(aggregated.max_damage, 1),
            globals_count: aggregated.globals_count,
            hofs_count: aggregated.hofs_count,
            latest_kill_loot: aggregated
                .latest_kill_loot
                .map(|loot| loot.round_half_even(2).value()),
            multiplier_last: round_opt(aggregated.multiplier_last, 4),
            multiplier_avg: round_opt(aggregated.multiplier_avg, 4),
            multiplier_max: round_opt(aggregated.multiplier_max, 4),
            multiplier_history: aggregated.multiplier_history,
            cumulative_net_history: aggregated.cumulative_net,
            current_mob: aggregated.mob_name.clone(),
            mob_source: if aggregated.mob_name.is_some() {
                aggregated
                    .mob_source
                    .map(|source| source.as_str().to_string())
            } else {
                None
            },
            mob_entry_mode: aggregated.mob_entry_mode.as_str().to_string(),
            notable_event_rows: notable_rows,
            warnings: aggregated.warnings,
        };
        Ok(TrackingReadout {
            current_tool,
            active: Some(active),
        })
    }
}
