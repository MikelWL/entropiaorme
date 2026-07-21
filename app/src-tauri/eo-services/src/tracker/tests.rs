use chrono::NaiveDateTime;
use serde_json::Value;

use crate::bus_events::BusEvent;
use crate::event_bus::Topic;

use super::actor::TrackerActor;
use super::mob::MobSource;
use super::time::{epoch_to_naive, parse_bus_timestamp, python_total_seconds};
use super::weapons::{value_truthy, DamageEnhancerState};
use super::*;
use crate::bus_events::{
    ActiveHealToolChangedPayload, ActiveToolChangedPayload, EnhancerBreakPayload, EnhancerBreakTag,
    LootGroupPayload, LootItem, LootTag, TickFlushedPayload,
};
use crate::bus_events::{CombatPayload, GlobalPayload};
use crate::clock::MockClock;
use crate::cost_engine::cost_per_shot_from_props;
use crate::ped::Ped;
use crate::time::{epoch_to_parts, naive_isoformat, naive_to_epoch, to_iso_utc};
use serde_json::json;
use std::sync::Mutex as StdMutex;

type CostScript = Arc<dyn Fn(&str) -> f64 + Send + Sync>;
type ProfileScript = Arc<dyn Fn(&str) -> EquipmentProfile + Send + Sync>;
type TrifectaScript = Arc<dyn Fn() -> Option<serde_json::Map<String, Value>> + Send + Sync>;
type BoolScript = Arc<dyn Fn() -> bool + Send + Sync>;
type ManualMobScript = Arc<dyn Fn() -> Option<(String, String)> + Send + Sync>;

/// Closure-scripted equipment library for tests.
#[derive(Default)]
struct ScriptedEquipment {
    cost: Option<CostScript>,
    profile: Option<ProfileScript>,
    trifecta: Option<TrifectaScript>,
    harvest_guardrail: Option<HarvestGuardrailTools>,
}

impl EquipmentLibrary for ScriptedEquipment {
    fn weapon_profile(&self, tool_name: &str) -> EquipmentProfile {
        self.profile.as_ref().and_then(|lookup| lookup(tool_name))
    }

    fn cost_per_shot(&self, tool_name: &str) -> f64 {
        self.cost
            .as_ref()
            .map(|lookup| lookup(tool_name))
            .unwrap_or(0.0)
    }

    fn resolve_trifecta(&self) -> Option<serde_json::Map<String, Value>> {
        self.trifecta.as_ref().and_then(|resolve| resolve())
    }

    fn resolve_harvest_guardrail(&self) -> Option<HarvestGuardrailTools> {
        self.harvest_guardrail.clone()
    }
}

/// Closure-scripted session-capture config for tests; unset fields
/// fall back to the inert defaults.
#[derive(Default)]
struct ScriptedConfig {
    mode: Option<String>,
    tag: Option<String>,
    manual_entry_enabled: Option<BoolScript>,
    manual_mob: Option<ManualMobScript>,
    trifecta_mode: bool,
    blacklist: Vec<String>,
}

impl TrackingConfig for ScriptedConfig {
    fn mob_tracking_mode(&self) -> String {
        self.mode.clone().unwrap_or_else(|| "mob".to_string())
    }

    fn mob_tracking_tag(&self) -> String {
        self.tag.clone().unwrap_or_default()
    }

    fn manual_mob_entry_enabled(&self) -> bool {
        self.manual_entry_enabled.as_ref().is_none_or(|f| f())
    }

    fn manual_mob(&self) -> Option<(String, String)> {
        self.manual_mob.as_ref().and_then(|f| f())
    }

    fn weapon_attribution_trifecta(&self) -> bool {
        self.trifecta_mode
    }

    fn loot_filter_blacklist(&self) -> Vec<String> {
        self.blacklist.clone()
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    bus: Arc<EventBus>,
    clock: Arc<MockClock>,
    db: Db,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let db = runtime
        .block_on(Db::open(&dir.path().join("entropia_orme.db")))
        .unwrap();
    Rig {
        _dir: dir,
        runtime,
        bus: Arc::new(EventBus::new()),
        clock: Arc::new(MockClock::new(None, 0.0)),
        db,
    }
}

impl Rig {
    fn tracker(&self, providers: Providers) -> Arc<HuntTracker> {
        self.runtime
            .block_on(HuntTracker::new(
                self.bus.clone(),
                self.db.clone(),
                self.clock.clone(),
                providers,
            ))
            .unwrap()
    }

    /// Drive one tracker command (or any future) to completion on the
    /// rig's runtime.
    fn wait<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// Structural probe against the actor's owned state.
    fn probe<R, F>(&self, tracker: &HuntTracker, probe: F) -> R
    where
        R: Send + 'static,
        F: FnOnce(&mut super::actor::TrackerActor) -> R + Send + 'static,
    {
        self.runtime.block_on(tracker.inspect(probe))
    }

    fn capture(&self) -> Arc<StdMutex<Vec<(Topic, Value)>>> {
        let captured = Arc::new(StdMutex::new(Vec::new()));
        let sink = captured.clone();
        self.bus.add_tap(move |event| {
            sink.lock()
                .unwrap()
                .push((event.topic(), event.payload_value()));
        });
        captured
    }

    fn scalar_f64(&self, sql: &'static str, binds: &[&str]) -> f64 {
        let binds: Vec<String> = binds.iter().map(|bind| bind.to_string()).collect();
        self.wait(self.db.with_reader(move |conn| {
            Ok(
                conn.query_row(sql, rusqlite::params_from_iter(binds.iter()), |row| {
                    row.get::<_, f64>(0)
                })?,
            )
        }))
        .unwrap()
    }

    fn scalar_i64(&self, sql: &'static str, binds: &[&str]) -> i64 {
        let binds: Vec<String> = binds.iter().map(|bind| bind.to_string()).collect();
        self.wait(self.db.with_reader(move |conn| {
            Ok(
                conn.query_row(sql, rusqlite::params_from_iter(binds.iter()), |row| {
                    row.get::<_, i64>(0)
                })?,
            )
        }))
        .unwrap()
    }

    fn execute(&self, sql: &'static str) {
        self.wait(self.db.with_writer(move |conn| {
            conn.execute(sql, [])?;
            Ok(())
        }))
        .unwrap();
    }
}

fn naive(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").unwrap()
}

fn updated_events(captured: &StdMutex<Vec<(Topic, Value)>>) -> Vec<Value> {
    captured
        .lock()
        .unwrap()
        .iter()
        .filter(|(topic, _)| *topic == Topic::TrackingSessionUpdated)
        .map(|(_, data)| data.clone())
        .collect()
}

#[test]
fn session_lifecycle_round_trip() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|name| if name == "Rifle" { 0.05 } else { 0.0 })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    let captured = rig.capture();

    assert!(!tracker.is_tracking());
    assert!(rig.wait(tracker.stop_session()).unwrap().is_none());
    assert!(!rig.bus.has_subscribers(Topic::Combat));

    let session = rig.wait(tracker.start_session()).unwrap();
    assert!(tracker.is_tracking());
    assert!(rig.bus.has_subscribers(Topic::Combat));
    let start_ts = naive_to_epoch(naive("2026-01-01T00:00:00"));
    assert_eq!(
        rig.scalar_f64(
            "SELECT started_at FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        start_ts
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        1
    );

    // Accumulate one kill with both shrapnel kinds, plus dangling
    // shots after it.
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 30.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![
            LootItem {
                item_name: "Animal Hide".into(),
                quantity: 1,
                value_ped: 4.5,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Shrapnel".into(),
                quantity: 50,
                value_ped: 0.5,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Shrapnel".into(),
                quantity: 10,
                value_ped: 0.1,
                is_enhancer_shrapnel: true,
            },
        ],
        total_ped: 5.1,
    }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.5,
            timestamp: "2026-01-01T00:00:03".into(),
        }));

    // A skill gain qualifies the session for a summary.
    {
        let session_id = session.id.clone();
        rig.wait(rig.db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
                     VALUES (?, 1.0, 'Rifle', 1.0, 0.5)",
                rusqlite::params![session_id],
            )?;
            Ok(())
        }))
        .unwrap();
    }

    rig.clock.advance(10.0).unwrap();
    let stopped = rig.wait(tracker.stop_session()).unwrap().unwrap();
    assert_eq!(stopped.id, session.id);
    assert_eq!(stopped.kills.len(), 1);
    assert_eq!(stopped.dangling_cost, Ped(0.05));
    assert!(!tracker.is_tracking());
    assert!(!rig.bus.has_subscribers(Topic::Combat));

    let end_ts = naive_to_epoch(naive("2026-01-01T00:00:10"));
    assert_eq!(
        rig.scalar_f64(
            "SELECT ended_at FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        end_ts
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        0
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT dangling_cost FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        0.05
    );

    // Ledger gains: the enhancer rebate at full value, the
    // conversion margin at 1%, both rounded half-even to 4.
    assert_eq!(
        rig.scalar_f64(
            "SELECT amount FROM ledger_entries WHERE tag = 'enhancer' \
                 AND description = 'Enhancer Shrapnel Rebate'",
            &[],
        ),
        0.1
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT amount FROM ledger_entries WHERE tag = 'convert' \
                 AND description = 'Shrapnel Conversion'",
            &[],
        ),
        0.005
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM session_summaries WHERE session_id = ?",
            &[&session.id],
        ),
        1
    );

    // Producer events after the stop reach nothing.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:11".into()),
        items: vec![],
        total_ped: 0.0,
    }));
    assert_eq!(rig.scalar_i64("SELECT COUNT(*) FROM kills", &[]), 1);

    // The lifecycle's domain events: started, the hotbar weapon-switch
    // re-hydrate nudge (emitted directly, stamped at the switch's
    // instant), then stopped.
    let updated = updated_events(&captured);
    assert_eq!(updated.len(), 3);
    assert_eq!(updated[0]["payload"]["reason"], "started");
    assert_eq!(updated[0]["payload"]["status"], "active");
    assert_eq!(updated[0]["occurred_at"], to_iso_utc(start_ts));
    assert_eq!(updated[1]["payload"]["reason"], "updated");
    assert_eq!(updated[1]["payload"]["status"], "active");
    assert_eq!(updated[1]["occurred_at"], to_iso_utc(start_ts));
    assert_eq!(updated[2]["payload"]["reason"], "stopped");
    assert_eq!(updated[2]["payload"]["status"], "idle");
    assert_eq!(updated[2]["occurred_at"], to_iso_utc(end_ts));
}

#[test]
fn start_while_tracking_stops_the_prior_session() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    let captured = rig.capture();

    let first = rig.wait(tracker.start_session()).unwrap();
    rig.clock.advance(5.0).unwrap();
    let second = rig.wait(tracker.start_session()).unwrap();
    assert_ne!(first.id, second.id);

    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            &[&first.id],
        ),
        0
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            &[&second.id],
        ),
        1
    );

    // The second start's event order: the prior session's stop
    // lands before the new session's start.
    let topics: Vec<Topic> = captured
        .lock()
        .unwrap()
        .iter()
        .map(|(topic, _)| *topic)
        .collect();
    assert_eq!(
        topics,
        vec![
            Topic::SessionStarted,
            Topic::TrackingSessionUpdated,
            Topic::SessionStopped,
            Topic::TrackingSessionUpdated,
            Topic::SessionStarted,
            Topic::TrackingSessionUpdated,
        ]
    );
}

#[test]
fn recovery_closes_crash_orphaned_sessions() {
    let rig = rig();
    rig.execute(
        "INSERT INTO tracking_sessions (id, started_at, is_active, mob_tracking_mode) \
             VALUES ('orphan', 1000.0, 1, 'mob')",
    );
    rig.execute(
        "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
             timestamp, shots_fired, damage_dealt, damage_taken, critical_hits, \
             cost_ped, enhancer_cost, loot_total_ped, is_global, is_hof) \
             VALUES ('k1', 'orphan', 'Atrox', '', '', 1500.0, 3, 30.0, 0.0, 0, \
             0.15, 0.0, 80.0, 0, 0)",
    );
    rig.execute(
        "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, \
             is_enhancer_shrapnel) VALUES ('k1', 'Shrapnel', 500, 50.0, 0)",
    );
    rig.execute(
        "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, \
             is_enhancer_shrapnel) VALUES ('k1', 'Shrapnel', 300, 30.0, 1)",
    );
    rig.execute(
        "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, damage_dealt, \
             critical_hits, cost_per_shot) VALUES ('k1', 'Rifle', 3, 30.0, 0, 0.05)",
    );
    rig.execute(
        "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
             VALUES ('orphan', 1100.0, 'Rifle', 1.0, 0.5)",
    );

    let _tracker = rig.tracker(Providers::default());

    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = 'orphan'",
            &[],
        ),
        0
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT ended_at FROM tracking_sessions WHERE id = 'orphan'",
            &[],
        ),
        1500.0
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT amount FROM ledger_entries WHERE tag = 'convert'",
            &[],
        ),
        0.5
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT amount FROM ledger_entries WHERE tag = 'enhancer'",
            &[],
        ),
        30.0
    );
    let expected_date = naive_isoformat(epoch_to_naive(1500.0));
    let date: String = rig
        .wait(rig.db.with_reader(|conn| {
            Ok(conn.query_row(
                "SELECT date FROM ledger_entries WHERE tag = 'convert'",
                [],
                |row| row.get::<_, String>(0),
            )?)
        }))
        .unwrap();
    assert_eq!(date, expected_date);
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM session_summaries WHERE session_id = 'orphan'",
            &[],
        ),
        1
    );
}

#[test]
fn stopping_a_session_relands_its_days_rollups() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|name| if name == "Rifle" { 0.05 } else { 0.0 })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    let session = rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 30.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![LootItem {
            item_name: "Animal Hide".into(),
            quantity: 1,
            value_ped: 4.5,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 4.5,
    }));
    // A dangling shot after the kill: its cost persists only at stop.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.5,
            timestamp: "2026-01-01T00:00:03".into(),
        }));

    // Two days later the session's day is behind the heal watermark,
    // rolled up WITHOUT the still-unpersisted dangling cost.
    rig.clock.advance(2.0 * 86_400.0).unwrap();
    let now = naive_to_epoch(rig.clock.now());
    rig.runtime.block_on(async {
        rig.db
            .with_writer(move |conn| crate::daily_rollup::heal_rollups(conn, now))
            .await
            .unwrap();
    });
    let start_day = crate::daily_rollup::epoch_day(naive_to_epoch(naive("2026-01-01T00:00:00")));
    let pre_stop: Option<f64> = {
        let start_day = start_day.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT dangling_cost FROM daily_rollups WHERE day = ?",
                rusqlite::params![start_day],
                |row| row.get::<_, Option<f64>>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(pre_stop, Some(0.0), "pre-stop: the column default");

    // The stop transaction persists the dangling cost and relands
    // the session's days in the same commit.
    let stopped = rig.wait(tracker.stop_session()).unwrap().unwrap();
    assert_eq!(stopped.id, session.id);
    let post_stop: Option<f64> = {
        let start_day = start_day.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT dangling_cost FROM daily_rollups WHERE day = ?",
                rusqlite::params![start_day],
                |row| row.get::<_, Option<f64>>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(post_stop, Some(0.05), "the stop hook relanded the day");
    // The stop day itself (today) stays raw.
    let today = crate::daily_rollup::epoch_day(now);
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM daily_rollups WHERE day >= ?",
            &[&today],
        ),
        0
    );
}

#[test]
fn recovery_relands_the_orphans_days_and_backdated_ledger_keys() {
    let rig = rig();
    let start_epoch = naive_to_epoch(naive("2025-12-30T10:00:00"));
    let kill_epoch = naive_to_epoch(naive("2025-12-30T11:00:00"));
    rig.wait(rig.db.with_writer(move |conn| {
        conn.execute(
            "INSERT INTO tracking_sessions (id, started_at, is_active, mob_tracking_mode) \
                 VALUES ('orphan', ?, 1, 'mob')",
            rusqlite::params![start_epoch],
        )?;
        conn.execute(
            "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
                 timestamp, shots_fired, damage_dealt, damage_taken, critical_hits, \
                 cost_ped, enhancer_cost, loot_total_ped, is_global, is_hof) \
                 VALUES ('k1', 'orphan', 'Atrox', '', '', ?, 3, 30.0, 0.0, 0, \
                 0.15, 0.0, 80.0, 0, 0)",
            rusqlite::params![kill_epoch],
        )?;
        Ok(())
    }))
    .unwrap();
    rig.execute(
        "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, \
             is_enhancer_shrapnel) VALUES ('k1', 'Shrapnel', 500, 50.0, 0)",
    );
    rig.execute(
        "INSERT INTO kill_tool_stats (kill_id, tool_name, shots_fired, damage_dealt, \
             critical_hits, cost_per_shot) VALUES ('k1', 'Rifle', 3, 30.0, 0, 0.05)",
    );

    // Heal first (clock: 2026-01-01), so the orphan's day sits at or
    // below the watermark when recovery closes it.
    let now = naive_to_epoch(rig.clock.now());
    rig.runtime.block_on(async {
        rig.db
            .with_writer(move |conn| crate::daily_rollup::heal_rollups(conn, now))
            .await
            .unwrap();
    });

    let _tracker = rig.tracker(Providers::default());

    // Recovery relanded the kill day's families.
    let kill_day = crate::daily_rollup::epoch_day(kill_epoch);
    let loot: Option<f64> = {
        let kill_day = kill_day.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT loot_tt FROM daily_rollups WHERE day = ?",
                rusqlite::params![kill_day],
                |row| row.get::<_, Option<f64>>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(loot, Some(80.0));

    // The backdated shrapnel-conversion ledger entry (a datetime
    // key at the crashed session's end) rolled up eagerly too.
    let ledger_key = naive_isoformat(epoch_to_naive(kill_epoch));
    let (kind, amount): (String, f64) = {
        let ledger_key = ledger_key.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT entry_type, amount FROM daily_ledger_rollups WHERE day = ? AND tag = 'convert'",
                rusqlite::params![ledger_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
            )?)
        }))
        .unwrap()
    };
    assert_eq!((kind.as_str(), amount), ("markup", 0.5));
}

#[test]
fn a_failed_stop_rolls_back_every_stop_write() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    let session = rig.wait(tracker.start_session()).unwrap();
    // A kill with convertible Shrapnel and a skill gain, so the stop
    // sequence writes the session close, a ledger gain, and a summary.
    rig.execute(
        "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
             timestamp, shots_fired, damage_dealt, damage_taken, critical_hits, \
             cost_ped, enhancer_cost, loot_total_ped, is_global, is_hof) \
             VALUES ('k1', (SELECT id FROM tracking_sessions WHERE is_active = 1), \
             'Atrox', '', '', 1500.0, 3, 30.0, 0.0, 0, 0.15, 0.0, 80.0, 0, 0)",
    );
    rig.execute(
        "INSERT INTO kill_loot_items (kill_id, item_name, quantity, value_ped, \
             is_enhancer_shrapnel) VALUES ('k1', 'Shrapnel', 500, 50.0, 0)",
    );
    rig.execute(
        "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
             VALUES ((SELECT id FROM tracking_sessions WHERE is_active = 1), 1100.0, \
             'Rifle', 1.0, 0.5)",
    );
    // Force the final statement of the stop sequence to fail.
    rig.execute("DROP TABLE session_summaries");

    assert!(rig.wait(tracker.stop_session()).is_err());

    // The whole stop transaction rolled back: the session is still
    // active with no end stamp, and no ledger gain landed.
    assert_eq!(
        rig.scalar_i64(
            "SELECT is_active FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        1
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM tracking_sessions WHERE id = ? AND ended_at IS NOT NULL",
            &[&session.id],
        ),
        0
    );
    assert_eq!(
        rig.scalar_i64("SELECT COUNT(*) FROM ledger_entries", &[]),
        0
    );
}

#[test]
fn loot_creates_and_persists_kills_with_filtering() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|name| if name == "Rifle" { 0.05 } else { 0.0 })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    let session = rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 30.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::CriticalHit {
            amount: 10.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::TargetDodge {
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageReceived {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));

    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![
            LootItem {
                item_name: "Animal Hide".into(),
                quantity: 1,
                value_ped: 4.5,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Universal Ammo".into(),
                quantity: 20,
                value_ped: 0.2,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Shrapnel".into(),
                quantity: 10,
                value_ped: 0.1,
                is_enhancer_shrapnel: true,
            },
        ],
        total_ped: 4.8,
    }));

    let kill_id: String = {
        let session_id = session.id.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT id FROM kills WHERE session_id = ?",
                rusqlite::params![session_id],
                |row| row.get::<_, String>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(
        rig.scalar_i64("SELECT shots_fired FROM kills WHERE id = ?", &[&kill_id]),
        3
    );
    assert_eq!(
        rig.scalar_f64("SELECT damage_dealt FROM kills WHERE id = ?", &[&kill_id]),
        40.0
    );
    assert_eq!(
        rig.scalar_f64("SELECT damage_taken FROM kills WHERE id = ?", &[&kill_id]),
        5.0
    );
    assert_eq!(
        rig.scalar_i64("SELECT critical_hits FROM kills WHERE id = ?", &[&kill_id],),
        1
    );
    assert_eq!(
        rig.scalar_f64("SELECT cost_ped FROM kills WHERE id = ?", &[&kill_id]),
        0.05 * 3.0
    );
    // The blacklisted ammo never lands; the enhancer shrapnel
    // lands as an item but stays out of the loot total.
    assert_eq!(
        rig.scalar_f64("SELECT loot_total_ped FROM kills WHERE id = ?", &[&kill_id],),
        4.5
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM kill_loot_items WHERE kill_id = ?",
            &[&kill_id],
        ),
        2
    );
    let mob: String = {
        let kill_id = kill_id.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT mob_name FROM kills WHERE id = ?",
                rusqlite::params![kill_id],
                |row| row.get::<_, String>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(mob, "Unknown");
    assert_eq!(
        rig.scalar_i64(
            "SELECT shots_fired FROM kill_tool_stats WHERE kill_id = ? \
                 AND tool_name = 'Rifle'",
            &[&kill_id],
        ),
        3
    );
    assert_eq!(
        rig.scalar_f64("SELECT timestamp FROM kills WHERE id = ?", &[&kill_id],),
        naive_to_epoch(naive("2026-01-01T00:00:02"))
    );

    // The accumulator reset: an immediate second group carries
    // zero shots.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:04".into()),
        items: vec![LootItem {
            item_name: "Mud".into(),
            quantity: 1,
            value_ped: 0.03,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 0.03,
    }));
    assert_eq!(
        rig.scalar_i64(
            "SELECT shots_fired FROM kills WHERE session_id = ? AND id != ?",
            &[&session.id, &kill_id],
        ),
        0
    );
}

#[test]
fn loot_dedup_inside_the_window_only() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    let group = |ts: &str| {
        BusEvent::LootGroup(LootGroupPayload {
            kind: LootTag,
            timestamp: Some(ts.into()),
            items: vec![LootItem {
                item_name: "Animal Hide".into(),
                quantity: 1,
                value_ped: 1.0,
                is_enhancer_shrapnel: false,
            }],
            total_ped: 1.0,
        })
    };
    rig.bus.publish(&group("2026-01-01T00:00:02"));
    // Identical fingerprint inside the strict 2s window: dropped.
    rig.bus.publish(&group("2026-01-01T00:00:03"));
    assert_eq!(rig.scalar_i64("SELECT COUNT(*) FROM kills", &[]), 1);
    // Exactly the window: recorded (the comparison is strict).
    rig.bus.publish(&group("2026-01-01T00:00:04"));
    assert_eq!(rig.scalar_i64("SELECT COUNT(*) FROM kills", &[]), 2);
    // A different fingerprint inside the window: recorded.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:05".into()),
        items: vec![LootItem {
            item_name: "Mud".into(),
            quantity: 1,
            value_ped: 1.0,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 1.0,
    }));
    assert_eq!(rig.scalar_i64("SELECT COUNT(*) FROM kills", &[]), 3);
}

#[test]
fn snapshot_aggregates_and_rounds_the_readout() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|name| if name == "Rifle" { 0.05 } else { 0.0 })),
            ..Default::default()
        }),
        player_name: "Hero".to_string(),
        ..Providers::default()
    });

    let idle = rig.wait(tracker.snapshot()).unwrap();
    assert!(idle.active.is_none());
    assert_eq!(idle.current_tool, None);

    let session = rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    rig.bus.publish(&BusEvent::ActiveHealToolChanged(
        ActiveHealToolChangedPayload {
            tool_name: "FAP".into(),
            cost_per_use_ped: 0.02,
            reload_seconds: 2.5,
            source: None,
        },
    ));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 30.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::CriticalHit {
            amount: 10.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::TargetDodge {
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![LootItem {
            item_name: "Animal Hide".into(),
            quantity: 1,
            value_ped: 5.0,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 5.0,
    }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 20.0,
            timestamp: "2026-01-01T00:00:03".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:04".into()),
        items: vec![LootItem {
            item_name: "Mud".into(),
            quantity: 1,
            value_ped: 0.03,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 0.03,
    }));
    // In-flight accumulator damage after the latest kill.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.5,
            timestamp: "2026-01-01T00:00:05".into(),
        }));
    // Two counted heals (the second exactly at the reload bound).
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 12.0,
        timestamp: "2026-01-01T00:00:05".into(),
    }));
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 12.0,
        timestamp: "2026-01-01T00:00:07.500000".into(),
    }));
    // A global correlated to the latest kill.
    rig.bus
        .publish(&BusEvent::Global(GlobalPayload::GlobalKill {
            timestamp: "2026-01-01T00:00:05".into(),
            player: "hero".into(),
            creature: "Atrox".into(),
            value: 12.0,
        }));
    {
        let session_id = session.id.clone();
        rig.wait(rig.db.with_writer(move |conn| {
            conn.execute(
                "INSERT INTO skill_gains (session_id, timestamp, skill_name, amount, ped_value) \
                     VALUES (?, 1.0, 'Rifle', 1.0, 1.0), (?, 2.0, 'Rifle', 1.0, 0.25)",
                rusqlite::params![session_id.clone(), session_id],
            )?;
            Ok(())
        }))
        .unwrap();
    }

    rig.clock.advance(60.0).unwrap();
    let readout = rig.wait(tracker.snapshot()).unwrap();
    assert_eq!(readout.current_tool.as_deref(), Some("Rifle"));
    let active = readout.active.unwrap();
    assert_eq!(active.session_id, session.id);
    assert_eq!(active.started_at, "2026-01-01T00:00:00");
    assert_eq!(active.kill_count, 2);
    assert_eq!(active.elapsed, 60);
    assert_eq!(active.cost, 0.29);
    assert_eq!(active.returns, 5.03);
    assert_eq!(active.pes, 1.25);
    assert_eq!(active.net, 4.74);
    assert_eq!(active.return_rate, 17.3448);
    assert_eq!(active.damage_dealt_total, 60.0);
    assert_eq!(active.weapon_damage_dealt, 67.5);
    assert_eq!(active.weapon_cost, 0.25);
    assert_eq!(active.shots_fired_total, 4);
    assert_eq!(active.critical_hits_total, 1);
    assert_eq!(active.max_damage, 40.0);
    assert_eq!(active.globals_count, 1);
    assert_eq!(active.hofs_count, 0);
    assert_eq!(active.latest_kill_loot, Some(0.03));
    assert_eq!(active.multiplier_last, Some(0.6));
    assert_eq!(active.multiplier_avg, Some(16.9667));
    assert_eq!(active.multiplier_max, Some(33.3333));
    assert_eq!(active.multiplier_history, vec![33.3333, 0.6]);
    assert_eq!(active.cumulative_net_history, vec![4.82, 4.79]);
    assert_eq!(active.current_mob, None);
    assert_eq!(active.mob_source, None);
    assert_eq!(active.mob_entry_mode, "mob");
    assert_eq!(active.notable_event_rows.len(), 1);
    let row = &active.notable_event_rows[0];
    assert_eq!(row.0, "global_kill");
    assert_eq!(row.1, "Atrox");
    assert_eq!(row.2, 12.0);
    assert_eq!(row.3, Some(naive_to_epoch(naive("2026-01-01T00:00:05"))));
    assert!(active.warnings.is_empty());

    // The session heal cost reached the session row on stop.
    rig.wait(tracker.stop_session()).unwrap();
    assert_eq!(
        rig.scalar_f64(
            "SELECT heal_cost FROM tracking_sessions WHERE id = ?",
            &[&session.id],
        ),
        0.04
    );
}

#[test]
fn unknown_tool_stats_merge_on_identification() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|name| if name == "Pistol" { 0.02 } else { 0.0 })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();

    // Shots before any tool is known accumulate under "Unknown".
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 9.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::CriticalHit {
            amount: 4.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Pistol".into(),
            source: None,
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 6.0,
            timestamp: "2026-01-01T00:00:02".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:03".into()),
        items: vec![],
        total_ped: 0.0,
    }));

    let rows: Vec<(String, i64, f64, i64, f64)> = rig
        .wait(rig.db.with_reader(|conn| {
            let mut stmt = conn.prepare(
                "SELECT tool_name, shots_fired, damage_dealt, critical_hits, cost_per_shot \
                     FROM kill_tool_stats",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        }))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], ("Pistol".to_string(), 3, 19.0, 1, 0.02));
}

#[test]
fn phased_tool_stats_split_on_cost_change() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    rig.probe(&tracker, |actor| {
        let accumulator = &mut actor.session.active_mut().unwrap().accumulator;
        TrackerActor::tool_stats_for_phase(accumulator, "Rifle", Ped(0.05)).shots_fired += 1;
        // Within the tolerance: the same phase.
        TrackerActor::tool_stats_for_phase(accumulator, "Rifle", Ped(0.05 + 1e-12)).shots_fired +=
            1;
        // A real cost change: a second phase keyed `Rifle#2`.
        TrackerActor::tool_stats_for_phase(accumulator, "Rifle", Ped(0.04)).shots_fired += 1;
        // A third: `Rifle#3`; a different tool keeps its bare key.
        TrackerActor::tool_stats_for_phase(accumulator, "Rifle", Ped(0.03)).shots_fired += 1;
        TrackerActor::tool_stats_for_phase(accumulator, "Pistol", Ped(0.02)).shots_fired += 1;
        // A cost difference of exactly the tolerance opens a phase:
        // the comparison is strict (2e-9 - 1e-9 is exactly 1e-9).
        TrackerActor::tool_stats_for_phase(accumulator, "Laser", Ped(1e-9)).shots_fired += 1;
        TrackerActor::tool_stats_for_phase(accumulator, "Laser", Ped(2e-9)).shots_fired += 1;

        let keys: Vec<(String, String, i64)> = accumulator
            .tool_stats
            .iter()
            .map(|(key, stats)| (key.clone(), stats.tool_name.clone(), stats.shots_fired))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("Rifle".to_string(), "Rifle".to_string(), 2),
                ("Rifle#2".to_string(), "Rifle".to_string(), 1),
                ("Rifle#3".to_string(), "Rifle".to_string(), 1),
                ("Pistol".to_string(), "Pistol".to_string(), 1),
                ("Laser".to_string(), "Laser".to_string(), 1),
                ("Laser#2".to_string(), "Laser".to_string(), 1),
            ]
        );
    });
}

#[test]
fn heal_ticks_dedup_by_reload_and_warn_without_tool() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    // No heal tool equipped: the warning lands once, no cost.
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 10.0,
        timestamp: "2026-01-01T00:00:01".into(),
    }));
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 10.0,
        timestamp: "2026-01-01T00:00:09".into(),
    }));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(
            active.warnings,
            vec!["Healing detected: no heal tool equipped via hotbar".to_string()]
        );
        assert_eq!(active.heal_cost, Ped::ZERO);
    });

    rig.bus.publish(&BusEvent::ActiveHealToolChanged(
        ActiveHealToolChangedPayload {
            tool_name: "FAP".into(),
            cost_per_use_ped: 0.03,
            reload_seconds: 5.0,
            source: None,
        },
    ));
    // Counted; then inside the 5s reload window (deduped); then at
    // the bound (counted: the comparison admits equality).
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 10.0,
        timestamp: "2026-01-01T00:00:20".into(),
    }));
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 10.0,
        timestamp: "2026-01-01T00:00:24".into(),
    }));
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 10.0,
        timestamp: "2026-01-01T00:00:25".into(),
    }));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.heal_cost, Ped(0.06));
        assert_eq!(active.warnings.len(), 1, "the warning fires once");
    });
}

#[test]
fn globals_correlate_within_the_window() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        player_name: "  Hero  ".to_string(),
        ..Providers::default()
    });
    let session = rig.wait(tracker.start_session()).unwrap();

    let loot = |ts: &str, value: f64| {
        BusEvent::LootGroup(LootGroupPayload {
            kind: LootTag,
            timestamp: Some(ts.into()),
            items: vec![LootItem {
                item_name: "Animal Hide".into(),
                quantity: 1,
                value_ped: value,
                is_enhancer_shrapnel: false,
            }],
            total_ped: value,
        })
    };
    rig.bus.publish(&loot("2026-01-01T00:00:02", 1.0));
    // The wrong player never lands.
    rig.bus
        .publish(&BusEvent::Global(GlobalPayload::GlobalKill {
            timestamp: "2026-01-01T00:00:03".into(),
            player: "Villain".into(),
            creature: "Atrox".into(),
            value: 8.0,
        }));
    assert_eq!(
        rig.scalar_i64("SELECT COUNT(*) FROM notable_events", &[]),
        0
    );
    // Case-insensitive match (the configured name is stripped at
    // construction); a HoF inside the window tags the kill.
    rig.bus.publish(&BusEvent::Global(GlobalPayload::HofKill {
        timestamp: "2026-01-01T00:00:04".into(),
        player: "HERO".into(),
        creature: "Atrox".into(),
        value: 120.0,
    }));
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM kills WHERE is_global = 1 AND is_hof = 1",
            &[],
        ),
        1
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM notable_events WHERE kill_id IS NOT NULL",
            &[],
        ),
        1
    );

    // A stale global (past the 5s window) records the notable
    // event with no kill correlation.
    rig.bus.publish(&loot("2026-01-01T00:00:10", 2.0));
    rig.bus
        .publish(&BusEvent::Global(GlobalPayload::GlobalKill {
            timestamp: "2026-01-01T00:00:16".into(),
            player: "Hero".into(),
            creature: "Rare Thing".into(),
            value: 50.0,
        }));
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM notable_events WHERE kill_id IS NULL \
                 AND mob_or_item = 'Rare Thing'",
            &[],
        ),
        1
    );
    assert_eq!(
        rig.scalar_i64("SELECT COUNT(*) FROM kills WHERE is_global = 1", &[]),
        1
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM notable_events WHERE session_id = ?",
            &[&session.id],
        ),
        2
    );

    // An empty configured player name disables correlation.
    let unnamed = rig.tracker(Providers::default());
    rig.wait(unnamed.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::Global(GlobalPayload::GlobalKill {
            timestamp: "2026-01-01T00:00:20".into(),
            player: "".into(),
            creature: "Atrox".into(),
            value: 1.0,
        }));
    assert_eq!(
        rig.scalar_i64("SELECT COUNT(*) FROM notable_events", &[]),
        2
    );
}

#[test]
fn enhancer_breaks_filter_and_deplete_stacks() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            profile: Some(Arc::new(|name| {
                (name == "Rifle").then(|| {
                    let profile = json!({
                        "damage_enhancers": 2,
                        "weapon_entity": {"name": "Rifle Prime"},
                    });
                    profile.as_object().unwrap().clone()
                })
            })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    rig.probe(&tracker, |actor| {
        let weapons = &actor.session.active().unwrap().weapons;
        assert_eq!(weapons.active_key.as_deref(), Some("Rifle Prime"));
        assert_eq!(
            weapons.enhancer_states["Rifle Prime"].stacks,
            vec![100, 100]
        );
    });

    // A non-damage enhancer never applies; a damage break naming
    // a different item never applies.
    rig.bus
        .publish(&BusEvent::EnhancerBreak(EnhancerBreakPayload {
            kind: EnhancerBreakTag,
            timestamp: "2026-01-01T00:00:01".into(),
            enhancer_name: "Accuracy Enhancer 5".into(),
            item_name: "Rifle Prime".into(),
            remaining: 150,
            shrapnel_ped: 0.0,
        }));
    rig.bus
        .publish(&BusEvent::EnhancerBreak(EnhancerBreakPayload {
            kind: EnhancerBreakTag,
            timestamp: "2026-01-01T00:00:01".into(),
            enhancer_name: "Damage Enhancer 5".into(),
            item_name: "Sword".into(),
            remaining: 150,
            shrapnel_ped: 0.0,
        }));
    rig.probe(&tracker, |actor| {
        assert_eq!(
            actor.session.active().unwrap().weapons.enhancer_states["Rifle Prime"].stacks,
            vec![100, 100]
        );
    });

    // A matching break with a remaining count redistributes,
    // front-loading the remainder. The match admits the observed
    // hotbar spelling and lowercased-alphanumeric containment.
    rig.bus
        .publish(&BusEvent::EnhancerBreak(EnhancerBreakPayload {
            kind: EnhancerBreakTag,
            timestamp: "2026-01-01T00:00:01".into(),
            enhancer_name: "Damage Enhancer 5".into(),
            item_name: "rifle-prime".into(),
            remaining: 151,
            shrapnel_ped: 0.0,
        }));
    rig.probe(&tracker, |actor| {
        assert_eq!(
            actor.session.active().unwrap().weapons.enhancer_states["Rifle Prime"].stacks,
            vec![76, 75]
        );
    });
    rig.bus
        .publish(&BusEvent::EnhancerBreak(EnhancerBreakPayload {
            kind: EnhancerBreakTag,
            timestamp: "2026-01-01T00:00:01".into(),
            enhancer_name: "damage enh".into(),
            item_name: "Rifle".into(),
            remaining: 150,
            shrapnel_ped: 0.0,
        }));
    rig.probe(&tracker, |actor| {
        assert_eq!(
            actor.session.active().unwrap().weapons.enhancer_states["Rifle Prime"].stacks,
            vec![75, 75]
        );
    });
}

#[test]
fn damage_enhancer_state_arithmetic() {
    let props = Arc::new(json!({"damage_enhancers": 3.7}));
    let mut state = DamageEnhancerState::from_props("Rifle", props);
    assert_eq!(state.stacks, vec![100, 100, 100], "int() truncates");
    assert_eq!(state.active_slots(), 3);

    state.set_total(7);
    assert_eq!(state.stacks, vec![3, 2, 2], "the remainder front-loads");
    state.set_total(-5);
    assert_eq!(state.stacks, vec![0, 0, 0], "totals clamp at zero");

    state.set_total(2);
    assert_eq!(state.stacks, vec![1, 1, 0]);
    assert_eq!(state.active_slots(), 2);
    // A break with no remaining decrements the last positive slot
    // and reports the depletion.
    assert!(state.apply_break(None));
    assert_eq!(state.stacks, vec![1, 0, 0]);
    assert!(
        state.apply_break(Some(3)),
        "redistribution re-activating slots reports the change"
    );
    assert_eq!(state.stacks, vec![1, 1, 1]);

    let mut slotless = DamageEnhancerState::from_props("Bare", Arc::new(json!({})));
    assert_eq!(slotless.stacks, Vec::<i64>::new());
    assert!(!slotless.apply_break(Some(50)), "no slots, no change");

    let negative =
        DamageEnhancerState::from_props("Neg", Arc::new(json!({"damage_enhancers": -2})));
    assert_eq!(negative.stacks, Vec::<i64>::new());
}

#[test]
fn trifecta_attribution_and_heal_filtering() {
    let rig = rig();
    let trifecta = json!({
        "small_weapon": {"name": "Pistol", "damage_min": 5.0, "damage_max": 10.0,
                         "total_damage": 0.0, "cost_per_shot_ped": 0.05,
                         "role": "small_weapon"},
        "big_weapon": {"name": "Cannon", "damage_min": 20.0, "damage_max": 40.0,
                       "total_damage": 0.0, "cost_per_shot_ped": 0.2,
                       "role": "big_weapon"},
        "heal_tool": {"name": "FAP", "cost_per_use_ped": 0.02, "reload_seconds": 2.5,
                      "heal_min": 10.0, "heal_max": 20.0},
    });
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            trifecta: Some(Arc::new(move || {
                Some(trifecta.as_object().unwrap().clone())
            })),
            ..Default::default()
        }),
        config: Arc::new(ScriptedConfig {
            trifecta_mode: true,
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();

    // Hotbar-driven changes are ignored in trifecta mode.
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Sword".into(),
            source: None,
        }));
    rig.bus.publish(&BusEvent::ActiveHealToolChanged(
        ActiveHealToolChangedPayload {
            tool_name: "Other".into(),
            cost_per_use_ped: 9.9,
            reload_seconds: 2.5,
            source: None,
        },
    ));
    rig.probe(&tracker, |actor| {
        assert_eq!(actor.session.active().unwrap().weapons.hotbar_tool, None);
        assert_eq!(actor.heal_tool.name.as_deref(), Some("FAP"));
        assert_eq!(actor.heal_tool.cost_per_use, Ped(0.02));
    });

    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    // Unmatched damage warns once and lands under "Unknown".
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 0.5,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 0.5,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    // A critical inside the big weapon's regular band prefers the
    // big regular explanation.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::CriticalHit {
            amount: 25.0,
            timestamp: "2026-01-01T00:00:02".into(),
        }));
    // A countered shot attributes to the last offensive tool.
    rig.bus.publish(&BusEvent::Combat(CombatPayload::TargetJam {
        timestamp: "2026-01-01T00:00:02".into(),
    }));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        let stats: Vec<(String, i64, f64)> = active
            .accumulator
            .tool_stats
            .iter()
            .map(|(key, stats)| (key.clone(), stats.shots_fired, stats.cost_per_shot.value()))
            .collect();
        assert_eq!(
            stats,
            vec![
                ("Pistol".to_string(), 1, 0.05),
                ("Unknown".to_string(), 2, 0.0),
                ("Cannon".to_string(), 2, 0.2),
            ]
        );
        assert_eq!(
            active.warnings,
            vec!["Trifecta attribution: damage fell outside both weapon ranges".to_string()]
        );
    });

    // The trifecta heal band filters mismatched heal amounts
    // entirely (no dedup stamp, no cost).
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 50.0,
        timestamp: "2026-01-01T00:00:03".into(),
    }));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.heal_cost, Ped::ZERO);
        assert_eq!(active.last_heal_time, None);
    });
    rig.bus.publish(&BusEvent::Combat(CombatPayload::SelfHeal {
        amount: 15.0,
        timestamp: "2026-01-01T00:00:04".into(),
    }));
    rig.probe(&tracker, |actor| {
        assert_eq!(actor.session.active().unwrap().heal_cost, Ped(0.02));
    });
}

#[test]
fn tag_and_manual_mob_rules() {
    let rig = rig();

    // No session: every command refuses.
    let tracker = rig.tracker(Providers::default());
    assert_eq!(
        rig.wait(tracker.set_manual_tag("Foo")),
        Err(TrackerCommandError::NoActiveSession)
    );
    assert_eq!(
        rig.wait(tracker.set_manual_mob("Atrox", "Atrox", "Young")),
        Err(TrackerCommandError::NoActiveSession)
    );

    // Tag mode: the configured tag is stripped and stamps kills;
    // manual mob locking refuses; empty tags refuse.
    let tagged = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            mode: Some("tag".to_string()),
            tag: Some("  Team Hunt \u{1c}".to_string()),
            ..Default::default()
        }),
        ..Providers::default()
    });
    let session = rig.wait(tagged.start_session()).unwrap();
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![],
        total_ped: 0.0,
    }));
    let mob: String = {
        let session_id = session.id.clone();
        rig.wait(rig.db.with_reader(move |conn| {
            Ok(conn.query_row(
                "SELECT mob_name FROM kills WHERE session_id = ?",
                rusqlite::params![session_id],
                |row| row.get::<_, String>(0),
            )?)
        }))
        .unwrap()
    };
    assert_eq!(mob, "Team Hunt");
    assert_eq!(
        rig.wait(tagged.set_manual_mob("Atrox", "Atrox", "Young")),
        Err(TrackerCommandError::TagModeLocksMob)
    );
    assert_eq!(
        rig.wait(tagged.set_manual_tag("   ")),
        Err(TrackerCommandError::EmptyTag)
    );
    rig.wait(tagged.set_manual_tag(" Solo Run ")).unwrap();
    rig.probe(&tagged, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), Some("Solo Run"));
        assert_eq!(active.tag, "Solo Run");
        assert_eq!(active.mob.source(), Some(MobSource::Tag));
    });
    assert_eq!(
        rig.wait(tagged.release_current_mob()).as_deref(),
        Some("Solo Run")
    );
    rig.wait(tagged.stop_session()).unwrap();

    // Mob mode: the manual provider stamps "<maturity> <species>"
    // at start; tag setting refuses; release clears.
    let manual = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            manual_mob: Some(Arc::new(|| {
                Some(("Atrox".to_string(), "Young".to_string()))
            })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(manual.start_session()).unwrap();
    assert_eq!(
        rig.wait(manual.set_manual_tag("Foo")),
        Err(TrackerCommandError::NotTagMode)
    );
    rig.probe(&manual, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), Some("Young Atrox"));
        assert_eq!(active.mob.species_maturity(), ("Atrox", "Young"));
        assert_eq!(active.mob.source(), Some(MobSource::Manual));
    });
    rig.wait(manual.set_manual_mob("Old Atrox", "Atrox", "Old"))
        .unwrap();
    rig.probe(&manual, |actor| {
        assert_eq!(
            actor.session.active().unwrap().mob.name(),
            Some("Old Atrox")
        );
    });
    assert_eq!(
        rig.wait(manual.release_current_mob()).as_deref(),
        Some("Old Atrox")
    );
    assert_eq!(rig.wait(manual.release_current_mob()), None);
    rig.wait(manual.stop_session()).unwrap();

    // Manual entry disabled: the command refuses; a maturity-less
    // manual mob displays the bare species.
    let disabled = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            manual_entry_enabled: Some(Arc::new(|| false)),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(disabled.start_session()).unwrap();
    assert_eq!(
        rig.wait(disabled.set_manual_mob("Atrox", "Atrox", "")),
        Err(TrackerCommandError::ManualEntryDisabled)
    );
    rig.wait(disabled.stop_session()).unwrap();
    let bare = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            manual_mob: Some(Arc::new(|| Some(("Atrox".to_string(), String::new())))),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(bare.start_session()).unwrap();
    rig.probe(&bare, |actor| {
        assert_eq!(actor.session.active().unwrap().mob.name(), Some("Atrox"));
    });
    rig.wait(bare.stop_session()).unwrap();
}

#[test]
fn reload_config_transitions_manual_mob_and_heal_state() {
    let rig = rig();
    let scripted_mob: Arc<StdMutex<Option<(String, String)>>> = Arc::new(StdMutex::new(Some((
        "Atrox".to_string(),
        "Young".to_string(),
    ))));
    let provider_view = scripted_mob.clone();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            manual_mob: Some(Arc::new(move || provider_view.lock().unwrap().clone())),
            ..Default::default()
        }),
        ..Providers::default()
    });

    // Idle reload only refreshes the loot filter.
    rig.wait(tracker.reload_config());
    assert!(!tracker.is_tracking());

    rig.wait(tracker.start_session()).unwrap();
    rig.bus.publish(&BusEvent::ActiveHealToolChanged(
        ActiveHealToolChangedPayload {
            tool_name: "FAP".into(),
            cost_per_use_ped: 0.03,
            reload_seconds: 5.0,
            source: None,
        },
    ));
    rig.probe(&tracker, |actor| {
        assert_eq!(
            actor.session.active().unwrap().mob.name(),
            Some("Young Atrox")
        );
        assert_eq!(actor.heal_tool.cost_per_use, Ped(0.03));
    });

    // The provider switching mobs re-stamps; switching to None
    // clears a manual stamp; the non-trifecta branch resets the
    // heal scalars.
    *scripted_mob.lock().unwrap() = Some(("Feffoid".to_string(), String::new()));
    rig.wait(tracker.reload_config());
    rig.probe(&tracker, |actor| {
        assert_eq!(actor.session.active().unwrap().mob.name(), Some("Feffoid"));
        assert_eq!(actor.heal_tool.cost_per_use, Ped::ZERO);
        assert_eq!(actor.heal_tool.reload_seconds, 2.5);
    });
    *scripted_mob.lock().unwrap() = None;
    rig.wait(tracker.reload_config());
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), None);
        assert_eq!(active.mob.source(), None);
    });
}

#[test]
fn tick_flushed_coalesces_dirty_mutations() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    let captured = rig.capture();
    let session = rig.wait(tracker.start_session()).unwrap();

    // A clean tick wakes nothing.
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("2026-01-01T00:00:01".into()),
    }));
    assert_eq!(updated_events(&captured).len(), 1, "only the start event");

    // A mutating event then a tick: one update stamped with the
    // tick's own instant.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:02".into(),
        }));
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("2026-01-01T00:00:02".into()),
    }));
    let events = updated_events(&captured);
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[1],
        json!({
            "type": "tracking.session.updated",
            "event_version": 1,
            "occurred_at": to_iso_utc(naive_to_epoch(naive("2026-01-01T00:00:02"))),
            "payload": {"sessionId": session.id, "status": "active", "reason": "updated"},
        })
    );

    // The dirty flag resets: the next tick is silent again.
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("2026-01-01T00:00:03".into()),
    }));
    assert_eq!(updated_events(&captured).len(), 2);

    // An epoch-numeric tick stamp passes straight through the
    // float() leg; an absent one falls back to the injected clock.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:04".into(),
        }));
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("1735680000.0".into()),
    }));
    let events = updated_events(&captured);
    assert_eq!(events[2]["occurred_at"], "2024-12-31T21:20:00+00:00");

    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:05".into(),
        }));
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: None,
    }));
    let events = updated_events(&captured);
    assert_eq!(
        events[3]["occurred_at"],
        to_iso_utc(naive_to_epoch(naive("2026-01-01T00:00:00"))),
        "the frozen mock clock stamps the fallback"
    );

    // An unparseable timestamp drops the event (the original's
    // float() raise, contained) with the dirty flag consumed; a
    // numeric string passes through float() instead.
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:06".into(),
        }));
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("garbage".into()),
    }));
    assert_eq!(updated_events(&captured).len(), 4);
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("2026-01-01T00:00:07".into()),
    }));
    assert_eq!(
        updated_events(&captured).len(),
        4,
        "the dropped event consumed the dirty flag"
    );
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 5.0,
            timestamp: "2026-01-01T00:00:08".into(),
        }));
    rig.bus.publish(&BusEvent::TickFlushed(TickFlushedPayload {
        timestamp: Some("1735680000.5".into()),
    }));
    let events = updated_events(&captured);
    assert_eq!(events[4]["occurred_at"], "2024-12-31T21:20:00.500000+00:00");
}

#[test]
fn tool_change_emits_a_direct_overlay_nudge() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    let captured = rig.capture();
    let _session = rig.wait(tracker.start_session()).unwrap();
    assert_eq!(updated_events(&captured).len(), 1, "only the start event");

    // A hotbar weapon-switch emits one re-hydrate nudge immediately,
    // WITHOUT waiting for a chat-log tick: the coalesced tick only
    // flushes on combat, so the overlay must be nudged directly or it
    // stays stale until the first attack.
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    let events = updated_events(&captured);
    assert_eq!(events.len(), 2, "the weapon-switch nudged immediately");
    assert_eq!(events[1]["payload"]["reason"], "updated");
    assert_eq!(events[1]["payload"]["status"], "active");

    // Re-equipping the same weapon changes nothing: no nudge.
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: None,
        }));
    assert_eq!(
        updated_events(&captured).len(),
        2,
        "an unchanged tool re-equip emits nothing"
    );

    // A heal-tool equip nudges on the same direct path.
    rig.bus.publish(&BusEvent::ActiveHealToolChanged(
        ActiveHealToolChangedPayload {
            tool_name: "FAP-5".into(),
            cost_per_use_ped: 0.5,
            reload_seconds: 2.5,
            source: None,
        },
    ));
    assert_eq!(
        updated_events(&captured).len(),
        3,
        "the heal-tool equip nudged immediately"
    );
}

#[test]
fn session_event_wire_shape_matches_the_python_model_dump() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    let captured = rig.capture();
    let session = rig.wait(tracker.start_session()).unwrap();

    let events = updated_events(&captured);
    let start_ts = naive_to_epoch(naive("2026-01-01T00:00:00"));
    assert_eq!(
        events[0],
        json!({
            "type": "tracking.session.updated",
            "event_version": 1,
            "occurred_at": to_iso_utc(start_ts),
            "payload": {"sessionId": session.id, "status": "active", "reason": "started"},
        })
    );
    let captured_topics: Vec<Topic> = captured
        .lock()
        .unwrap()
        .iter()
        .map(|(topic, _)| *topic)
        .collect();
    assert!(captured_topics.contains(&Topic::TrackingSessionUpdated));
}

#[test]
fn helper_pins() {
    assert_eq!(to_iso_utc(1735680000.0), "2024-12-31T21:20:00+00:00");
    assert_eq!(to_iso_utc(1735680000.5), "2024-12-31T21:20:00.500000+00:00");

    let whole = naive("2026-01-01T00:00:05");
    assert_eq!(naive_isoformat(whole), "2026-01-01T00:00:05");
    let fractional =
        NaiveDateTime::parse_from_str("2026-01-01T00:00:05.250000", "%Y-%m-%dT%H:%M:%S%.f")
            .unwrap();
    assert_eq!(naive_isoformat(fractional), "2026-01-01T00:00:05.250000");

    assert_eq!(
        parse_bus_timestamp(Some(&json!("2026-01-01T00:00:05"))),
        Some(whole)
    );
    assert_eq!(
        parse_bus_timestamp(Some(&json!("2026-01-01T00:00:05.5"))),
        NaiveDateTime::parse_from_str("2026-01-01T00:00:05.5", "%Y-%m-%dT%H:%M:%S%.f").ok()
    );
    assert_eq!(parse_bus_timestamp(Some(&json!("garbage"))), None);
    assert_eq!(parse_bus_timestamp(Some(&json!(12.5))), None);
    assert_eq!(parse_bus_timestamp(None), None);

    let delta = naive("2026-01-01T00:00:05") - naive("2026-01-01T00:00:02");
    assert_eq!(python_total_seconds(delta), 3.0);
    let negative = naive("2026-01-01T00:00:02") - naive("2026-01-01T00:00:05");
    assert_eq!(python_total_seconds(negative), -3.0);

    // The naive epoch round-trip holds in the host zone.
    let instant = naive("2026-06-15T12:30:45");
    assert_eq!(epoch_to_naive(naive_to_epoch(instant)), instant);

    // The instant basis composes to the same bytes: resolving a local
    // reading and rendering it back is the identity for representable
    // wall-clock times, and its epoch matches the naive path exactly.
    let resolved = super::time::resolve_local(instant);
    assert_eq!(
        super::time::local_isoformat(resolved),
        naive_isoformat(instant)
    );
    assert_eq!(
        super::time::instant_to_epoch(resolved),
        naive_to_epoch(instant)
    );
    assert_eq!(
        super::time::epoch_to_instant(naive_to_epoch(instant)),
        resolved
    );

    assert!(value_truthy(&json!(true)));
    assert!(value_truthy(&json!(1.5)));
    assert!(value_truthy(&json!("x")));
    assert!(value_truthy(&json!([0])));
    assert!(value_truthy(&json!({"k": 0})));
    assert!(!value_truthy(&json!(null)));
    assert!(!value_truthy(&json!(false)));
    assert!(!value_truthy(&json!(0)));
    assert!(!value_truthy(&json!("")));
    assert!(!value_truthy(&json!([])));
    assert!(!value_truthy(&json!({})));
}
#[test]
fn snapshot_prices_enhancer_cost_and_skips_costless_multipliers() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    let loot = |ts: &str, name: &str, value: f64| {
        BusEvent::LootGroup(LootGroupPayload {
            kind: LootTag,
            timestamp: Some(ts.into()),
            items: vec![LootItem {
                item_name: name.into(),
                quantity: 1,
                value_ped: value,
                is_enhancer_shrapnel: false,
            }],
            total_ped: value,
        })
    };
    rig.bus.publish(&loot("2026-01-01T00:00:02", "Hide", 2.0));
    let readout = rig.wait(tracker.snapshot()).unwrap();
    let active = readout.active.unwrap();
    // A costless kill: no rate, no multipliers (a >= admission
    // would divide by zero into infinities).
    assert_eq!(active.cost, 0.0);
    assert_eq!(active.return_rate, 0.0);
    assert_eq!(active.multiplier_last, None);
    assert_eq!(active.multiplier_avg, None);
    assert_eq!(active.multiplier_max, None);
    assert!(active.multiplier_history.is_empty());

    // Enhancer cost flows from the accumulator into the kill and
    // the live readout arithmetic.
    rig.probe(&tracker, |actor| {
        actor
            .session
            .active_mut()
            .unwrap()
            .accumulator
            .enhancer_cost = Ped(0.25);
    });
    rig.bus.publish(&loot("2026-01-01T00:00:05", "Mud", 1.0));
    rig.probe(&tracker, |actor| {
        actor
            .session
            .active_mut()
            .unwrap()
            .accumulator
            .enhancer_cost = Ped(0.5);
    });
    let active = rig.wait(tracker.snapshot()).unwrap().active.unwrap();
    assert_eq!(active.cost, 0.75);
    assert_eq!(active.returns, 3.0);
    assert_eq!(active.net, 2.25);
    assert_eq!(active.return_rate, 4.0);
    assert_eq!(active.cumulative_net_history, vec![2.0, 2.75]);

    // The unresolved enhancer cost is the dangling remainder.
    let stopped = rig.wait(tracker.stop_session()).unwrap().unwrap();
    assert_eq!(stopped.dangling_cost, Ped(0.5));
}

#[test]
fn inferred_cost_outranks_the_equipment_lookup() {
    let rig = rig();
    let trifecta = json!({
        "small_weapon": {"name": "Pistol", "damage_min": 5.0, "damage_max": 10.0,
                         "total_damage": 0.0, "cost_per_shot_ped": 0.05,
                         "role": "small_weapon"},
        "big_weapon": {"name": "Cannon", "damage_min": 20.0, "damage_max": 40.0,
                       "total_damage": 0.0, "cost_per_shot_ped": 0.2,
                       "role": "big_weapon"},
    });
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            trifecta: Some(Arc::new(move || {
                Some(trifecta.as_object().unwrap().clone())
            })),
            cost: Some(Arc::new(|_| 0.9)),
            ..Default::default()
        }),
        config: Arc::new(ScriptedConfig {
            trifecta_mode: true,
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();

    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::CriticalHit {
            amount: 25.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    // The countered shot carries no inferred cost, so the static
    // equipment cost prices it: a new phase of the last tool.
    rig.bus.publish(&BusEvent::Combat(CombatPayload::TargetJam {
        timestamp: "2026-01-01T00:00:02".into(),
    }));
    rig.probe(&tracker, |actor| {
        let stats: Vec<(String, f64, i64)> = actor
            .session
            .active()
            .unwrap()
            .accumulator
            .tool_stats
            .iter()
            .map(|(key, stats)| (key.clone(), stats.cost_per_shot.value(), stats.shots_fired))
            .collect();
        assert_eq!(
            stats,
            vec![
                ("Pistol".to_string(), 0.05, 1),
                ("Cannon".to_string(), 0.2, 1),
                ("Cannon#2".to_string(), 0.9, 1),
            ]
        );
    });
}

#[test]
fn the_unknown_entry_backfills_its_cost_once() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            cost: Some(Arc::new(|_| 0.7)),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 9.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 6.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![],
        total_ped: 0.0,
    }));
    let kill_id: String = rig
        .wait(rig.db.with_reader(|conn| {
            Ok(conn.query_row("SELECT id FROM kills", [], |row| row.get::<_, String>(0))?)
        }))
        .unwrap();
    assert_eq!(
        rig.scalar_f64("SELECT cost_ped FROM kills WHERE id = ?", &[&kill_id]),
        1.4
    );
    assert_eq!(
        rig.scalar_f64(
            "SELECT cost_per_shot FROM kill_tool_stats WHERE kill_id = ? \
                 AND tool_name = 'Unknown'",
            &[&kill_id],
        ),
        0.7
    );
}

#[test]
fn a_costless_tool_merges_unknown_into_its_bare_entry() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 9.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Stick".into(),
            source: None,
        }));
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 6.0,
            timestamp: "2026-01-01T00:00:02".into(),
        }));
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:03".into()),
        items: vec![],
        total_ped: 0.0,
    }));
    let rows: Vec<(String, i64, f64)> = rig
        .wait(rig.db.with_reader(|conn| {
            let mut stmt =
                conn.prepare("SELECT tool_name, shots_fired, damage_dealt FROM kill_tool_stats")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        }))
        .unwrap();
    assert_eq!(rows, vec![("Stick".to_string(), 2, 15.0)]);
}

#[test]
fn break_matching_admits_every_containment_direction() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            profile: Some(Arc::new(|name| {
                (name == "MyGun").then(|| {
                    json!({"damage_enhancers": 1, "weapon_entity": {"name": "Blast Master"}})
                        .as_object()
                        .unwrap()
                        .clone()
                })
            })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "MyGun".into(),
            source: None,
        }));

    let break_event = |item: &str, remaining: i64| {
        BusEvent::EnhancerBreak(EnhancerBreakPayload {
            kind: EnhancerBreakTag,
            timestamp: "2026-01-01T00:00:01".into(),
            enhancer_name: "Damage Enhancer 5".into(),
            item_name: item.into(),
            remaining,
            shrapnel_ped: 0.0,
        })
    };
    let stacks = |tracker: &HuntTracker| {
        rig.probe(tracker, |actor| {
            actor.session.active().unwrap().weapons.enhancer_states["Blast Master"]
                .stacks
                .clone()
        })
    };
    // The canonical name contains the item; the item contains the
    // canonical name; the observed hotbar name contains the item;
    // the item contains the observed name. Each direction matches.
    rig.bus.publish(&break_event("Blast", 99));
    assert_eq!(stacks(&tracker), vec![99]);
    rig.bus.publish(&break_event("Blast Master Deluxe", 98));
    assert_eq!(stacks(&tracker), vec![98]);
    rig.bus.publish(&break_event("Gun", 97));
    assert_eq!(stacks(&tracker), vec![97]);
    rig.bus.publish(&break_event("MyGun Deluxe", 96));
    assert_eq!(stacks(&tracker), vec![96]);
    // No containment in any direction: ignored.
    rig.bus.publish(&break_event("Sword", 90));
    assert_eq!(stacks(&tracker), vec![96]);

    // Stopping the session drops the whole ActiveSession, weapon
    // runtime included: the clear is structural under the typestate.
    rig.wait(tracker.stop_session()).unwrap();
    rig.probe(&tracker, |actor| {
        assert!(actor.session.active().is_none());
    });
}

#[test]
fn recovery_zero_timestamp_kills_fall_back_to_the_start() {
    let rig = rig();
    rig.execute(
        "INSERT INTO tracking_sessions (id, started_at, is_active, mob_tracking_mode) \
             VALUES ('orphan2', 2000.0, 1, 'mob')",
    );
    rig.execute(
        "INSERT INTO kills (id, session_id, mob_name, mob_species, mob_maturity, \
             timestamp, shots_fired, damage_dealt, damage_taken, critical_hits, \
             cost_ped, enhancer_cost, loot_total_ped, is_global, is_hof) \
             VALUES ('kz', 'orphan2', 'Atrox', '', '', 0.0, 1, 1.0, 0.0, 0, \
             0.1, 0.0, 1.0, 0, 0)",
    );
    let _tracker = rig.tracker(Providers::default());
    assert_eq!(
        rig.scalar_f64(
            "SELECT ended_at FROM tracking_sessions WHERE id = 'orphan2'",
            &[],
        ),
        2000.0,
        "a zero kill timestamp is falsy there, not a real maximum"
    );
}

#[test]
fn reload_config_in_tag_mode_never_consults_the_manual_provider() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            mode: Some("tag".to_string()),
            tag: Some("Team".to_string()),
            manual_mob: Some(Arc::new(|| {
                Some(("Atrox".to_string(), "Young".to_string()))
            })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.wait(tracker.reload_config());
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), Some("Team"));
        assert_eq!(active.mob.source(), Some(MobSource::Tag));
    });
}

#[test]
fn is_session_tag_mode_reflects_the_session_capture() {
    // The flag is the per-session mode snapshotted at start_session,
    // not the live config: idle is false (both before any session and
    // after one stops, since the snapshot is not cleared on stop), a
    // tag-mode session is true, a mob-mode session is false.
    let tag_rig = rig();
    let tagged = tag_rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            mode: Some("tag".to_string()),
            tag: Some("Team".to_string()),
            ..Default::default()
        }),
        ..Providers::default()
    });
    assert!(
        !tagged.is_session_tag_mode(),
        "idle (no session) is never tag mode"
    );
    tag_rig.wait(tagged.start_session()).unwrap();
    assert!(
        tagged.is_session_tag_mode(),
        "a session captured in tag mode reports tag mode"
    );
    tag_rig.wait(tagged.stop_session()).unwrap();
    assert!(
        !tagged.is_session_tag_mode(),
        "idle after stopping a tag session is never tag mode"
    );

    let mob_rig = rig();
    let mobbed = mob_rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            mode: Some("mob".to_string()),
            ..Default::default()
        }),
        ..Providers::default()
    });
    mob_rig.wait(mobbed.start_session()).unwrap();
    assert!(
        !mobbed.is_session_tag_mode(),
        "a session captured in mob mode is not tag mode"
    );
    mob_rig.wait(mobbed.stop_session()).unwrap();
}

#[test]
fn the_session_tag_stamps_only_in_tag_mode_with_a_real_tag() {
    let rig = rig();
    // A configured tag outside tag mode never stamps.
    let mob_mode = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            tag: Some("Sneaky".to_string()),
            manual_entry_enabled: Some(Arc::new(|| false)),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(mob_mode.start_session()).unwrap();
    rig.probe(&mob_mode, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), None);
        assert_eq!(active.mob.source(), None);
    });
    rig.wait(mob_mode.stop_session()).unwrap();

    // Tag mode with an all-blank tag has nothing to stamp.
    let blank = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            mode: Some("tag".to_string()),
            tag: Some("   ".to_string()),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(blank.start_session()).unwrap();
    rig.probe(&blank, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), None);
        assert_eq!(active.mob.source(), None);
    });
}

#[test]
fn the_blacklist_provider_refreshes_at_session_start() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            blacklist: vec!["Mud".to_string()],
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![
            LootItem {
                item_name: "Mud".into(),
                quantity: 1,
                value_ped: 1.0,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Hide".into(),
                quantity: 1,
                value_ped: 2.0,
                is_enhancer_shrapnel: false,
            },
        ],
        total_ped: 3.0,
    }));
    assert_eq!(
        rig.scalar_f64("SELECT loot_total_ped FROM kills", &[]),
        2.0,
        "the provider's blacklist drops Mud"
    );
    assert_eq!(
        rig.scalar_i64("SELECT COUNT(*) FROM kill_loot_items", &[]),
        1
    );
}

#[test]
fn command_error_messages_match_the_original() {
    assert_eq!(
        TrackerCommandError::NoActiveSession.to_string(),
        "No active session"
    );
    assert_eq!(
        TrackerCommandError::NotTagMode.to_string(),
        "Active session is not in tag mode"
    );
    assert_eq!(
        TrackerCommandError::EmptyTag.to_string(),
        "Tag cannot be empty"
    );
    assert_eq!(
        TrackerCommandError::TagModeLocksMob.to_string(),
        "Tag mode sessions do not allow manual mob locking"
    );
    assert_eq!(
        TrackerCommandError::ManualEntryDisabled.to_string(),
        "Manual mob entry is not enabled for this session"
    );
}

#[test]
fn enhancer_state_prices_through_the_cost_engine() {
    let props: Arc<Value> = Arc::new(json!({
        "weapon_entity": {"economy": {"decay": 0.05, "ammo_burn": 200}},
        "damage_enhancers": 2,
    }));
    let mut state = DamageEnhancerState::from_props("Rifle", props.clone());
    let priced = |slots: i64| {
        cost_per_shot_from_props(&props, Some(slots))["totalCostPerUse"]
            .as_f64()
            .unwrap()
            / 100.0
    };
    let two_slots = priced(2);
    assert!(two_slots > 0.0);
    assert_eq!(state.current_cost().value(), two_slots);
    assert_eq!(
        state.current_cost().value(),
        two_slots,
        "the cached read agrees"
    );
    state.set_total(1);
    assert_eq!(
        state.current_cost().value(),
        priced(1),
        "a stack change reprices at the new active count"
    );
}

#[test]
fn epoch_helpers_carry_and_keep_fractions() {
    assert_eq!(epoch_to_parts(5.0), (5, 0));
    assert_eq!(epoch_to_parts(2.25), (2, 250_000));
    assert_eq!(
        epoch_to_parts(1.999_999_9),
        (2, 0),
        "microsecond round-up carries into the seconds"
    );
    assert_eq!(
        epoch_to_parts(-0.25),
        (-1, 750_000),
        "negative fractions borrow a second"
    );

    let base = naive("2026-06-15T12:30:45");
    let fractional =
        NaiveDateTime::parse_from_str("2026-06-15T12:30:45.250000", "%Y-%m-%dT%H:%M:%S%.f")
            .unwrap();
    let delta = naive_to_epoch(fractional) - naive_to_epoch(base);
    assert!((delta - 0.25).abs() < 1e-9);
    assert_eq!(epoch_to_naive(naive_to_epoch(fractional)), fractional);
}
#[test]
fn a_zero_priced_weapon_state_still_prefers_the_inferred_cost() {
    let rig = rig();
    let trifecta = json!({
        "small_weapon": {"name": "Pistol", "damage_min": 5.0, "damage_max": 10.0,
                         "total_damage": 0.0, "cost_per_shot_ped": 0.05,
                         "role": "small_weapon",
                         "weapon_props": {"weapon_entity": {"economy": {
                             "decay": 0, "ammo_burn": 0}}}},
    });
    let tracker = rig.tracker(Providers {
        equipment: Arc::new(ScriptedEquipment {
            trifecta: Some(Arc::new(move || {
                Some(trifecta.as_object().unwrap().clone())
            })),
            cost: Some(Arc::new(|_| 0.3)),
            ..Default::default()
        }),
        config: Arc::new(ScriptedConfig {
            trifecta_mode: true,
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.bus
        .publish(&BusEvent::Combat(CombatPayload::DamageDealt {
            amount: 7.0,
            timestamp: "2026-01-01T00:00:01".into(),
        }));
    rig.probe(&tracker, |actor| {
        let (key, stats) = &actor.session.active().unwrap().accumulator.tool_stats[0];
        assert_eq!(key, "Pistol");
        assert_eq!(
            stats.cost_per_shot,
            Ped(0.05),
            "the attribution's cost backfills ahead of the equipment lookup"
        );
    });
}

#[test]
fn a_global_at_the_exact_window_bound_is_not_correlated() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        player_name: "Hero".to_string(),
        ..Providers::default()
    });
    let session = rig.wait(tracker.start_session()).unwrap();
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:20".into()),
        items: vec![LootItem {
            item_name: "Hide".into(),
            quantity: 1,
            value_ped: 1.0,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 1.0,
    }));
    rig.bus
        .publish(&BusEvent::Global(GlobalPayload::GlobalKill {
            timestamp: "2026-01-01T00:00:25".into(),
            player: "Hero".into(),
            creature: "Atrox".into(),
            value: 9.0,
        }));
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM kills WHERE session_id = ? AND is_global = 1",
            &[&session.id],
        ),
        0,
        "the five-second window is strict"
    );
    assert_eq!(
        rig.scalar_i64(
            "SELECT COUNT(*) FROM notable_events WHERE session_id = ? \
                 AND kill_id IS NULL",
            &[&session.id],
        ),
        1
    );
}

#[test]
fn reload_clears_a_manual_stamp_once_entry_disables() {
    let rig = rig();
    let enabled = Arc::new(StdMutex::new(true));
    let provider_view = enabled.clone();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            manual_entry_enabled: Some(Arc::new(move || *provider_view.lock().unwrap())),
            manual_mob: Some(Arc::new(|| {
                Some(("Atrox".to_string(), "Young".to_string()))
            })),
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();
    rig.probe(&tracker, |actor| {
        assert_eq!(
            actor.session.active().unwrap().mob.name(),
            Some("Young Atrox")
        );
    });

    *enabled.lock().unwrap() = false;
    rig.wait(tracker.reload_config());
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().unwrap();
        assert_eq!(active.mob.name(), None);
        assert_eq!(active.mob.source(), None);
    });
}

#[test]
fn prime_demo_activates_a_demo_session_and_stamps_its_mob() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    assert!(!tracker.is_tracking(), "idle before priming");

    let session = crate::tracking_models::TrackingSession {
        id: "demo".to_string(),
        start_time: chrono::DateTime::from_timestamp(1_000, 0).unwrap(),
        end_time: None,
        kills: Vec::new(),
        harvests: Vec::new(),
        dangling_cost: Ped::ZERO,
    };
    rig.wait(tracker.prime_demo(
        session,
        super::mob::MobSelection::Tag("Atrox".to_string()),
        super::mob::TrackingMode::Tag,
    ));

    // The demo session is live without ever running start_session.
    assert!(tracker.is_tracking(), "prime_demo activates the session");
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("a demo session is active");
        assert_eq!(active.mob.name(), Some("Atrox"));
    });
}

#[test]
fn on_tool_changed_ensures_a_bucket_before_merging_the_unknown_stats() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    // A demo session gives an active session without the bus wiring; the
    // handler is exercised directly on the actor thread.
    let session = crate::tracking_models::TrackingSession {
        id: "demo".to_string(),
        start_time: chrono::DateTime::from_timestamp(1_000, 0).unwrap(),
        end_time: None,
        kills: Vec::new(),
        harvests: Vec::new(),
        dangling_cost: Ped::ZERO,
    };
    rig.wait(tracker.prime_demo(
        session,
        super::mob::MobSelection::Unset,
        super::mob::TrackingMode::Mob,
    ));

    rig.probe(&tracker, |actor| {
        {
            let active = actor.session.active_mut().unwrap();
            active.weapons.hotbar_tool = None;
            // An accumulated 'Unknown' bucket plus an unrelated named
            // bucket: the identified tool has no bucket yet, so the merge
            // must create one before folding 'Unknown' in.
            active.accumulator.tool_stats = vec![
                (
                    "Unknown".to_string(),
                    crate::tracking_models::ToolStats {
                        tool_name: "Unknown".to_string(),
                        shots_fired: 5,
                        damage_dealt: 12.0,
                        critical_hits: 1,
                        cost_per_shot: Ped::ZERO,
                    },
                ),
                (
                    "Other".to_string(),
                    crate::tracking_models::ToolStats::new("Other", Ped::ZERO),
                ),
            ];
        }
        // The inert equipment gives Rifle no cost, so the else-branch that
        // ensures the bucket (rather than the positive-cost phase path) runs.
        actor.on_tool_changed(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".to_string(),
            source: None,
        }));

        let active = actor.session.active().unwrap();
        let keys: Vec<&str> = active
            .accumulator
            .tool_stats
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        assert!(
            keys.contains(&"Rifle"),
            "the identified tool ensured its bucket"
        );
        assert!(keys.contains(&"Other"), "the unrelated bucket survives");
        assert!(!keys.contains(&"Unknown"), "the Unknown bucket merged away");
        let rifle = active
            .accumulator
            .tool_stats
            .iter()
            .find(|(key, _)| key == "Rifle")
            .unwrap();
        assert_eq!(rifle.1.shots_fired, 5, "Unknown shots folded into Rifle");
    });
}

#[test]
fn harvest_tool_equip_prices_wood_swings_and_fails() {
    use crate::bus_events::{ActiveHarvestToolChangedPayload, HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    rig.bus.publish(&BusEvent::ActiveHarvestToolChanged(
        ActiveHarvestToolChangedPayload {
            tool_name: "Terratech PH-3".into(),
            cost_per_use_ped: 0.1,
            source: Some("hotbar:4".into()),
        },
    ));
    // A wood group is a swing, priced at the equipped tool's per-use cost.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![
            LootItem {
                item_name: "Short Moonleaf Board".into(),
                quantity: 9,
                value_ped: 0.09,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Wood Shavings".into(),
                quantity: 8,
                value_ped: 0.008,
                is_enhancer_shrapnel: false,
            },
        ],
        total_ped: 0.098,
    }));
    // The explicit failed swing costs the same decay.
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:04".into(),
    }));
    // A non-wood group still lands on the kill path.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:06".into()),
        items: vec![LootItem {
            item_name: "Animal Hide".into(),
            quantity: 1,
            value_ped: 1.0,
            is_enhancer_shrapnel: false,
        }],
        total_ped: 1.0,
    }));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 2, "one success + one fail");
        assert!(harvests[0].success);
        assert_eq!(harvests[0].tool_name.as_deref(), Some("Terratech PH-3"));
        assert_eq!(harvests[0].cost_ped, Ped(0.1));
        assert_eq!(harvests[0].loot_total_ped, Ped(0.098));
        assert_eq!(harvests[0].loot_items.len(), 2);
        assert!(!harvests[1].success);
        assert_eq!(harvests[1].cost_ped, Ped(0.1));
        assert!(harvests[1].loot_items.is_empty());
        assert_eq!(active.session.kills.len(), 1, "the hide group is a kill");
        assert!(
            active.warnings.is_empty(),
            "no no-tool warning when the tool is equipped"
        );
    });

    // Both swings persisted with their loot rows.
    let (events, items): (i64, i64) = rig
        .wait(rig.db.with_reader(|conn| {
            Ok((
                conn.query_row("SELECT COUNT(*) FROM harvest_events", [], |row| row.get(0))?,
                conn.query_row("SELECT COUNT(*) FROM harvest_loot_items", [], |row| {
                    row.get(0)
                })?,
            ))
        }))
        .unwrap();
    assert_eq!(events, 2);
    assert_eq!(items, 2);
}

#[test]
fn wood_loot_with_no_tool_records_zero_cost_and_warns_once() {
    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    for (ts, quantity) in [("2026-01-01T00:00:02", 9), ("2026-01-01T00:00:05", 7)] {
        rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
            kind: LootTag,
            timestamp: Some(ts.into()),
            items: vec![LootItem {
                item_name: "Short Moonleaf Board".into(),
                quantity,
                value_ped: 0.01 * quantity as f64,
                is_enhancer_shrapnel: false,
            }],
            total_ped: 0.01 * quantity as f64,
        }));
    }

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert_eq!(active.session.harvests.len(), 2);
        for harvest in &active.session.harvests {
            assert_eq!(harvest.tool_name, None);
            assert_eq!(harvest.cost_ped, Ped::ZERO, "never guess a cost");
        }
        assert_eq!(active.session.kills.len(), 0, "no phantom kill from wood");
        assert_eq!(
            active.warnings,
            vec!["Harvesting detected: no harvesting tool equipped via hotbar".to_string()],
            "the no-tool warning is one-shot"
        );
    });
}

/// The scripted guardrail used across the guardrail tests: the
/// PH-1/PH-3/PH-4 intent the tree-cutting loadout implies.
fn guardrail_providers() -> Providers {
    Providers {
        equipment: Arc::new(ScriptedEquipment {
            harvest_guardrail: Some(HarvestGuardrailTools {
                short: Some(GuardrailTool {
                    name: "Terratech PH-1 (L)".into(),
                    cost_per_use_ped: 0.02,
                }),
                long: Some(GuardrailTool {
                    name: "Terratech PH-3".into(),
                    cost_per_use_ped: 0.1,
                }),
                huge: Some(GuardrailTool {
                    name: "Terratech PH-4 (L)".into(),
                    cost_per_use_ped: 0.875,
                }),
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn equip_harvest_tool(rig: &Rig, name: &str, cost: f64) {
    use crate::bus_events::ActiveHarvestToolChangedPayload;
    rig.bus.publish(&BusEvent::ActiveHarvestToolChanged(
        ActiveHarvestToolChangedPayload {
            tool_name: name.into(),
            cost_per_use_ped: cost,
            source: Some("hotbar:4".into()),
        },
    ));
}

fn wood_group(ts: &str, board: Option<&str>) -> BusEvent {
    let mut items = vec![LootItem {
        item_name: "Wood Shavings".into(),
        quantity: 8,
        value_ped: 0.008,
        is_enhancer_shrapnel: false,
    }];
    if let Some(name) = board {
        items.push(LootItem {
            item_name: name.into(),
            quantity: 2,
            value_ped: 0.02,
            is_enhancer_shrapnel: false,
        });
    }
    let total_ped = items.iter().map(|item| item.value_ped).sum();
    BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some(ts.into()),
        items,
        total_ped,
    })
}

#[test]
fn tree_size_classification_reads_the_board_prefix() {
    use super::harvest::{tree_size_for_board, tree_size_for_group};

    assert_eq!(
        tree_size_for_board("Short Moonleaf Board"),
        Some(TreeSize::Short)
    );
    assert_eq!(tree_size_for_board("Moonleaf Board"), Some(TreeSize::Long));
    assert_eq!(
        tree_size_for_board("Long Kaisenbrandt Board"),
        Some(TreeSize::Huge)
    );
    // No space after the prefix word: a species name, not a size.
    assert_eq!(tree_size_for_board("Longleaf Board"), Some(TreeSize::Long));
    assert_eq!(tree_size_for_board("Wood Shavings"), None);
    assert_eq!(tree_size_for_board("Shrapnel"), None);

    let group = [
        LootItem {
            item_name: "Wood Shavings".into(),
            quantity: 3,
            value_ped: 0.003,
            is_enhancer_shrapnel: false,
        },
        LootItem {
            item_name: "Long Moonleaf Board".into(),
            quantity: 1,
            value_ped: 0.05,
            is_enhancer_shrapnel: false,
        },
    ];
    assert_eq!(tree_size_for_group(&group), Some(TreeSize::Huge));
    assert_eq!(tree_size_for_group(&group[..1]), None);
}

#[test]
fn guardrail_attributes_a_mismatched_swing_to_the_intended_tool() {
    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    // The hotbar believes the huge-tree tool; the evidence says short.
    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:04",
        Some("Short Moonleaf Board"),
    ));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert_eq!(active.session.harvests.len(), 2);
        for harvest in &active.session.harvests {
            assert_eq!(
                harvest.tool_name.as_deref(),
                Some("Terratech PH-1 (L)"),
                "the intended tool wins over the hotbar belief"
            );
            assert_eq!(harvest.cost_ped, Ped(0.02));
        }
        let mismatch = active
            .guardrail_mismatch
            .as_ref()
            .expect("the disagreement stands");
        assert_eq!(mismatch.expected_tool, "Terratech PH-1 (L)");
        assert_eq!(
            mismatch.observed_tool.as_deref(),
            Some("Terratech PH-4 (L)")
        );
        assert_eq!(mismatch.tree_size, TreeSize::Short);
        assert_eq!(active.warnings.len(), 1, "the warning is one-shot");
        assert!(active.warnings[0].starts_with("Harvest guardrail:"));
    });
}

#[test]
fn guardrail_agreement_and_hotbar_presses_clear_the_mismatch() {
    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert!(active.guardrail_mismatch.is_some());
    });

    // A fresh harvest-tool press re-syncs the belief and clears the cue.
    equip_harvest_tool(&rig, "Terratech PH-1 (L)", 0.02);
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert!(active.guardrail_mismatch.is_none());
    });

    // Agreeing evidence keeps it clear; disagreeing evidence re-arms it,
    // and a weapon press clears it again.
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:06",
        Some("Short Moonleaf Board"),
    ));
    rig.bus
        .publish(&wood_group("2026-01-01T00:00:08", Some("Moonleaf Board")));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let mismatch = active.guardrail_mismatch.as_ref().expect("re-armed");
        assert_eq!(mismatch.expected_tool, "Terratech PH-3");
        assert_eq!(mismatch.tree_size, TreeSize::Long);
    });
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Sollomate Opalo".into(),
            source: Some("hotbar:1".into()),
        }));
    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert!(
            active.guardrail_mismatch.is_none(),
            "a weapon press also re-syncs the belief"
        );
    });
}

#[test]
fn guardrail_falls_back_to_the_hotbar_belief_without_board_evidence() {
    use crate::bus_events::{HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    // A failed swing and a shavings-only success carry no evidence.
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:02".into(),
    }));
    rig.bus.publish(&wood_group("2026-01-01T00:00:04", None));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert_eq!(active.session.harvests.len(), 2);
        for harvest in &active.session.harvests {
            assert_eq!(harvest.tool_name.as_deref(), Some("Terratech PH-4 (L)"));
            assert_eq!(harvest.cost_ped, Ped(0.875));
        }
        assert!(active.guardrail_mismatch.is_none());
        assert!(active.warnings.is_empty());
    });
}

#[test]
fn guardrail_with_no_tool_equipped_stamps_the_intended_tool() {
    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvest = &active.session.harvests[0];
        assert_eq!(harvest.tool_name.as_deref(), Some("Terratech PH-1 (L)"));
        assert_eq!(harvest.cost_ped, Ped(0.02));
        let mismatch = active.guardrail_mismatch.as_ref().expect("flagged");
        assert_eq!(mismatch.observed_tool, None);
        assert_eq!(active.warnings.len(), 1);
        assert!(
            active.warnings[0].starts_with("Harvest guardrail:"),
            "the guardrail warning replaces the no-tool warning"
        );
    });
}

#[test]
fn a_standing_mismatch_prices_evidence_less_swings_by_the_expected_tool() {
    use crate::bus_events::{HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    // Board evidence arms the mismatch (short tree, PH-1 expected).
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));
    // While it stands, a fail and a shavings-only swing inherit PH-1.
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:04".into(),
    }));
    rig.bus.publish(&wood_group("2026-01-01T00:00:06", None));
    // A hotbar press clears the mismatch; a later fail follows the
    // fresh belief again.
    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:08".into(),
    }));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 4);
        for harvest in &harvests[1..3] {
            assert_eq!(
                harvest.tool_name.as_deref(),
                Some("Terratech PH-1 (L)"),
                "evidence-less swings inherit the standing mismatch's tool"
            );
            assert_eq!(harvest.cost_ped, Ped(0.02));
        }
        assert_eq!(
            harvests[3].tool_name.as_deref(),
            Some("Terratech PH-4 (L)"),
            "after the clearing press the belief stands again"
        );
        assert_eq!(harvests[3].cost_ped, Ped(0.875));
    });
}

#[test]
fn mismatch_setting_evidence_restamps_the_preceding_evidence_less_run() {
    use crate::bus_events::{HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-3", 0.1);
    // An agreeing long-tree run whose swings the belief was right
    // about: its trailing fail must never be rewritten.
    rig.bus
        .publish(&wood_group("2026-01-01T00:00:02", Some("Moonleaf Board")));
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:05".into(),
    }));
    // The desynced short-tree run, past the chain window: a fail and a
    // shavings-only swing before the first board drops, all still
    // believed PH-3.
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:50".into(),
    }));
    rig.bus.publish(&wood_group("2026-01-01T00:00:52", None));
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:54",
        Some("Short Moonleaf Board"),
    ));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 5);
        assert_eq!(
            harvests[1].tool_name.as_deref(),
            Some("Terratech PH-3"),
            "the fail beyond the chain window keeps its stamp"
        );
        assert_eq!(harvests[1].cost_ped, Ped(0.1));
        for harvest in &harvests[2..5] {
            assert_eq!(
                harvest.tool_name.as_deref(),
                Some("Terratech PH-1 (L)"),
                "the contiguous run before the evidence is re-stamped"
            );
            assert_eq!(harvest.cost_ped, Ped(0.02));
        }
    });

    // The re-stamp reached the persisted rows too.
    let rows: Vec<(Option<String>, f64)> = rig
        .wait(rig.db.with_reader(|conn| {
            let mut stmt =
                conn.prepare("SELECT tool_name, cost_ped FROM harvest_events ORDER BY timestamp")?;
            let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
        }))
        .unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[1], (Some("Terratech PH-3".into()), 0.1));
    for row in &rows[2..5] {
        assert_eq!(row, &(Some("Terratech PH-1 (L)".into()), 0.02));
    }
}

#[test]
fn an_unconfigured_tree_size_stays_outside_the_guardrail_remit() {
    // Only the short size carries an intent; long-tree evidence is
    // outside the guardrail's remit and must neither inherit a
    // standing short-tree mismatch nor trigger the retro pass.
    let providers = Providers {
        equipment: Arc::new(ScriptedEquipment {
            harvest_guardrail: Some(HarvestGuardrailTools {
                short: Some(GuardrailTool {
                    name: "Terratech PH-1 (L)".into(),
                    cost_per_use_ped: 0.02,
                }),
                long: None,
                huge: None,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let rig = rig();
    let tracker = rig.tracker(providers);
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    // Short-tree evidence arms the mismatch (expected PH-1).
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));
    // An evidence-less swing inherits the standing mismatch's tool.
    rig.bus.publish(&wood_group("2026-01-01T00:00:04", None));
    // Long-tree evidence: unconfigured size, so the belief stands and
    // nothing before it is re-stamped.
    rig.bus
        .publish(&wood_group("2026-01-01T00:00:06", Some("Moonleaf Board")));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 3);
        assert_eq!(harvests[0].tool_name.as_deref(), Some("Terratech PH-1 (L)"));
        assert_eq!(
            harvests[1].tool_name.as_deref(),
            Some("Terratech PH-1 (L)"),
            "the evidence-less swing inherited the standing mismatch"
        );
        assert_eq!(
            harvests[2].tool_name.as_deref(),
            Some("Terratech PH-4 (L)"),
            "the unconfigured size follows the belief, not the mismatch"
        );
        assert_eq!(harvests[2].cost_ped, Ped(0.875));
        assert!(
            active.guardrail_mismatch.is_some(),
            "the short-tree mismatch stays standing; the long swing proves nothing about it"
        );
    });
}

#[test]
fn the_retro_pass_never_reaches_back_past_a_hotbar_press() {
    use crate::bus_events::{HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    // A fail stamped under the PH-3 belief, then a press (belief
    // re-syncs to PH-4), then evidence contradicting the NEW belief.
    // The press is a boundary: the fail's stamp belongs to the earlier
    // belief regime and stays, even inside the chain window.
    equip_harvest_tool(&rig, "Terratech PH-3", 0.1);
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:02".into(),
    }));
    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:05",
        Some("Short Moonleaf Board"),
    ));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 2);
        assert_eq!(
            harvests[0].tool_name.as_deref(),
            Some("Terratech PH-3"),
            "the pre-press fail keeps its stamp"
        );
        assert_eq!(harvests[0].cost_ped, Ped(0.1));
        assert_eq!(harvests[1].tool_name.as_deref(), Some("Terratech PH-1 (L)"));
        assert!(active.guardrail_mismatch.is_some());
    });
}

#[test]
fn agreeing_evidence_never_restamps_preceding_swings() {
    use crate::bus_events::{HarvestFailPayload, HarvestFailTag};

    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    // A genuine long-tree fail on PH-3, then a legitimate move to a
    // short tree with a proper hotbar press: the agreeing short board
    // clears nothing and rewrites nothing.
    equip_harvest_tool(&rig, "Terratech PH-3", 0.1);
    rig.bus.publish(&BusEvent::HarvestFail(HarvestFailPayload {
        kind: HarvestFailTag,
        timestamp: "2026-01-01T00:00:02".into(),
    }));
    equip_harvest_tool(&rig, "Terratech PH-1 (L)", 0.02);
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:05",
        Some("Short Moonleaf Board"),
    ));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        let harvests = &active.session.harvests;
        assert_eq!(harvests.len(), 2);
        assert_eq!(harvests[0].tool_name.as_deref(), Some("Terratech PH-3"));
        assert_eq!(harvests[0].cost_ped, Ped(0.1));
        assert!(active.guardrail_mismatch.is_none());
    });
}

#[test]
fn the_snapshot_carries_the_guardrail_mismatch_view() {
    let rig = rig();
    let tracker = rig.tracker(guardrail_providers());
    rig.wait(tracker.start_session()).unwrap();

    equip_harvest_tool(&rig, "Terratech PH-4 (L)", 0.875);
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:02",
        Some("Short Moonleaf Board"),
    ));

    let readout = rig.wait(tracker.snapshot()).unwrap();
    let active = readout.active.expect("session is active");
    let mismatch = active
        .harvest_guardrail_mismatch
        .expect("the view carries the disagreement");
    assert_eq!(mismatch.expected_tool, "Terratech PH-1 (L)");
    assert_eq!(
        mismatch.observed_tool.as_deref(),
        Some("Terratech PH-4 (L)")
    );
    assert_eq!(mismatch.tree_size, "short");

    // Without a guardrail the view stays empty on the same evidence.
    let plain = rig.tracker(Providers::default());
    rig.wait(plain.start_session()).unwrap();
    rig.bus.publish(&wood_group(
        "2026-01-01T00:00:12",
        Some("Short Moonleaf Board"),
    ));
    let readout = rig.wait(plain.snapshot()).unwrap();
    assert!(readout
        .active
        .expect("session is active")
        .harvest_guardrail_mismatch
        .is_none());
}

#[test]
fn the_snapshot_current_tool_follows_the_hand_between_weapon_and_harvest() {
    use crate::bus_events::ActiveHarvestToolChangedPayload;

    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: Some("hotbar:1".into()),
        }));
    let (tool, _) = rig.wait(tracker.aggregate());
    assert_eq!(tool.as_deref(), Some("Rifle"));

    rig.bus.publish(&BusEvent::ActiveHarvestToolChanged(
        ActiveHarvestToolChangedPayload {
            tool_name: "Terratech PH-3".into(),
            cost_per_use_ped: 0.1,
            source: Some("hotbar:4".into()),
        },
    ));
    let (tool, _) = rig.wait(tracker.aggregate());
    assert_eq!(
        tool.as_deref(),
        Some("Terratech PH-3"),
        "a harvest equip takes the displayed hand item"
    );

    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: Some("hotbar:1".into()),
        }));
    let (tool, _) = rig.wait(tracker.aggregate());
    assert_eq!(
        tool.as_deref(),
        Some("Rifle"),
        "a weapon equip takes the hand back"
    );
}

#[test]
fn a_blacklisted_wood_group_still_routes_to_harvest_not_a_kill() {
    let rig = rig();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            blacklist: vec![
                "Wood Shavings".to_string(),
                "Short Moonleaf Board".to_string(),
            ],
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();

    // Every item filtered: the swing still happened (classification
    // reads the raw group), only the recorded loot is trimmed.
    rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
        kind: LootTag,
        timestamp: Some("2026-01-01T00:00:02".into()),
        items: vec![
            LootItem {
                item_name: "Short Moonleaf Board".into(),
                quantity: 9,
                value_ped: 0.09,
                is_enhancer_shrapnel: false,
            },
            LootItem {
                item_name: "Wood Shavings".into(),
                quantity: 8,
                value_ped: 0.008,
                is_enhancer_shrapnel: false,
            },
        ],
        total_ped: 0.098,
    }));

    rig.probe(&tracker, |actor| {
        let active = actor.session.active().expect("session is active");
        assert_eq!(active.session.kills.len(), 0, "no phantom kill");
        assert_eq!(active.session.harvests.len(), 1);
        assert!(active.session.harvests[0].loot_items.is_empty());
        assert_eq!(active.session.harvests[0].loot_total_ped, Ped::ZERO);
    });
}

#[test]
fn the_cumulative_net_history_includes_harvest_swings() {
    use crate::bus_events::ActiveHarvestToolChangedPayload;

    let rig = rig();
    let tracker = rig.tracker(Providers::default());
    rig.wait(tracker.start_session()).unwrap();

    rig.bus.publish(&BusEvent::ActiveHarvestToolChanged(
        ActiveHarvestToolChangedPayload {
            tool_name: "Terratech PH-1 (L)".into(),
            cost_per_use_ped: 0.02,
            source: Some("hotbar:4".into()),
        },
    ));
    for (ts, value) in [("2026-01-01T00:00:02", 0.1), ("2026-01-01T00:00:05", 0.06)] {
        rig.bus.publish(&BusEvent::LootGroup(LootGroupPayload {
            kind: LootTag,
            timestamp: Some(ts.into()),
            items: vec![LootItem {
                item_name: "Short Moonleaf Board".into(),
                quantity: 1,
                value_ped: value,
                is_enhancer_shrapnel: false,
            }],
            total_ped: value,
        }));
    }

    let (_, aggregate) = rig.wait(tracker.aggregate());
    let aggregate = aggregate.expect("active aggregate");
    // Two swings: +0.08, then +0.04 -> running 0.08, 0.12; the curve's
    // endpoint reconciles with the displayed Net (returns - cost).
    assert_eq!(aggregate.cumulative_net, vec![0.08, 0.12]);
    assert_eq!(
        (aggregate.returns - aggregate.cost)
            .round_half_even(2)
            .value(),
        0.12
    );
}

#[test]
fn a_weapon_equip_clears_the_harvest_hand_even_in_trifecta_mode() {
    use crate::bus_events::ActiveHarvestToolChangedPayload;

    let rig = rig();
    let tracker = rig.tracker(Providers {
        config: Arc::new(ScriptedConfig {
            trifecta_mode: true,
            ..Default::default()
        }),
        ..Providers::default()
    });
    rig.wait(tracker.start_session()).unwrap();

    rig.bus.publish(&BusEvent::ActiveHarvestToolChanged(
        ActiveHarvestToolChangedPayload {
            tool_name: "Terratech PH-1 (L)".into(),
            cost_per_use_ped: 0.02,
            source: Some("hotbar:4".into()),
        },
    ));
    rig.probe(&tracker, |actor| assert!(actor.hand_is_harvest));

    // The trifecta early-return must not preserve the stale hand flag.
    rig.bus
        .publish(&BusEvent::ActiveToolChanged(ActiveToolChangedPayload {
            tool_name: "Rifle".into(),
            source: Some("hotbar:1".into()),
        }));
    rig.probe(&tracker, |actor| assert!(!actor.hand_is_harvest));
}
