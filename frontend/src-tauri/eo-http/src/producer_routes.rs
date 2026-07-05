//! Native tracking snapshot assembly: the shared dashboard-hydration
//! computation, retained after the live tracking HTTP routes moved to the
//! typed-command facade (`eo_api::tracking`).
//!
//! The live producer routes (start/stop, the manual-mob and tag locks, the
//! snapshot GET) have migrated off the HTTP surface. What remains here is the
//! `TrackingSnapshot` value assembly ([`HydrationState::build_snapshot_value`])
//! and the helpers it needs, kept because the guide-mode demo snapshot
//! (`crate::demo`) reuses the identical assembly with a constructed demo config
//! and a fixed running state, keeping the two byte-for-byte aligned.

use std::sync::Arc;

use eo_services::config_service::{active_trifecta_preset, AppConfig};
use eo_services::db::DbError;
use eo_services::tracker::{to_iso_utc, HuntTracker};
use serde_json::{json, Map, Value};

use crate::hydration::HydrationState;

/// The `TrackingSnapshot` response-model field order (the polymorphic
/// dashboard hydration shape, served `exclude_unset`). The snake-case status
/// trio sits among the camelCase headline numbers exactly as the model
/// declares them; the projection emits whichever keys the active or idle
/// branch set, in this order.
const SNAPSHOT_FIELDS: [&str; 35] = [
    "status",
    "hotbarListenerActive",
    "weaponAttribution",
    "repairOcrEnabled",
    "endOfSessionArmourReminderEnabled",
    "mobEntryMode",
    "currentMob",
    "mobSource",
    "currentTool",
    "trifectaAttribution",
    "recentEvents",
    "session_id",
    "started_at",
    "kill_count",
    "elapsed",
    "cost",
    "returns",
    "pes",
    "net",
    "returnRate",
    "damageDealtTotal",
    "weaponDamageDealt",
    "weaponCost",
    "shotsFiredTotal",
    "criticalHitsTotal",
    "maxDamage",
    "globalsCount",
    "hofsCount",
    "latestKillLoot",
    "multiplierLast",
    "multiplierAvg",
    "multiplierMax",
    "multiplierHistory",
    "cumulativeNetHistory",
    "warnings",
];

/// Project a service value into a response model's field order, emitting
/// only the keys present in the value (Pydantic's `exclude_unset`). The
/// snapshot's polymorphic `exclude_unset` model carries no undeclared
/// top-level keys, so the present-keys-in-order rule reproduces its wire
/// shape exactly.
pub(crate) fn project(value: &Value, order: &[&str]) -> Value {
    let mut out = Map::new();
    if let Some(object) = value.as_object() {
        for &field in order {
            if let Some(found) = object.get(field) {
                out.insert(field.to_string(), found.clone());
            }
        }
    }
    Value::Object(out)
}

impl HydrationState {
    /// Assemble the `TrackingSnapshot` projected value from a tracker readout,
    /// a resolved config, and the hotbar listener's running state. Shared with
    /// the guide-mode demo snapshot, which reuses the identical assembly with a
    /// constructed demo config and a fixed running state (`true`), keeping the
    /// two byte-for-byte aligned.
    pub(crate) async fn build_snapshot_value(
        &self,
        tracker: &Arc<HuntTracker>,
        config: &AppConfig,
        hotbar_active: bool,
    ) -> Result<Value, DbError> {
        // `_weapon_attribution`: trifecta unless the hotbar hooks are on.
        let weapon_attribution = if config.hotbar_hooks_enabled {
            "hotbar"
        } else {
            "trifecta"
        };
        let trifecta_attribution = if weapon_attribution == "trifecta" {
            self.trifecta_attribution_summary(config).await?
        } else {
            Value::Null
        };
        let readout = tracker.snapshot()?;
        let current_tool = match &readout.current_tool {
            Some(tool) => Value::String(tool.clone()),
            None => Value::Null,
        };

        let value = match &readout.active {
            None => {
                // The configured manual label hydrates an idle dashboard.
                let (current_mob, mob_source) = configured_manual_label(config);
                json!({
                    "status": "idle",
                    "hotbarListenerActive": hotbar_active,
                    "weaponAttribution": weapon_attribution,
                    "repairOcrEnabled": config.repair_ocr_enabled,
                    "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                    "currentTool": current_tool,
                    "trifectaAttribution": trifecta_attribution,
                    "mobEntryMode": config.mob_tracking_mode,
                    "currentMob": current_mob,
                    "mobSource": mob_source,
                    "recentEvents": [],
                })
            }
            Some(active) => {
                let recent_events: Vec<Value> = active
                    .notable_event_rows
                    .iter()
                    .enumerate()
                    .map(|(index, (event_type, mob_or_item, value_ped, ts))| {
                        // Built in the NotableEvent declaration order with the
                        // extra `id` last, exactly as `extra="allow"` emits it.
                        json!({
                            "type": notable_event_category(event_type),
                            "description": notable_event_description(event_type, mob_or_item, *value_ped),
                            "value": *value_ped,
                            "eventType": event_type.clone(),
                            "timestamp": ts_to_iso(*ts),
                            "id": format!("ne-{index}"),
                        })
                    })
                    .collect();
                let warnings: Vec<Value> = active
                    .warnings
                    .iter()
                    // The tracker warning shares NotableEvent's required trio;
                    // its `value` is the model-coerced float zero.
                    .map(|message| json!({"type": "warning", "description": message, "value": 0.0}))
                    .collect();
                json!({
                    "status": "active",
                    "session_id": active.session_id.clone(),
                    "started_at": active.started_at.clone(),
                    "kill_count": active.kill_count,
                    "elapsed": active.elapsed,
                    "cost": active.cost,
                    "returns": active.returns,
                    "pes": active.pes,
                    "net": active.net,
                    "returnRate": active.return_rate,
                    "damageDealtTotal": active.damage_dealt_total,
                    "weaponDamageDealt": active.weapon_damage_dealt,
                    "weaponCost": active.weapon_cost,
                    "shotsFiredTotal": active.shots_fired_total,
                    "criticalHitsTotal": active.critical_hits_total,
                    "maxDamage": active.max_damage,
                    "globalsCount": active.globals_count,
                    "hofsCount": active.hofs_count,
                    "latestKillLoot": active.latest_kill_loot,
                    "multiplierLast": active.multiplier_last,
                    "multiplierAvg": active.multiplier_avg,
                    "multiplierMax": active.multiplier_max,
                    "multiplierHistory": active.multiplier_history.clone(),
                    "cumulativeNetHistory": active.cumulative_net_history.clone(),
                    "hotbarListenerActive": hotbar_active,
                    "weaponAttribution": weapon_attribution,
                    "repairOcrEnabled": config.repair_ocr_enabled,
                    "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                    "currentTool": current_tool,
                    "trifectaAttribution": trifecta_attribution,
                    "mobEntryMode": active.mob_entry_mode.clone(),
                    "currentMob": active.current_mob.clone(),
                    "mobSource": active.mob_source.clone(),
                    "recentEvents": recent_events,
                    "warnings": warnings,
                })
            }
        };
        Ok(project(&value, &SNAPSHOT_FIELDS))
    }

    /// `_trifecta_attribution_summary`: the active preset's bound weapon/heal
    /// names plus the preset list, or null when no preset exists and nothing
    /// is bound.
    async fn trifecta_attribution_summary(&self, config: &AppConfig) -> Result<Value, DbError> {
        let active = active_trifecta_preset(config);
        let small = active.and_then(|preset| preset.small_weapon_id);
        let big = active.and_then(|preset| preset.big_weapon_id);
        let heal = active.and_then(|preset| preset.heal_id);
        let presets: Vec<Value> = config
            .trifecta_presets
            .iter()
            .map(|preset| json!({"id": preset.id, "name": preset.name}))
            .collect();
        if presets.is_empty() && small.is_none() && big.is_none() && heal.is_none() {
            return Ok(Value::Null);
        }
        let mut summary = Map::new();
        summary.insert(
            "activePresetId".into(),
            match &config.active_trifecta_preset_id {
                Some(id) => Value::String(id.clone()),
                None => Value::Null,
            },
        );
        summary.insert(
            "presetName".into(),
            match active {
                Some(preset) => Value::String(preset.name.clone()),
                None => Value::Null,
            },
        );
        summary.insert("presets".into(), Value::Array(presets));
        summary.insert(
            "smallWeapon".into(),
            self.equipment_name(small, "weapon").await?,
        );
        summary.insert(
            "bigWeapon".into(),
            self.equipment_name(big, "weapon").await?,
        );
        summary.insert(
            "healTool".into(),
            self.equipment_name(heal, "healing").await?,
        );
        Ok(Value::Object(summary))
    }

    /// The equipment-library name for a bound id and type, or null when the id
    /// is unset or the row is absent.
    async fn equipment_name(&self, id: Option<i64>, item_type: &str) -> Result<Value, DbError> {
        let Some(id) = id else {
            return Ok(Value::Null);
        };
        match self.db.equipment_item(id, item_type).await? {
            Some((_id, name, _properties)) => Ok(Value::String(name)),
            None => Ok(Value::Null),
        }
    }
}

/// `_configured_manual_label`: the idle-state mob label and its source. Tag
/// mode reports the trimmed free-text tag (or none); manual mode reports the
/// stored species (with maturity) display (or none).
fn configured_manual_label(config: &AppConfig) -> (Value, Value) {
    if config.mob_tracking_mode == "tag" {
        let tag = config.mob_tracking_tag.trim();
        if tag.is_empty() {
            return (Value::Null, Value::Null);
        }
        return (
            Value::String(tag.to_string()),
            Value::String("tag".to_string()),
        );
    }
    let species = config.manual_mob_species.trim();
    let maturity = config.manual_mob_maturity.trim();
    if species.is_empty() {
        return (Value::Null, Value::Null);
    }
    let display = if maturity.is_empty() {
        species.to_string()
    } else {
        format!("{maturity} {species}")
    };
    (Value::String(display), Value::String("manual".to_string()))
}

/// `_ts_to_iso`: a Unix timestamp to an ISO 8601 UTC string (the same
/// `+00:00`-suffixed form the domain events stamp), or null.
fn ts_to_iso(ts: Option<f64>) -> Value {
    match ts {
        Some(ts) => Value::String(to_iso_utc(ts)),
        None => Value::Null,
    }
}

/// `_notable_event_category`: quest / HoF / global from the event-type prefix.
fn notable_event_category(event_type: &str) -> &'static str {
    if event_type.starts_with("quest_") {
        "quest"
    } else if event_type.starts_with("hof_") {
        "hof"
    } else {
        "global"
    }
}

/// `_notable_event_label`: the curated label for the known event types, else
/// the category title-cased (`HoF` kept as the special case).
fn notable_event_label(event_type: &str) -> String {
    match event_type {
        "global_kill" => "Global Kill".to_string(),
        "global_item" => "Global Item".to_string(),
        "hof_kill" => "HoF Kill".to_string(),
        "hof_item" => "HoF Item".to_string(),
        "quest_started" => "Quest Started".to_string(),
        "quest_completed" => "Quest Completed".to_string(),
        _ => {
            let category = notable_event_category(event_type);
            if category == "hof" {
                "HoF".to_string()
            } else {
                capitalize(category)
            }
        }
    }
}

/// `_notable_event_description`: the label with the mob or item, and the value
/// in PED for everything but the quest events.
fn notable_event_description(event_type: &str, mob_or_item: &str, value_ped: f64) -> String {
    let label = notable_event_label(event_type);
    if event_type.starts_with("quest_") {
        format!("{label}: {mob_or_item}")
    } else {
        format!("{label}: {mob_or_item} ({value_ped:.2} PED)")
    }
}

/// Python `str.capitalize` over an ASCII category: first letter upper, the
/// rest lower (the category words are already lower-case, so this upper-cases
/// the lead).
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eo_services::config_service::{AppConfig, TrifectaPresetConfig};

    #[test]
    fn notable_event_category_keys_on_the_prefix() {
        assert_eq!(notable_event_category("quest_started"), "quest");
        assert_eq!(notable_event_category("hof_kill"), "hof");
        assert_eq!(notable_event_category("global_item"), "global");
        assert_eq!(notable_event_category("anything_else"), "global");
    }

    #[test]
    fn notable_event_label_curates_the_known_types_then_title_cases() {
        assert_eq!(notable_event_label("global_kill"), "Global Kill");
        assert_eq!(notable_event_label("global_item"), "Global Item");
        assert_eq!(notable_event_label("hof_kill"), "HoF Kill");
        assert_eq!(notable_event_label("hof_item"), "HoF Item");
        assert_eq!(notable_event_label("quest_started"), "Quest Started");
        assert_eq!(notable_event_label("quest_completed"), "Quest Completed");
        // Unknown types fall back to the category title-case (HoF kept).
        assert_eq!(notable_event_label("hof_unknown"), "HoF");
        assert_eq!(notable_event_label("global_unknown"), "Global");
        assert_eq!(notable_event_label("quest_unknown"), "Quest");
    }

    #[test]
    fn notable_event_description_carries_the_value_except_for_quests() {
        assert_eq!(
            notable_event_description("global_kill", "Atrox Old", 12.5),
            "Global Kill: Atrox Old (12.50 PED)"
        );
        assert_eq!(
            notable_event_description("quest_completed", "Sweat Collector", 0.0),
            "Quest Completed: Sweat Collector"
        );
    }

    #[test]
    fn capitalize_upper_cases_the_lead() {
        assert_eq!(capitalize("global"), "Global");
        assert_eq!(capitalize("quest"), "Quest");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn ts_to_iso_renders_the_utc_string_or_null() {
        assert_eq!(ts_to_iso(None), Value::Null);
        let rendered = ts_to_iso(Some(1_780_000_000.0));
        let text = rendered.as_str().expect("a string instant");
        assert!(text.contains('T'), "got {text}");
        assert!(text.ends_with("+00:00"), "got {text}");
    }

    #[test]
    fn configured_manual_label_reports_tag_then_manual() {
        // Tag mode with a tag set.
        let config = AppConfig {
            mob_tracking_mode: "tag".into(),
            mob_tracking_tag: "Boss Hunt".into(),
            ..Default::default()
        };
        assert_eq!(
            configured_manual_label(&config),
            (json!("Boss Hunt"), json!("tag"))
        );
        // Tag mode, blank tag -> nulls.
        let config = AppConfig {
            mob_tracking_mode: "tag".into(),
            mob_tracking_tag: "  ".into(),
            ..Default::default()
        };
        assert_eq!(configured_manual_label(&config), (Value::Null, Value::Null));

        // Manual (non-tag) mode with a species + maturity.
        let config = AppConfig {
            mob_tracking_mode: "mob".into(),
            manual_mob_species: "Atrox".into(),
            manual_mob_maturity: "Old".into(),
            ..Default::default()
        };
        assert_eq!(
            configured_manual_label(&config),
            (json!("Old Atrox"), json!("manual"))
        );
        // No maturity -> the bare species.
        let config = AppConfig {
            mob_tracking_mode: "mob".into(),
            manual_mob_species: "Atrox".into(),
            ..Default::default()
        };
        assert_eq!(
            configured_manual_label(&config),
            (json!("Atrox"), json!("manual"))
        );
        // No species -> nulls.
        let config = AppConfig {
            mob_tracking_mode: "mob".into(),
            ..Default::default()
        };
        assert_eq!(configured_manual_label(&config), (Value::Null, Value::Null));
    }

    #[tokio::test]
    async fn trifecta_summary_resolves_bound_equipment_names() {
        let dir = tempfile::tempdir().unwrap();
        let db = eo_services::db::Db::open(&dir.path().join("entropia_orme.db"))
            .await
            .unwrap();
        // A bound small weapon; the big-weapon and heal slots stay unbound.
        sqlx::query(
            "INSERT INTO equipment_library (id, name, item_type, properties_json) \
             VALUES (5, 'Opalo', 'weapon', '{}')",
        )
        .execute(db.write())
        .await
        .unwrap();
        let game_data = std::sync::Arc::new(
            eo_services::game_data_store::GameDataStore::new(dir.path()).unwrap(),
        );
        let clock: std::sync::Arc<dyn eo_services::clock::Clock> =
            std::sync::Arc::new(eo_services::clock::MockClock::new(None, 0.0));
        let hydration = HydrationState::new(db, game_data, clock, dir.path().to_path_buf());

        let config = AppConfig {
            trifecta_presets: vec![TrifectaPresetConfig {
                id: "p1".into(),
                name: "Preset One".into(),
                small_weapon_id: Some(5),
                big_weapon_id: None,
                heal_id: None,
            }],
            active_trifecta_preset_id: Some("p1".into()),
            ..Default::default()
        };

        let summary = hydration
            .trifecta_attribution_summary(&config)
            .await
            .expect("the summary resolves");
        assert_eq!(summary["activePresetId"], "p1");
        assert_eq!(summary["presetName"], "Preset One");
        assert_eq!(summary["smallWeapon"], "Opalo");
        assert_eq!(summary["bigWeapon"], Value::Null);
        assert_eq!(summary["healTool"], Value::Null);
    }
}
