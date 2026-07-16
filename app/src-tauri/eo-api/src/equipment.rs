//! The equipment family: catalogue search, the library CRUD (including
//! the trifecta-reference delete guard), the expanded detail, and the
//! cost shaping behind both.
//!
//! The stored `properties_json` bytes are an owned on-disk contract:
//! the writes serialise with the canonical spacing the DB-state goldens
//! pin, so stored equipment state is byte-stable. The response
//! shapes match the frontend's hand-written contract (`$lib/types/
//! equipment.ts`) field for field; where the HTTP layer passed stored
//! JSON values through untyped, the DTOs pin the number/string types
//! the writes have always produced.

use eo_services::config_service::load_config_readonly;
use eo_services::cost_engine::{
    cost_per_shot_from_props, get_weapon_damage_profile, heal_cost_per_use, heal_reload_seconds,
    is_limited,
};
use eo_wire::normalizer::{round_half_even, to_python_json_dumps};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::Nullable;
use crate::{Api, ApiError};

/// A catalogue vocabulary the search accepts; each maps to one snapshot
/// endpoint. An out-of-vocabulary value cannot be constructed (the
/// bindings expose the closed union), so the old unknown-type reply
/// class is unrepresentable rather than handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchKind {
    Weapon,
    Amp,
    Healer,
    Scope,
    Absorber,
    Consumable,
    Tool,
}

impl SearchKind {
    fn endpoint(self) -> &'static str {
        match self {
            SearchKind::Weapon => "weapons",
            SearchKind::Amp => "weapon_amplifiers",
            SearchKind::Healer => "medical_tools",
            SearchKind::Scope => "weapon_vision_attachments",
            SearchKind::Absorber => "absorbers",
            SearchKind::Consumable => "stimulants",
            SearchKind::Tool => "harvesting_tools",
        }
    }
}

/// The stored equipment classes. `Tool` is a harvesting tool (tree
/// cutting): a single-entity item like a healing tool, costed as pure
/// decay per swing with no ammo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EquipmentKind {
    Weapon,
    Healing,
    Consumable,
    Tool,
}

impl EquipmentKind {
    fn as_str(self) -> &'static str {
        match self {
            EquipmentKind::Weapon => "weapon",
            EquipmentKind::Healing => "healing",
            EquipmentKind::Consumable => "consumable",
            EquipmentKind::Tool => "tool",
        }
    }
}

/// A catalogue search hit.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentSearchHit {
    pub catalog_id: Nullable<String>,
    pub name: String,
    /// Decay per use, PEC.
    pub decay: f64,
    /// Ammo burn per use, PEC (catalogue units / 100).
    pub ammo_burn: f64,
    pub is_limited: bool,
}

/// A library entry in the list shape.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: EquipmentKind,
    pub amplifier_name: Nullable<String>,
    pub cost_per_use: f64,
    pub damage_min: Nullable<f64>,
    pub damage_max: Nullable<f64>,
    pub reload_seconds: Nullable<f64>,
    pub is_limited: bool,
    /// 1 = base item, 2 = amplified, 3 = fully accessorised.
    pub enrichment_level: i64,
}

/// One configured component of a stored weapon setup.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentComponent {
    pub catalog_id: Nullable<String>,
    pub name: String,
    pub decay: f64,
    pub ammo_burn: f64,
    pub markup_percent: f64,
    pub is_limited: bool,
    pub damage_enhancers: i64,
}

/// The absorber component of a stored weapon setup.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AbsorberComponent {
    pub catalog_id: Nullable<String>,
    pub name: String,
    pub decay: f64,
    pub ammo_burn: f64,
    pub absorption_percent: f64,
    pub markup_percent: f64,
    pub is_limited: bool,
}

/// One line of a cost breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdownLine {
    pub component: String,
    pub cost_pec: f64,
    pub markup_multiplier: f64,
    pub effective_cost_pec: f64,
}

/// The expanded library-entry detail.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentDetail {
    pub id: String,
    pub weapon: EquipmentComponent,
    pub amplifier: Nullable<EquipmentComponent>,
    pub scope: Nullable<EquipmentComponent>,
    pub absorber: Nullable<AbsorberComponent>,
    pub cost_breakdown: Vec<CostBreakdownLine>,
    pub total_cost_per_use: f64,
}

/// An add or update request. Field names stay in the request casing the
/// frontend has always sent.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct EquipmentRequest {
    #[serde(rename = "type")]
    pub kind: EquipmentKind,
    #[serde(default)]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub amp_catalog_id: Option<String>,
    #[serde(default)]
    pub scope_catalog_id: Option<String>,
    #[serde(default)]
    pub absorber_catalog_id: Option<String>,
    #[serde(default = "default_markup")]
    pub weapon_markup: i64,
    #[serde(default = "default_markup")]
    pub amp_markup: i64,
    #[serde(default = "default_markup")]
    pub scope_markup: i64,
    #[serde(default = "default_markup")]
    pub absorber_markup: i64,
    #[serde(default)]
    pub damage_enhancers: i64,
}

fn default_markup() -> i64 {
    100
}

/// Python `str.strip()`: `char::is_whitespace` plus the four
/// information-separator controls (FS/GS/RS/US) Python's `isspace`
/// includes and Rust's does not.
fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// `entity.get(key) or 0.0` over a catalogue economy number.
fn eco_or_zero(entity: &Value, key: &str) -> f64 {
    entity
        .get("economy")
        .and_then(|eco| eco.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

/// Python truthiness over a JSON value.
fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python truthiness over an optional stored entity (`if props.get(..)`).
fn entity_truthy(props: &Value, key: &str) -> bool {
    props.get(key).map(json_truthy).unwrap_or(false)
}

/// Enrichment level from the configured components.
fn compute_enrichment(props: &Value) -> i64 {
    if entity_truthy(props, "amp_entity") {
        if entity_truthy(props, "scope_entity") || entity_truthy(props, "absorber_entity") {
            return 3;
        }
        return 2;
    }
    1
}

/// `max(0, int(props.get("damage_enhancers", 0) or 0))`.
fn stored_enhancers(props: &Value) -> i64 {
    props
        .get("damage_enhancers")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as i64
}

/// A stored identifier value as its string form (catalogue ids are
/// strings in the snapshot; a numeric id keeps its decimal rendering).
fn id_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A stored entity's display name; a stored setup missing it is
/// unreadable state, reported as the internal error the HTTP layer
/// answered for the same condition.
fn entity_name(entity: &Value) -> Result<String, ApiError> {
    entity
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ApiError::invalid_state("stored entity missing a string name"))
}

/// A stored markup value (`props[key] or 100`), as the number the
/// stored JSON carries.
fn stored_markup(props: &Value, key: &str) -> f64 {
    props.get(key).and_then(Value::as_f64).unwrap_or(100.0)
}

/// A catalogue search row as its typed hit.
fn search_hit(row: &Value) -> EquipmentSearchHit {
    let entity = &row["data"];
    EquipmentSearchHit {
        catalog_id: id_string(&row["item_id"]).into(),
        name: row["item_name"].as_str().unwrap_or_default().to_string(),
        decay: eco_or_zero(entity, "decay"),
        ammo_burn: eco_or_zero(entity, "ammo_burn") / 100.0,
        is_limited: is_limited(entity),
    }
}

/// The typed cost lines from the cost engine's breakdown value.
fn breakdown_lines(cost_result: &Value) -> Result<Vec<CostBreakdownLine>, ApiError> {
    serde_json::from_value(cost_result["costBreakdown"].clone())
        .map_err(ApiError::internal("cost-breakdown shaping"))
}

/// Convert a library row to the list shape. The internal error mirrors
/// the unreadable-row condition the HTTP layer answered a 500 for (a
/// weapon or healing row missing its stored entity).
fn row_to_summary(
    id: i64,
    name: &str,
    item_type: &str,
    props: &Value,
) -> Result<EquipmentSummary, ApiError> {
    if item_type == "weapon" {
        let weapon_e = props
            .get("weapon_entity")
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                ApiError::invalid_state(format!("stored weapon row {id} missing weapon_entity"))
            })?;
        let amp_e = props.get("amp_entity").filter(|v| !v.is_null());
        let enhancers = stored_enhancers(props).max(0);
        let cost_result = cost_per_shot_from_props(props, None);
        let damage_profile = get_weapon_damage_profile(weapon_e, amp_e, enhancers);
        let rounded = |key: &str| -> Option<f64> {
            damage_profile
                .as_ref()
                .and_then(|profile| profile.get(key))
                .and_then(Value::as_f64)
                .map(|v| round_half_even(v, 2))
        };
        return Ok(EquipmentSummary {
            id: id.to_string(),
            name: name.to_string(),
            kind: EquipmentKind::Weapon,
            amplifier_name: amp_e
                .and_then(|amp| amp.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .into(),
            cost_per_use: cost_result["totalCostPerUse"].as_f64().unwrap_or(0.0),
            damage_min: rounded("damageMin").into(),
            damage_max: rounded("damageMax").into(),
            reload_seconds: None.into(),
            is_limited: is_limited(weapon_e),
            enrichment_level: compute_enrichment(props),
        });
    }

    if item_type == "consumable" {
        return Ok(EquipmentSummary {
            id: id.to_string(),
            name: name.to_string(),
            kind: EquipmentKind::Consumable,
            amplifier_name: None.into(),
            cost_per_use: 0.0,
            damage_min: None.into(),
            damage_max: None.into(),
            reload_seconds: None.into(),
            is_limited: false,
            enrichment_level: 1,
        });
    }

    if item_type == "tool" {
        let tool_e = props
            .get("tool_entity")
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                ApiError::invalid_state(format!("stored tool row {id} missing tool_entity"))
            })?;
        let markup = props.get("markup").and_then(Value::as_f64).unwrap_or(100.0) / 100.0;
        // A harvesting tool has no ammo, so the heal per-use recipe
        // (decay + ammo, markup-weighted) reduces to decay x markup.
        return Ok(EquipmentSummary {
            id: id.to_string(),
            name: name.to_string(),
            kind: EquipmentKind::Tool,
            amplifier_name: None.into(),
            cost_per_use: heal_cost_per_use(tool_e, markup),
            damage_min: None.into(),
            damage_max: None.into(),
            reload_seconds: None.into(),
            is_limited: is_limited(tool_e),
            enrichment_level: 1,
        });
    }

    let tool_e = props
        .get("tool_entity")
        .filter(|v| !v.is_null())
        .ok_or_else(|| {
            ApiError::invalid_state(format!("stored healing row {id} missing tool_entity"))
        })?;
    let markup = props.get("markup").and_then(Value::as_f64).unwrap_or(100.0) / 100.0;
    Ok(EquipmentSummary {
        id: id.to_string(),
        name: name.to_string(),
        kind: EquipmentKind::Healing,
        amplifier_name: None.into(),
        cost_per_use: heal_cost_per_use(tool_e, markup),
        damage_min: None.into(),
        damage_max: None.into(),
        reload_seconds: Some(round_half_even(heal_reload_seconds(tool_e), 2)).into(),
        is_limited: is_limited(tool_e),
        enrichment_level: 1,
    })
}

/// Convert a library row to the expanded detail shape. `catalog_id` is
/// the row's own column.
fn row_to_detail(
    id: i64,
    name: &str,
    item_type: &str,
    catalog_id: Option<&str>,
    props: &Value,
) -> Result<EquipmentDetail, ApiError> {
    let item_id = id.to_string();

    if item_type == "weapon" {
        let weapon_e = props
            .get("weapon_entity")
            .filter(|v| !v.is_null())
            .ok_or_else(|| {
                ApiError::invalid_state(format!("stored weapon row {id} missing weapon_entity"))
            })?;
        let enhancers = stored_enhancers(props).max(0);
        let cost_result = cost_per_shot_from_props(props, None);

        let component = |entity_key: &str,
                         id_key: &str,
                         markup_key: &str|
         -> Result<Option<EquipmentComponent>, ApiError> {
            match props.get(entity_key).filter(|v| !v.is_null()) {
                Some(entity) => Ok(Some(EquipmentComponent {
                    catalog_id: props.get(id_key).and_then(id_string).into(),
                    name: entity_name(entity)?,
                    decay: eco_or_zero(entity, "decay"),
                    ammo_burn: eco_or_zero(entity, "ammo_burn") / 100.0,
                    markup_percent: stored_markup(props, markup_key),
                    is_limited: is_limited(entity),
                    damage_enhancers: 0,
                })),
                None => Ok(None),
            }
        };

        let absorber = match props.get("absorber_entity").filter(|v| !v.is_null()) {
            Some(absorber_e) => {
                let absorption_pct = absorber_e
                    .get("economy")
                    .and_then(|eco| eco.get("absorption"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    * 100.0;
                Some(AbsorberComponent {
                    catalog_id: props.get("absorber_catalog_id").and_then(id_string).into(),
                    name: entity_name(absorber_e)?,
                    decay: eco_or_zero(absorber_e, "decay"),
                    ammo_burn: eco_or_zero(absorber_e, "ammo_burn") / 100.0,
                    absorption_percent: round_half_even(absorption_pct, 1),
                    markup_percent: stored_markup(props, "absorber_markup"),
                    is_limited: is_limited(absorber_e),
                })
            }
            None => None,
        };

        // `props.get("weapon_catalog_id") or <row catalog_id>`.
        let weapon_catalog_id = match props.get("weapon_catalog_id").filter(|v| json_truthy(v)) {
            Some(value) => id_string(value),
            None => catalog_id.map(str::to_string),
        };
        return Ok(EquipmentDetail {
            id: item_id,
            weapon: EquipmentComponent {
                catalog_id: weapon_catalog_id.into(),
                name: entity_name(weapon_e)?,
                decay: eco_or_zero(weapon_e, "decay"),
                ammo_burn: eco_or_zero(weapon_e, "ammo_burn") / 100.0,
                markup_percent: stored_markup(props, "weapon_markup"),
                is_limited: is_limited(weapon_e),
                damage_enhancers: enhancers,
            },
            amplifier: component("amp_entity", "amp_catalog_id", "amp_markup")?.into(),
            scope: component("scope_entity", "scope_catalog_id", "scope_markup")?.into(),
            absorber: absorber.into(),
            cost_breakdown: breakdown_lines(&cost_result)?,
            total_cost_per_use: cost_result["totalCostPerUse"].as_f64().unwrap_or(0.0),
        });
    }

    if item_type == "consumable" {
        return Ok(EquipmentDetail {
            id: item_id,
            weapon: EquipmentComponent {
                catalog_id: catalog_id.map(str::to_string).into(),
                name: name.to_string(),
                decay: 0.0,
                ammo_burn: 0.0,
                markup_percent: 100.0,
                is_limited: false,
                damage_enhancers: 0,
            },
            amplifier: None.into(),
            scope: None.into(),
            absorber: None.into(),
            cost_breakdown: Vec::new(),
            total_cost_per_use: 0.0,
        });
    }

    // Healing tool detail (simplified).
    let tool_e = props
        .get("tool_entity")
        .filter(|v| !v.is_null())
        .ok_or_else(|| {
            ApiError::invalid_state(format!("stored healing row {id} missing tool_entity"))
        })?;
    let markup_pct = stored_markup(props, "markup");
    let cost = heal_cost_per_use(tool_e, markup_pct / 100.0);
    let decay = eco_or_zero(tool_e, "decay");
    let mut breakdown = vec![CostBreakdownLine {
        component: "Decay".to_string(),
        cost_pec: decay,
        markup_multiplier: markup_pct / 100.0,
        effective_cost_pec: round_half_even(decay * markup_pct / 100.0, 4),
    }];
    let ammo_pec = eco_or_zero(tool_e, "ammo_burn") / 100.0;
    if ammo_pec > 0.0 {
        breakdown.push(CostBreakdownLine {
            component: "Ammo".to_string(),
            cost_pec: ammo_pec,
            markup_multiplier: 1.0,
            effective_cost_pec: ammo_pec,
        });
    }
    // `props.get("tool_catalog_id") or <row catalog_id>`.
    let tool_catalog_id = match props.get("tool_catalog_id").filter(|v| json_truthy(v)) {
        Some(value) => id_string(value),
        None => catalog_id.map(str::to_string),
    };
    Ok(EquipmentDetail {
        id: item_id,
        weapon: EquipmentComponent {
            catalog_id: tool_catalog_id.into(),
            name: entity_name(tool_e)?,
            decay,
            ammo_burn: ammo_pec,
            markup_percent: markup_pct,
            is_limited: is_limited(tool_e),
            damage_enhancers: 0,
        },
        amplifier: None.into(),
        scope: None.into(),
        absorber: None.into(),
        cost_breakdown: breakdown,
        total_cost_per_use: cost,
    })
}

/// The stored props built for an add/update request.
struct BuiltProps {
    name: String,
    stored_catalog_id: Option<String>,
    props: Value,
}

impl Api {
    /// Catalogue search: substring match by display name over one
    /// vocabulary. Queries under two characters answer empty before any
    /// lookup.
    pub async fn equipment_search(
        &self,
        q: &str,
        kind: SearchKind,
    ) -> Result<Vec<EquipmentSearchHit>, ApiError> {
        if q.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let rows = self.game_data.search_entities(q, Some(kind.endpoint()), 50);
        Ok(rows.iter().map(search_hit).collect())
    }

    /// The stored library, oldest first.
    pub async fn equipment_library(&self) -> Result<Vec<EquipmentSummary>, ApiError> {
        let rows = self
            .db
            .equipment_library_rows()
            .await
            .map_err(ApiError::internal("equipment library read"))?;
        let mut results = Vec::with_capacity(rows.len());
        for (id, name, item_type, raw_props) in rows {
            results.push(summary_from_parts(id, &name, &item_type, &raw_props)?);
        }
        Ok(results)
    }

    /// Store a new library entry.
    pub async fn equipment_add(
        &self,
        req: &EquipmentRequest,
    ) -> Result<EquipmentSummary, ApiError> {
        let built = self.build_props(req)?;
        let props_json = to_python_json_dumps(&built.props);
        let inserted = self
            .db
            .insert_equipment(
                built.name,
                req.kind.as_str().to_string(),
                built.stored_catalog_id,
                props_json,
            )
            .await
            .map_err(ApiError::internal("equipment insert"))?;
        let Some((id, name, item_type, raw_props)) = self
            .db
            .equipment_row(inserted)
            .await
            .map_err(ApiError::internal("inserted equipment read-back"))?
        else {
            // The row was just inserted; its absence is a driver-level
            // invariant break.
            return Err(ApiError::invalid_state(
                "inserted equipment read-back failed",
            ));
        };
        summary_from_parts(id, &name, &item_type, &raw_props)
    }

    /// Replace a stored entry's configuration; its class is fixed.
    pub async fn equipment_update(
        &self,
        item_id: i64,
        req: &EquipmentRequest,
    ) -> Result<EquipmentSummary, ApiError> {
        let existing_type: Option<String> = self
            .db
            .equipment_item_type(item_id)
            .await
            .map_err(ApiError::internal("equipment row lookup"))?;
        let Some(existing_type) = existing_type else {
            return Err(ApiError::not_found(format!(
                "Equipment item {item_id} not found"
            )));
        };
        if existing_type != req.kind.as_str() {
            return Err(ApiError::bad_request("Cannot change equipment type"));
        }
        let built = self.build_props(req)?;
        let props_json = to_python_json_dumps(&built.props);
        self.db
            .update_equipment(item_id, built.name, built.stored_catalog_id, props_json)
            .await
            .map_err(ApiError::internal("equipment update"))?;
        let Some((id, name, item_type, raw_props)) = self
            .db
            .equipment_row(item_id)
            .await
            .map_err(ApiError::internal("updated equipment read-back"))?
        else {
            // The row's existence was checked before the update; its
            // absence is a driver-level invariant break.
            return Err(ApiError::invalid_state(
                "updated equipment read-back failed",
            ));
        };
        summary_from_parts(id, &name, &item_type, &raw_props)
    }

    /// Delete a stored entry; refused while a trifecta preset references
    /// it. Idempotent over a missing row.
    pub async fn equipment_delete(&self, item_id: i64) -> Result<(), ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("settings read for the delete guard"))?;
        let referenced = config.trifecta_presets.iter().any(|preset| {
            [preset.small_weapon_id, preset.big_weapon_id, preset.heal_id].contains(&Some(item_id))
        });
        if referenced {
            return Err(ApiError::conflict(
                "Cannot remove equipment selected in a trifecta preset",
            ));
        }
        self.db
            .delete_equipment(item_id)
            .await
            .map_err(ApiError::internal("equipment delete"))?;
        Ok(())
    }

    /// The expanded detail for a stored entry.
    pub async fn equipment_detail(&self, item_id: i64) -> Result<EquipmentDetail, ApiError> {
        let row = self
            .db
            .equipment_detail_row(item_id)
            .await
            .map_err(ApiError::internal("equipment detail lookup"))?;
        let Some((id, name, item_type, catalog_id, raw_props)) = row else {
            return Err(ApiError::not_found(format!(
                "Equipment item {item_id} not found"
            )));
        };
        let props = serde_json::from_str::<Value>(&raw_props)
            .map_err(ApiError::internal("stored equipment props parse"))?;
        row_to_detail(id, &name, &item_type, catalog_id.as_deref(), &props)
    }

    /// `_fetch_entity`: catalogue lookup with the not-found contract.
    fn fetch_entity(&self, endpoint: &str, item_id: &str) -> Result<Value, ApiError> {
        let id_value = Value::String(item_id.to_string());
        match self.game_data.find_entity(endpoint, &id_value) {
            Some(entity) => Ok(entity.clone()),
            None => Err(ApiError::not_found(format!(
                "Entity '{item_id}' not found in catalogue endpoint '{endpoint}'."
            ))),
        }
    }

    /// Build the stored props for an add/update request, reproducing
    /// the route-order validation (missing catalogue id refusals,
    /// catalogue misses, the consumable identity rule).
    fn build_props(&self, req: &EquipmentRequest) -> Result<BuiltProps, ApiError> {
        match req.kind {
            EquipmentKind::Weapon => {
                let catalog_id = req
                    .catalog_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| ApiError::bad_request("catalog_id required for weapon"))?;
                let weapon_e = self.fetch_entity("weapons", catalog_id)?;
                let optional = |endpoint: &str, id: Option<&str>| -> Result<Value, ApiError> {
                    match id.filter(|v| !v.is_empty()) {
                        Some(id) => self.fetch_entity(endpoint, id),
                        None => Ok(Value::Null),
                    }
                };
                let amp_e = optional("weapon_amplifiers", req.amp_catalog_id.as_deref())?;
                let scope_e =
                    optional("weapon_vision_attachments", req.scope_catalog_id.as_deref())?;
                let absorber_e = optional("absorbers", req.absorber_catalog_id.as_deref())?;
                let name = weapon_e["name"].as_str().unwrap_or_default().to_string();
                let mut props = Map::new();
                props.insert("weapon_entity".into(), weapon_e);
                props.insert("weapon_catalog_id".into(), json!(catalog_id));
                props.insert("amp_entity".into(), amp_e);
                props.insert("amp_catalog_id".into(), json!(req.amp_catalog_id));
                props.insert("scope_entity".into(), scope_e);
                props.insert("scope_catalog_id".into(), json!(req.scope_catalog_id));
                props.insert("absorber_entity".into(), absorber_e);
                props.insert("absorber_catalog_id".into(), json!(req.absorber_catalog_id));
                props.insert("weapon_markup".into(), json!(req.weapon_markup));
                props.insert("amp_markup".into(), json!(req.amp_markup));
                props.insert("scope_markup".into(), json!(req.scope_markup));
                props.insert("absorber_markup".into(), json!(req.absorber_markup));
                props.insert(
                    "damage_enhancers".into(),
                    json!(req.damage_enhancers.max(0)),
                );
                Ok(BuiltProps {
                    name,
                    stored_catalog_id: Some(catalog_id.to_string()),
                    props: Value::Object(props),
                })
            }
            EquipmentKind::Healing => {
                let catalog_id = req
                    .catalog_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| ApiError::bad_request("catalog_id required for healing"))?;
                let tool_e = self.fetch_entity("medical_tools", catalog_id)?;
                let name = tool_e["name"].as_str().unwrap_or_default().to_string();
                let mut props = Map::new();
                props.insert("tool_entity".into(), tool_e);
                props.insert("tool_catalog_id".into(), json!(catalog_id));
                props.insert("markup".into(), json!(req.weapon_markup));
                Ok(BuiltProps {
                    name,
                    stored_catalog_id: Some(catalog_id.to_string()),
                    props: Value::Object(props),
                })
            }
            EquipmentKind::Tool => {
                let catalog_id = req
                    .catalog_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| ApiError::bad_request("catalog_id required for tool"))?;
                let tool_e = self.fetch_entity("harvesting_tools", catalog_id)?;
                let name = tool_e["name"].as_str().unwrap_or_default().to_string();
                let mut props = Map::new();
                props.insert("tool_entity".into(), tool_e);
                props.insert("tool_catalog_id".into(), json!(catalog_id));
                props.insert("markup".into(), json!(req.weapon_markup));
                Ok(BuiltProps {
                    name,
                    stored_catalog_id: Some(catalog_id.to_string()),
                    props: Value::Object(props),
                })
            }
            EquipmentKind::Consumable => {
                // Catalogue pick or free-text name.
                if let Some(catalog_id) = req.catalog_id.as_deref().filter(|id| !id.is_empty()) {
                    let entity = self.fetch_entity("stimulants", catalog_id)?;
                    let name = entity["name"].as_str().unwrap_or_default().to_string();
                    let mut props = Map::new();
                    props.insert("catalog_id".into(), json!(catalog_id));
                    props.insert("entity".into(), entity);
                    return Ok(BuiltProps {
                        name,
                        stored_catalog_id: Some(catalog_id.to_string()),
                        props: Value::Object(props),
                    });
                }
                if let Some(name) = req
                    .name
                    .as_deref()
                    .map(py_strip)
                    .filter(|name| !name.is_empty())
                {
                    let mut props = Map::new();
                    props.insert("catalog_id".into(), Value::Null);
                    props.insert("entity".into(), Value::Null);
                    return Ok(BuiltProps {
                        name: name.to_string(),
                        stored_catalog_id: None,
                        props: Value::Object(props),
                    });
                }
                Err(ApiError::bad_request(
                    "Consumable requires either catalog_id (catalogue pick) or name (custom)",
                ))
            }
        }
    }
}

/// Shape one library row (id, name, item_type, properties_json) to the
/// list form.
fn summary_from_parts(
    id: i64,
    name: &str,
    item_type: &str,
    raw_props: &str,
) -> Result<EquipmentSummary, ApiError> {
    let props = serde_json::from_str::<Value>(raw_props)
        .map_err(ApiError::internal("stored equipment props parse"))?;
    row_to_summary(id, name, item_type, &props)
}
