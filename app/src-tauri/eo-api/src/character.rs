//! The character family: calibration status, stats, skills, professions,
//! the Prospect forecast family, and the profession / path / HP
//! optimizers, all computed from the calibrated skill levels plus the
//! bundled game-data catalogue through the calculation services
//! (`eo-services`).
//!
//! The family is read-only: no stored bytes change. The response shapes
//! match the frontend's hand-written contract (`$lib/types/analytics.ts`)
//! field for field, expressed directly by the DTOs' declared field order
//! and `f64` typing; no separate projection pass exists.
//!
//! Contract lineage (ADR-0017/0019): two response shapes moved
//! deliberately at the typed-command crossing: the Prospect and path-optimizer *not-found* soft
//! errors converge on their families' full error shape (they were a
//! minimal three-key body), and the Prospect `sample` drops the internal
//! `skillShares` / `attributeRates` maps it leaked (computation
//! intermediates no consumer read). The legacy `GET /api/character/codex`
//! skill-progress list retires unconverted: it has no frontend caller,
//! exactly as the equipment cost endpoint retired with its family.

use eo_services::character_calc::{
    all_profession_levels, effective_points, hp_skill_optimizer, is_attribute, profession_level,
    profession_path_optimizer, profession_skill_optimizer, skill_rank,
};
use eo_services::db::DbError;
use eo_services::time::{naive_to_epoch, to_iso_utc};
use eo_services::tt_value_curve::{levels_for_tt_value, tt_value_at};
use eo_wire::normalizer::round_half_even;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Api, ApiError};

/// Skills are considered stale after 30 days without recalibration.
const STALE_DAYS: f64 = 30.0;
const PROSPECT_SAMPLE_WARN_SESSIONS: i64 = 3;
const PROSPECT_SAMPLE_WARN_HOURS: f64 = 2.0;
const PROSPECT_SAMPLE_WARN_CYCLED_PED: f64 = 50.0;

// ── Query arguments ─────────────────────────────────────────────────

/// The slice a Prospect forecast aggregates over. A closed vocabulary:
/// the bindings expose only these four, so the old out-of-vocabulary 422
/// is unrepresentable rather than validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProspectSliceType {
    Global,
    Tag,
    Mob,
    Weapon,
}

impl ProspectSliceType {
    fn as_str(self) -> &'static str {
        match self {
            ProspectSliceType::Global => "global",
            ProspectSliceType::Tag => "tag",
            ProspectSliceType::Mob => "mob",
            ProspectSliceType::Weapon => "weapon",
        }
    }
}

/// The Prospect forecast query. `sliceValue` is required for every slice
/// but `global`; `markupUplift` defaults to zero.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProspectQuery {
    pub profession: String,
    pub target_level: f64,
    pub slice_type: ProspectSliceType,
    #[serde(default)]
    pub slice_value: Option<String>,
    #[serde(default)]
    pub markup_uplift: f64,
}

// ── Response DTOs ───────────────────────────────────────────────────

/// GET calibration: whether skills are calibrated and how fresh.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationStatus {
    pub calibrated: bool,
    pub last_calibration: Option<String>,
    pub stale: bool,
}

/// One of the top professions on the stats card: the trimmed shape the
/// card renders (name, level, category), not the full profession row.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatProfession {
    pub name: String,
    pub level: f64,
    pub category: String,
}

/// GET stats: current HP and the top five professions.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComputedCharacterStats {
    pub hp: i64,
    pub top_professions: Vec<StatProfession>,
}

/// One calibrated skill row.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillLevel {
    pub name: String,
    pub category: String,
    pub level: f64,
    pub anchor_level: Option<f64>,
    pub gain_since_anchor: Option<f64>,
    pub rank_name: String,
    pub tt_value: f64,
    pub is_attribute: bool,
}

/// One profession row.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfessionLevel {
    pub name: String,
    pub level: f64,
    pub anchor_level: Option<f64>,
    pub gain_since_anchor: Option<f64>,
    pub category: String,
}

/// One grouped Prospect slice option.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProspectOption {
    pub value: String,
    pub label: String,
    pub sessions: i64,
    pub kills: i64,
    pub hours: f64,
    pub cycled_ped: f64,
}

/// GET prospect-options: the grouped slice options, one list per axis.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterProspectOptions {
    pub tags: Vec<ProspectOption>,
    pub mobs: Vec<ProspectOption>,
    pub weapons: Vec<ProspectOption>,
}

/// The observed sample a Prospect forecast projects from. The internal
/// `skillShares` / `attributeRates` maps are computed but not surfaced
/// (no consumer reads them; dropped with the migration).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProspectSample {
    pub sessions: i64,
    pub kills: i64,
    pub hours: f64,
    pub cycled_ped: f64,
    pub loot_tt: f64,
    pub pes: f64,
    pub attribute_levels: f64,
    pub cycled_per_hour: f64,
    pub loot_per_hour: f64,
    pub return_rate: f64,
    pub pes_per_ped: f64,
    pub loot_tt_per_ped: f64,
}

/// One skill/attribute row of a Prospect forecast.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProspectRow {
    pub name: String,
    pub is_attribute: bool,
    pub weight: f64,
    pub current_level: f64,
    pub observed_share: f64,
    pub observed_rate: f64,
    pub projected_gain: f64,
    pub projected_end_level: f64,
    pub profession_contribution: f64,
    pub relevant: bool,
}

/// GET prospect: the forecast. `error` is present only on the soft-error
/// paths (the frontend renders it inline rather than throwing); every
/// other field is always present, so the declared order below is the
/// wire order.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProspectResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub rows: Vec<ProspectRow>,
    pub warnings: Vec<String>,
    pub profession: String,
    pub slice_type: String,
    pub slice_value: Option<String>,
    pub markup_uplift: f64,
    pub current_level: f64,
    pub target_level: f64,
    pub projected_cycled_ped: f64,
    pub projected_hours: f64,
    pub expected_loot_tt: f64,
    pub expected_net_tt_burn: f64,
    pub speculative_loot_tt: Option<f64>,
    pub speculative_net_tt_burn: Option<f64>,
    pub sample: ProspectSample,
}

/// One skill row of the profession optimizer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerSkill {
    pub name: String,
    pub weight: f64,
    pub current_level: f64,
    pub levels_needed: f64,
    pub ped_to_next_level: f64,
    pub codex_category: Option<String>,
    pub codex_divisor: Option<f64>,
}

/// One attribute row of the profession / path optimizer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerAttribute {
    pub name: String,
    pub weight: f64,
    pub current_level: f64,
    pub contribution_factor: f64,
}

/// GET profession-optimizer: the cheapest-skill breakdown to the next
/// profession level. On a missing profession the declared tail fields
/// stay unset and only `error` accompanies the empty lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfessionOptimizerResult {
    pub skills: Vec<OptimizerSkill>,
    pub attributes: Vec<OptimizerAttribute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profession: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One allocation of the path optimizer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathAllocation {
    pub name: String,
    pub weight: f64,
    pub current_level: f64,
    pub levels_to_gain: f64,
    pub ped_cost: f64,
    pub new_level: f64,
    pub codex_category: Option<String>,
    pub codex_divisor: Option<f64>,
}

/// A skill the path optimizer left out, with the reason.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedSkill {
    pub name: String,
    pub weight: f64,
    pub reason: String,
}

/// GET profession-path-optimizer: the greedy allocation for a target
/// level or a PED budget. `inputTargetLevel` / `inputPedBudget` echo the
/// mode (exactly one is non-null); `error` marks a missing profession.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathOptimizerResult {
    pub allocations: Vec<PathAllocation>,
    pub attributes: Vec<OptimizerAttribute>,
    pub profession: String,
    pub mode: String,
    pub input_target_level: Option<f64>,
    pub input_ped_budget: Option<f64>,
    pub current_level: f64,
    pub end_level: f64,
    pub profession_levels_gained: f64,
    pub total_ped: f64,
    pub excluded: Vec<ExcludedSkill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One skill row of the HP optimizer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HpOptimizerSkill {
    pub name: String,
    pub hp_increase: f64,
    pub current_level: f64,
    pub levels_per_hp: f64,
    pub ped_per_hp: f64,
    pub hp_per_ped: f64,
    pub codex_category: Option<String>,
    pub codex_divisor: Option<f64>,
}

/// One attribute row of the HP optimizer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HpOptimizerAttribute {
    pub name: String,
    pub hp_increase: f64,
    pub current_level: f64,
    pub levels_per_hp: f64,
}

/// GET hp-optimizer: the HP-per-PED breakdown across contributing
/// skills and attributes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HpOptimizerResult {
    pub current_hp: f64,
    pub skills: Vec<HpOptimizerSkill>,
    pub attributes: Vec<HpOptimizerAttribute>,
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// Calibration status: believed-latest calibration timestamp and its
    /// staleness against the injected clock.
    pub async fn character_calibration(&self) -> Result<CalibrationStatus, ApiError> {
        let last_ts = self
            .last_calibration_ts()
            .await
            .map_err(ApiError::internal("calibration timestamp read"))?;
        let Some(last_ts) = last_ts else {
            return Ok(CalibrationStatus {
                calibrated: false,
                last_calibration: None,
                stale: true,
            });
        };
        let age_days = (naive_to_epoch(self.clock.now()) - last_ts) / 86400.0;
        Ok(CalibrationStatus {
            calibrated: true,
            last_calibration: Some(to_iso_utc(last_ts)),
            stale: age_days > STALE_DAYS,
        })
    }

    /// Current HP (Python `int()` truncation of the `Health` skill) and
    /// the top five professions by level.
    pub async fn character_stats(&self) -> Result<ComputedCharacterStats, ApiError> {
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("stats skill calibrations"))?;
        let hp = skill_levels
            .get("Health")
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as i64;

        let professions_data = self.game_data.get_entities("professions");
        let levels_by_name = all_profession_levels(&skill_levels, professions_data);
        let mut top_professions: Vec<StatProfession> = Vec::new();
        for prof in professions_data {
            let Some(name) = prof.get("name").and_then(Value::as_str) else {
                continue;
            };
            let level = levels_by_name
                .get(name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if level > 0.0 {
                top_professions.push(StatProfession {
                    name: name.to_string(),
                    level,
                    category: prof
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("General")
                        .to_string(),
                });
            }
        }
        sort_desc_by(&mut top_professions, |p| p.level);
        top_professions.truncate(5);
        Ok(ComputedCharacterStats {
            hp,
            top_professions,
        })
    }

    /// The calibrated skills, believed-current levels with scan-anchored
    /// gains, ranks, and TT valuations, ordered by level descending.
    pub async fn character_skills(&self) -> Result<Vec<SkillLevel>, ApiError> {
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("skills skill calibrations"))?;
        if skill_levels.is_empty() {
            return Ok(Vec::new());
        }
        let anchor_levels = self
            .skill_calibrations(Some("scan"))
            .await
            .map_err(ApiError::internal("skills anchor calibrations"))?;
        let skills_data = self.game_data.get_entities("skills");
        let ranks = get_ranks(&self.game_data);

        let mut result: Vec<SkillLevel> = Vec::new();
        for (name, level_value) in &skill_levels {
            let level = level_value.as_f64().unwrap_or(0.0);
            let entity = skills_data
                .iter()
                .find(|s| s.get("name").and_then(Value::as_str) == Some(name.as_str()));
            let category = entity
                .and_then(|e| e.get("category"))
                .filter(|c| json_truthy(c))
                .and_then(Value::as_object)
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("General")
                .to_string();
            let anchor = anchor_levels.get(name).and_then(Value::as_f64);
            let gain = anchor.map(|a| round_half_even(level - a, 4));
            result.push(SkillLevel {
                name: name.clone(),
                category,
                level,
                anchor_level: anchor,
                gain_since_anchor: gain,
                rank_name: skill_rank(level, &ranks),
                tt_value: round_half_even(tt_value_at(level), 2),
                is_attribute: is_attribute(name),
            });
        }
        sort_desc_by(&mut result, |s| s.level);
        Ok(result)
    }

    /// The professions, believed-current levels with scan-anchored
    /// gains, ordered by level descending.
    pub async fn character_professions(&self) -> Result<Vec<ProfessionLevel>, ApiError> {
        let professions_data = self.game_data.get_entities("professions");
        if professions_data.is_empty() {
            return Ok(Vec::new());
        }
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("professions skill calibrations"))?;
        let anchor_skills = self
            .skill_calibrations(Some("scan"))
            .await
            .map_err(ApiError::internal("professions anchor calibrations"))?;
        let current_levels = all_profession_levels(&skill_levels, professions_data);
        let anchor_levels = all_profession_levels(&anchor_skills, professions_data);
        let has_anchor = !anchor_skills.is_empty();

        let mut result: Vec<ProfessionLevel> = Vec::new();
        for prof in professions_data {
            let Some(name) = prof.get("name").and_then(Value::as_str) else {
                continue;
            };
            let level = current_levels
                .get(name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let anchor = if has_anchor {
                Some(
                    anchor_levels
                        .get(name)
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                )
            } else {
                None
            };
            let gain = anchor.map(|a| round_half_even(level - a, 4));
            result.push(ProfessionLevel {
                name: name.to_string(),
                level,
                anchor_level: anchor,
                gain_since_anchor: gain,
                category: prof
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("General")
                    .to_string(),
            });
        }
        sort_desc_by(&mut result, |p| p.level);
        Ok(result)
    }

    /// The grouped Prospect slice options over the recorded sessions.
    pub async fn character_prospect_options(&self) -> Result<CharacterProspectOptions, ApiError> {
        let sessions = eo_services::session_summary::load_prospect_sessions(&self.db)
            .await
            .map_err(ApiError::internal("prospect-options sessions load"))?;
        Ok(CharacterProspectOptions {
            tags: prospect_options(&sessions, "dominantTag")?,
            mobs: prospect_options(&sessions, "dominantMob")?,
            weapons: prospect_options(&sessions, "dominantWeapon")?,
        })
    }

    /// The Prospect forecast for a profession and target level over an
    /// observed session slice.
    pub async fn character_prospect(
        &self,
        query: &ProspectQuery,
    ) -> Result<ProspectResult, ApiError> {
        // The value validations performed before dispatch; the slice-type
        // vocabulary is a closed enum, so an out-of-vocabulary slice is
        // unrepresentable rather than checked.
        if query.target_level <= 0.0 {
            return Err(ApiError::bad_request("target_level must be positive"));
        }
        if query.markup_uplift < 0.0 {
            return Err(ApiError::bad_request(
                "markup_uplift must be zero or positive",
            ));
        }
        let slice_type = query.slice_type.as_str();
        if query.slice_type != ProspectSliceType::Global
            && query.slice_value.as_deref().is_none_or(str::is_empty)
        {
            return Err(ApiError::bad_request(
                "slice_value is required for non-global slices",
            ));
        }

        let profession_entity = self
            .game_data
            .get_entities("professions")
            .iter()
            .find(|prof| {
                prof.get("name").and_then(Value::as_str) == Some(query.profession.as_str())
            })
            .cloned();
        let Some(profession_entity) = profession_entity else {
            // A missing profession converges on the full error shape
            // (was a minimal {error, rows, warnings}); ratified.
            let shape = prospect_error_shape(
                &query.profession,
                slice_type,
                &query.slice_value,
                query.markup_uplift,
                0.0,
                query.target_level,
                prospect_sample(&[]),
                &format!("Profession '{}' not found", query.profession),
            );
            return serde_json::from_value(shape)
                .map_err(ApiError::internal("prospect not-found shaping"));
        };
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("prospect skill calibrations"))?;
        let sessions = eo_services::session_summary::load_prospect_sessions(&self.db)
            .await
            .map_err(ApiError::internal("prospect sessions load"))?;
        let matched = match_prospect_sessions(&sessions, slice_type, &query.slice_value);
        let sample = prospect_sample(&matched);
        let result = build_prospect_result(
            &query.profession,
            &profession_entity,
            &skill_levels,
            query.target_level,
            sample,
            slice_type,
            &query.slice_value,
            query.markup_uplift,
        );
        serde_json::from_value(result).map_err(ApiError::internal("prospect shaping"))
    }

    /// The profession optimizer: cheapest skill allocation to the next
    /// profession level.
    pub async fn character_profession_optimizer(
        &self,
        profession: &str,
    ) -> Result<ProfessionOptimizerResult, ApiError> {
        let prof_entity = self
            .game_data
            .get_entities("professions")
            .iter()
            .find(|p| p.get("name").and_then(Value::as_str) == Some(profession))
            .cloned();
        let Some(prof_entity) = prof_entity else {
            return Ok(ProfessionOptimizerResult {
                skills: Vec::new(),
                attributes: Vec::new(),
                profession: None,
                current_level: None,
                next_level: None,
                gap: None,
                error: Some(format!("Profession '{profession}' not found")),
            });
        };
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("optimizer skill calibrations"))?;
        let mut result = profession_skill_optimizer(&skill_levels, &prof_entity);
        if let Some(map) = result.as_object_mut() {
            map.insert("profession".into(), json!(profession));
        }
        serde_json::from_value(result).map_err(ApiError::internal("optimizer shaping"))
    }

    /// The path optimizer: greedy allocation for a target level or a PED
    /// budget (exactly one supplied).
    pub async fn character_path_optimizer(
        &self,
        profession: &str,
        target_level: Option<f64>,
        ped_budget: Option<f64>,
    ) -> Result<PathOptimizerResult, ApiError> {
        // The mode contract, validated before dispatch.
        if target_level.is_none() == ped_budget.is_none() {
            return Err(ApiError::bad_request(
                "Exactly one of target_level or ped_budget must be provided",
            ));
        }
        let prof_entity = self
            .game_data
            .get_entities("professions")
            .iter()
            .find(|p| p.get("name").and_then(Value::as_str) == Some(profession))
            .cloned();
        let Some(prof_entity) = prof_entity else {
            // A missing profession converges on the full error shape
            // (was a minimal {allocations, attributes, error}); ratified.
            let mode = if target_level.is_some() {
                "target"
            } else {
                "budget"
            };
            let shape = json!({
                "allocations": [],
                "attributes": [],
                "profession": profession,
                "mode": mode,
                "inputTargetLevel": target_level,
                "inputPedBudget": ped_budget,
                "currentLevel": 0.0,
                "endLevel": 0.0,
                "professionLevelsGained": 0.0,
                "totalPed": 0.0,
                "excluded": [],
                "error": format!("Profession '{profession}' not found"),
            });
            return serde_json::from_value(shape)
                .map_err(ApiError::internal("path optimizer not-found shaping"));
        };
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("path optimizer skill calibrations"))?;
        // The mode contract is validated above; a service-level rejection
        // here is unreachable.
        let mut result =
            profession_path_optimizer(&skill_levels, &prof_entity, target_level, ped_budget)
                .map_err(|_| ApiError::Internal)?;
        if let Some(map) = result.as_object_mut() {
            map.insert("profession".into(), json!(profession));
        }
        serde_json::from_value(result).map_err(ApiError::internal("path optimizer shaping"))
    }

    /// The HP optimizer: HP-per-PED across contributing skills and
    /// attributes.
    pub async fn character_hp_optimizer(&self) -> Result<HpOptimizerResult, ApiError> {
        let skill_levels = self
            .skill_calibrations(None)
            .await
            .map_err(ApiError::internal("hp optimizer skill calibrations"))?;
        let skills_data = self.game_data.get_entities("skills");
        let result = hp_skill_optimizer(&skill_levels, skills_data);
        serde_json::from_value(result).map_err(ApiError::internal("hp optimizer shaping"))
    }

    /// Latest calibrated level per skill: believed-current when `source`
    /// is None, the scan anchor when `source='scan'`, mirroring
    /// `_get_skill_calibrations` (the `MAX(scanned_at)` / `MAX(id)`
    /// tiebreaker SQL verbatim).
    async fn skill_calibrations(
        &self,
        source: Option<&str>,
    ) -> Result<Map<String, Value>, DbError> {
        let source = source.map(str::to_string);
        let rows: Vec<(String, f64)> = self
            .db
            .with_reader(move |conn| match source {
                None => {
                    let mut stmt = conn.prepare(
                        "WITH latest_ts AS (\n                        SELECT skill_name, MAX(scanned_at) AS ts\n                        FROM skill_calibrations\n                        GROUP BY skill_name\n                    )\n                    SELECT skill_name, level FROM skill_calibrations\n                    WHERE id IN (\n                        SELECT MAX(s2.id) FROM skill_calibrations s2\n                        JOIN latest_ts m ON s2.skill_name = m.skill_name AND s2.scanned_at = m.ts\n                        GROUP BY s2.skill_name\n                    )",
                    )?;
                    let mapped = stmt.query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?;
                    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
                }
                Some(source) => {
                    let mut stmt = conn.prepare(
                        "WITH latest_ts AS (\n                        SELECT skill_name, MAX(scanned_at) AS ts\n                        FROM skill_calibrations\n                        WHERE source = ?\n                        GROUP BY skill_name\n                    )\n                    SELECT skill_name, level FROM skill_calibrations\n                    WHERE id IN (\n                        SELECT MAX(s2.id) FROM skill_calibrations s2\n                        JOIN latest_ts m ON s2.skill_name = m.skill_name AND s2.scanned_at = m.ts\n                        WHERE s2.source = ?\n                        GROUP BY s2.skill_name\n                    )",
                    )?;
                    let mapped = stmt.query_map(rusqlite::params![source, source], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?;
                    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
                }
            })
            .await?;
        let mut levels = Map::new();
        for (name, level) in rows {
            levels.insert(name, json!(level));
        }
        Ok(levels)
    }

    /// Epoch timestamp of the most recent calibration, or None.
    async fn last_calibration_ts(&self) -> Result<Option<f64>, DbError> {
        self.db
            .with_reader(|conn| {
                Ok(conn.query_row(
                    "SELECT MAX(scanned_at) as ts FROM skill_calibrations",
                    [],
                    |row| row.get::<_, Option<f64>>(0),
                )?)
            })
            .await
    }
}

// ── Shaping helpers ─────────────────────────────────────────────────

/// The grouped slice options for one dominant-value key, as typed rows.
fn prospect_options(sessions: &[Value], key: &str) -> Result<Vec<ProspectOption>, ApiError> {
    serde_json::from_value(Value::Array(prospect_option_list(sessions, key)))
        .map_err(ApiError::internal("prospect options shaping"))
}

/// Stable descending sort by a float key (Python `sort(reverse=True)`).
fn sort_desc_by<T>(items: &mut [T], key: impl Fn(&T) -> f64) {
    items.sort_by(|a, b| key(b).partial_cmp(&key(a)).expect("levels are finite"));
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

/// Sorted `{name, skill}` rank thresholds from the catalogue.
fn get_ranks(game_data: &eo_services::game_data_store::GameDataStore) -> Vec<Value> {
    let entities = game_data.get_entities("skill_ranks");
    let Some(first) = entities.first() else {
        return Vec::new();
    };
    let rows = first
        .get("table")
        .and_then(|t| t.get("rows"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut valid: Vec<Value> = Vec::new();
    for row in rows {
        let Some(threshold) = row.get("skill").and_then(Value::as_f64) else {
            continue;
        };
        let Some(name) = row.get("name").filter(|n| !n.is_null()) else {
            continue;
        };
        valid.push(json!({"name": name, "skill": threshold}));
    }
    valid.sort_by(|a, b| {
        let left = a["skill"].as_f64().unwrap_or(0.0);
        let right = b["skill"].as_f64().unwrap_or(0.0);
        left.partial_cmp(&right).expect("thresholds are finite")
    });
    valid
}

// ── Prospect (ported helpers, Value-producing) ──────────────────────

/// Aggregate a session group into the Prospect sample shape. The
/// `skillShares` / `attributeRates` maps are kept for the forecast's own
/// use; the response DTO drops them.
fn prospect_sample(sessions: &[&Value]) -> Map<String, Value> {
    let mut regular_skill_ped: Map<String, Value> = Map::new();
    let mut attribute_levels: Map<String, Value> = Map::new();

    // Python's `sum(())` is the INTEGER zero (rendered `0`), and a
    // non-empty sum starts from it, so the float result carries IEEE
    // positive zero; Rust's empty f64 sum folds from -0.0 instead, so
    // the empty case takes the integer literally.
    let sum_of = |key: &str| -> Value {
        if sessions.is_empty() {
            return json!(0);
        }
        let total: f64 = sessions
            .iter()
            .map(|s| s.get(key).and_then(Value::as_f64).unwrap_or(0.0))
            .sum();
        json!(round_half_even(total, 4))
    };
    let kills: i64 = sessions
        .iter()
        .map(|s| s.get("kills").and_then(Value::as_i64).unwrap_or(0))
        .sum();

    let mut sample = Map::new();
    sample.insert("sessions".into(), json!(sessions.len()));
    sample.insert("kills".into(), json!(kills));
    sample.insert("hours".into(), sum_of("durationHours"));
    sample.insert("cycledPed".into(), sum_of("cycledPed"));
    sample.insert("lootTt".into(), sum_of("lootTt"));
    sample.insert("pes".into(), sum_of("regularSkillTt"));
    sample.insert("attributeLevels".into(), sum_of("attributeLevelsTotal"));

    for session in sessions {
        if let Some(map) = session.get("regularSkillPed").and_then(Value::as_object) {
            for (name, ped) in map {
                let current = regular_skill_ped
                    .get(name)
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                regular_skill_ped
                    .insert(name.clone(), json!(current + ped.as_f64().unwrap_or(0.0)));
            }
        }
        if let Some(map) = session.get("attributeLevels").and_then(Value::as_object) {
            for (name, amount) in map {
                let current = attribute_levels
                    .get(name)
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                attribute_levels.insert(
                    name.clone(),
                    json!(current + amount.as_f64().unwrap_or(0.0)),
                );
            }
        }
    }

    let hours = sample["hours"].as_f64().unwrap_or(0.0);
    let cycled = sample["cycledPed"].as_f64().unwrap_or(0.0);
    let loot_tt = sample["lootTt"].as_f64().unwrap_or(0.0);
    let pes = sample["pes"].as_f64().unwrap_or(0.0);
    sample.insert(
        "cycledPerHour".into(),
        json!(if hours > 0.0 {
            round_half_even(cycled / hours, 4)
        } else {
            0.0
        }),
    );
    sample.insert(
        "lootPerHour".into(),
        json!(if hours > 0.0 {
            round_half_even(loot_tt / hours, 4)
        } else {
            0.0
        }),
    );
    sample.insert(
        "returnRate".into(),
        json!(if cycled > 0.0 {
            round_half_even(loot_tt / cycled, 4)
        } else {
            0.0
        }),
    );
    sample.insert(
        "pesPerPed".into(),
        json!(if cycled > 0.0 {
            round_half_even(pes / cycled, 6)
        } else {
            0.0
        }),
    );
    sample.insert(
        "lootTtPerPed".into(),
        json!(if cycled > 0.0 {
            round_half_even(loot_tt / cycled, 6)
        } else {
            0.0
        }),
    );

    let mut skill_shares = Map::new();
    for (name, ped) in &regular_skill_ped {
        let ped = ped.as_f64().unwrap_or(0.0);
        if pes > 0.0 && ped > 0.0 {
            skill_shares.insert(name.clone(), json!(ped / pes));
        }
    }
    sample.insert("skillShares".into(), Value::Object(skill_shares));

    let mut attribute_rates = Map::new();
    for (name, amount) in &attribute_levels {
        let amount = amount.as_f64().unwrap_or(0.0);
        if cycled > 0.0 && amount > 0.0 {
            attribute_rates.insert(name.clone(), json!(amount / cycled));
        }
    }
    sample.insert("attributeRates".into(), Value::Object(attribute_rates));
    sample
}

/// The grouped option list for one dominant-value key.
fn prospect_option_list(sessions: &[Value], key: &str) -> Vec<Value> {
    let mut grouped: Map<String, Value> = Map::new();
    for session in sessions {
        let Some(value) = session
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        grouped
            .entry(value.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("group lists are arrays")
            .push(session.clone());
    }

    let mut options: Vec<Value> = Vec::new();
    for (value, group) in &grouped {
        let members: Vec<&Value> = group.as_array().expect("array").iter().collect();
        let sample = prospect_sample(&members);
        options.push(json!({
            "value": value,
            "label": value,
            "sessions": sample["sessions"],
            "kills": sample["kills"],
            "hours": round_half_even(sample["hours"].as_f64().unwrap_or(0.0), 2),
            "cycledPed": round_half_even(sample["cycledPed"].as_f64().unwrap_or(0.0), 2),
        }));
    }

    options.sort_by(|a, b| {
        let sessions_cmp = b["sessions"].as_i64().cmp(&a["sessions"].as_i64());
        if sessions_cmp != std::cmp::Ordering::Equal {
            return sessions_cmp;
        }
        let cycled_cmp = b["cycledPed"]
            .as_f64()
            .partial_cmp(&a["cycledPed"].as_f64())
            .expect("cycled values are finite");
        if cycled_cmp != std::cmp::Ordering::Equal {
            return cycled_cmp;
        }
        a["label"].as_str().cmp(&b["label"].as_str())
    });
    options
}

/// Filter sessions to a slice (`global` passes everything through).
fn match_prospect_sessions<'s>(
    sessions: &'s [Value],
    slice_type: &str,
    slice_value: &Option<String>,
) -> Vec<&'s Value> {
    if slice_type == "global" {
        return sessions.iter().collect();
    }
    let Some(slice_value) = slice_value.as_deref().filter(|v| !v.is_empty()) else {
        return Vec::new();
    };
    let key = match slice_type {
        "tag" => "dominantTag",
        "mob" => "dominantMob",
        "weapon" => "dominantWeapon",
        _ => return Vec::new(),
    };
    sessions
        .iter()
        .filter(|session| session.get(key).and_then(Value::as_str) == Some(slice_value))
        .collect()
}

fn build_prospect_warnings(sample: &Map<String, Value>, projected_cycled_ped: f64) -> Vec<Value> {
    let mut warnings = Vec::new();
    if sample["sessions"].as_i64().unwrap_or(0) < PROSPECT_SAMPLE_WARN_SESSIONS {
        warnings.push(json!("Thin sample: fewer than 3 matching sessions."));
    }
    if sample["hours"].as_f64().unwrap_or(0.0) < PROSPECT_SAMPLE_WARN_HOURS {
        warnings.push(json!("Thin sample: less than 2 hours of matching play."));
    }
    let cycled = sample["cycledPed"].as_f64().unwrap_or(0.0);
    if cycled < PROSPECT_SAMPLE_WARN_CYCLED_PED {
        warnings.push(json!("Thin sample: less than 50 PED of matching cycling."));
    }
    if cycled > 0.0 && projected_cycled_ped > cycled * 20.0 {
        warnings.push(json!(
            "Long extrapolation: forecast extends far beyond the observed sample."
        ));
    }
    warnings
}

/// Project skill levels after cycling `total_ped` through the sample's
/// observed composition: (projected levels, projected gains).
fn project_prospect_levels(
    skill_levels: &Map<String, Value>,
    sample: &Map<String, Value>,
    total_ped: f64,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut projected_levels: Map<String, Value> = skill_levels
        .iter()
        .map(|(name, level)| (name.clone(), json!(level.as_f64().unwrap_or(0.0))))
        .collect();
    let mut projected_gains: Map<String, Value> = Map::new();

    let pes_per_ped = sample["pesPerPed"].as_f64().unwrap_or(0.0);
    let skill_tt_budget = total_ped * pes_per_ped;
    if let Some(shares) = sample["skillShares"].as_object() {
        for (skill_name, share) in shares {
            let current = projected_levels
                .get(skill_name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let allocated_tt = skill_tt_budget * share.as_f64().unwrap_or(0.0);
            let gained = levels_for_tt_value(current, allocated_tt);
            projected_levels.insert(
                skill_name.clone(),
                json!(round_half_even(current + gained, 4)),
            );
            projected_gains.insert(skill_name.clone(), json!(round_half_even(gained, 4)));
        }
    }
    if let Some(rates) = sample["attributeRates"].as_object() {
        for (skill_name, rate) in rates {
            let current = projected_levels
                .get(skill_name)
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let gained = total_ped * rate.as_f64().unwrap_or(0.0);
            projected_levels.insert(
                skill_name.clone(),
                json!(round_half_even(current + gained, 4)),
            );
            projected_gains.insert(skill_name.clone(), json!(round_half_even(gained, 4)));
        }
    }
    (projected_levels, projected_gains)
}

/// Whether the observed sample contains gains that move the profession.
fn relevant_prospect_progress(sample: &Map<String, Value>, profession: &Value) -> bool {
    let observed_regular = sample["skillShares"].as_object();
    let observed_attrs = sample["attributeRates"].as_object();
    let skills = profession
        .get("skills")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for entry in skills {
        let name = entry
            .get("skill")
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let weight = entry.get("weight").and_then(Value::as_f64).unwrap_or(0.0);
        if name.is_empty() || weight <= 0.0 {
            continue;
        }
        if observed_regular.is_some_and(|m| m.contains_key(name))
            || observed_attrs.is_some_and(|m| m.contains_key(name))
        {
            return true;
        }
    }
    false
}

/// An early-return prospect shape (error before any forecast values).
#[allow(clippy::too_many_arguments)]
fn prospect_error_shape(
    profession_name: &str,
    slice_type: &str,
    slice_value: &Option<String>,
    markup_uplift: f64,
    current_level: f64,
    target_level: f64,
    sample: Map<String, Value>,
    error: &str,
) -> Value {
    json!({
        "profession": profession_name,
        "sliceType": slice_type,
        "sliceValue": slice_value,
        "markupUplift": markup_uplift,
        "currentLevel": round_half_even(current_level, 2),
        "targetLevel": round_half_even(target_level, 2),
        "projectedCycledPed": 0.0,
        "projectedHours": 0.0,
        "expectedLootTt": 0.0,
        "expectedNetTtBurn": 0.0,
        "speculativeLootTt": null,
        "speculativeNetTtBurn": null,
        "sample": sample,
        "rows": [],
        "warnings": [],
        "error": error,
    })
}

/// The full forecast, mirroring `_build_prospect_result` (including the
/// doubling search and 60-step bisection over projected cycling).
#[allow(clippy::too_many_arguments)]
fn build_prospect_result(
    profession_name: &str,
    profession: &Value,
    skill_levels: &Map<String, Value>,
    target_level: f64,
    sample: Map<String, Value>,
    slice_type: &str,
    slice_value: &Option<String>,
    markup_uplift: f64,
) -> Value {
    let current_level = profession_level(skill_levels, profession);

    let projected_levels: Map<String, Value>;
    let mut projected_gains: Map<String, Value> = Map::new();
    let projected_cycled_ped: f64;

    if target_level <= current_level {
        projected_levels = skill_levels
            .iter()
            .map(|(name, level)| (name.clone(), json!(level.as_f64().unwrap_or(0.0))))
            .collect();
        projected_cycled_ped = 0.0;
    } else {
        let cycled = sample["cycledPed"].as_f64().unwrap_or(0.0);
        let hours = sample["hours"].as_f64().unwrap_or(0.0);
        if cycled <= 0.0 || hours <= 0.0 {
            return prospect_error_shape(
                profession_name,
                slice_type,
                slice_value,
                markup_uplift,
                current_level,
                target_level,
                sample,
                "Insufficient matching data for a forecast.",
            );
        }
        if !relevant_prospect_progress(&sample, profession) {
            return prospect_error_shape(
                profession_name,
                slice_type,
                slice_value,
                markup_uplift,
                current_level,
                target_level,
                sample,
                "The observed sample does not contain gains that move this profession.",
            );
        }

        let mut lower = 0.0_f64;
        let mut upper = cycled.max(1.0);
        let mut upper_level = profession_level(
            &project_prospect_levels(skill_levels, &sample, upper).0,
            profession,
        );
        while upper_level < target_level && upper < 1_000_000_000.0 {
            lower = upper;
            upper *= 2.0;
            upper_level = profession_level(
                &project_prospect_levels(skill_levels, &sample, upper).0,
                profession,
            );
        }
        if upper_level < target_level {
            return prospect_error_shape(
                profession_name,
                slice_type,
                slice_value,
                markup_uplift,
                current_level,
                target_level,
                sample,
                "Target is outside the reachable forecast range for this sample.",
            );
        }
        for _ in 0..60 {
            let mid = (lower + upper) / 2.0;
            let (test_levels, _) = project_prospect_levels(skill_levels, &sample, mid);
            if profession_level(&test_levels, profession) >= target_level {
                upper = mid;
            } else {
                lower = mid;
            }
        }
        projected_cycled_ped = round_half_even(upper, 2);
        let projected = project_prospect_levels(skill_levels, &sample, projected_cycled_ped);
        projected_levels = projected.0;
        projected_gains = projected.1;
    }

    let loot_tt_per_ped = sample["lootTtPerPed"].as_f64().unwrap_or(0.0);
    let expected_loot_tt = round_half_even(projected_cycled_ped * loot_tt_per_ped, 2);
    let expected_net_tt_burn = round_half_even(projected_cycled_ped - expected_loot_tt, 2);
    let cycled = sample["cycledPed"].as_f64().unwrap_or(0.0);
    let hours = sample["hours"].as_f64().unwrap_or(0.0);
    let projected_hours = if cycled > 0.0 {
        round_half_even(projected_cycled_ped * (hours / cycled), 2)
    } else {
        0.0
    };

    let (speculative_loot_tt, speculative_net_tt_burn) = if markup_uplift > 0.0 {
        let loot = round_half_even(expected_loot_tt * (1.0 + markup_uplift), 2);
        (
            json!(loot),
            json!(round_half_even(projected_cycled_ped - loot, 2)),
        )
    } else {
        (Value::Null, Value::Null)
    };

    let mut weights: Map<String, Value> = Map::new();
    if let Some(skills) = profession.get("skills").and_then(Value::as_array) {
        for entry in skills {
            let name = entry
                .get("skill")
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let weight = entry.get("weight").and_then(Value::as_f64).unwrap_or(0.0);
            weights.insert(name, json!(weight));
        }
    }

    // `set(skillShares) | set(attributeRates)`: Python set union order
    // is arbitrary, and the rows sort below is total (contribution,
    // attribute flag, then the unique name), so insertion order here
    // never reaches the wire.
    let mut row_names: Vec<String> = Vec::new();
    if let Some(shares) = sample["skillShares"].as_object() {
        row_names.extend(shares.keys().cloned());
    }
    if let Some(rates) = sample["attributeRates"].as_object() {
        for name in rates.keys() {
            if !row_names.contains(name) {
                row_names.push(name.clone());
            }
        }
    }

    let mut rows: Vec<Value> = Vec::new();
    for name in &row_names {
        let current_skill_level = skill_levels
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let projected_gain = projected_gains
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let projected_end_level = projected_levels
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(current_skill_level);
        let weight = weights.get(name).and_then(Value::as_f64).unwrap_or(0.0);
        let contribution = if weight > 0.0 {
            (effective_points(name, projected_gain) * weight) / 10000.0
        } else {
            0.0
        };
        let observed_share = sample["skillShares"]
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let observed_rate = sample["attributeRates"]
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        rows.push(json!({
            "name": name,
            "isAttribute": is_attribute(name),
            "weight": weight,
            "currentLevel": round_half_even(current_skill_level, 2),
            "observedShare": round_half_even(observed_share, 4),
            "observedRate": round_half_even(observed_rate, 6),
            "projectedGain": round_half_even(projected_gain, 2),
            "projectedEndLevel": round_half_even(projected_end_level, 2),
            "professionContribution": round_half_even(contribution, 4),
            "relevant": weight > 0.0,
        }));
    }
    rows.sort_by(|a, b| {
        let contribution = b["professionContribution"]
            .as_f64()
            .partial_cmp(&a["professionContribution"].as_f64())
            .expect("contributions are finite");
        if contribution != std::cmp::Ordering::Equal {
            return contribution;
        }
        let attribute = a["isAttribute"].as_bool().cmp(&b["isAttribute"].as_bool());
        if attribute != std::cmp::Ordering::Equal {
            return attribute;
        }
        a["name"].as_str().cmp(&b["name"].as_str())
    });

    let warnings = build_prospect_warnings(&sample, projected_cycled_ped);
    json!({
        "profession": profession_name,
        "sliceType": slice_type,
        "sliceValue": slice_value,
        "markupUplift": markup_uplift,
        "currentLevel": round_half_even(current_level, 2),
        "targetLevel": round_half_even(target_level, 2),
        "projectedCycledPed": projected_cycled_ped,
        "projectedHours": projected_hours,
        "expectedLootTt": expected_loot_tt,
        "expectedNetTtBurn": expected_net_tt_burn,
        "speculativeLootTt": speculative_loot_tt,
        "speculativeNetTtBurn": speculative_net_tt_burn,
        "sample": sample,
        "rows": rows,
        "warnings": warnings,
    })
}
