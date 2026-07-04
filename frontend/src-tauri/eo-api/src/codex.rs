//! The codex family: the species listing and per-species rank breakdown,
//! the skill-recommendation read, the six meta attributes, and the
//! claim / unclaim / calibrate / meta-claim writes.
//!
//! Ported from the HTTP route handlers onto typed DTOs. The computation
//! stays in `eo_services::codex::CodexService` (which returns
//! `serde_json::Value`); the facade types the boundary by shaping each
//! response into a declared DTO (`serde_json::from_value`, the character
//! family's Value-bridge pattern), so no stored bytes change and the wire
//! shape is single-sourced in Rust. The response DTOs' field order is the
//! wire order the service's `json!` bodies carried, so a shaped DTO
//! serialises byte-identical to the HTTP-era body.
//!
//! Several transport-era behaviours retire with the migration, ratified
//! under ADR-0017/0019. The conditional-GET (ETag) contract retires with
//! the transport (the reads answer their body directly). The recommend
//! route's rank 422 envelope becomes a typed `bad_request` (the bound is
//! now checked on the `i64` argument, not a pydantic query model), its
//! `target` vocabulary is a closed enum (the out-of-vocabulary 422 is
//! unrepresentable), and the calibrate route's surrogate-codec 400 plus
//! the body-taint / beyond-`i64` 422/500 ceremony become unrepresentable
//! over the typed command (a `String` argument cannot carry a surrogate
//! and an `i64` rank cannot overflow the parse).

use eo_services::codex::{CodexError, CodexService};
use eo_services::skill_tracker::SUPPRESS_TIMEOUT_SECONDS;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Api, ApiError};

// ── Request arguments ───────────────────────────────────────────────

/// What a codex recommendation ranks by. A closed vocabulary: the
/// bindings expose only these two, so the HTTP route's out-of-vocabulary
/// 422 (it defaulted an unknown `target` to `profession`) is
/// unrepresentable rather than validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CodexRecommendTarget {
    Profession,
    Hp,
}

impl CodexRecommendTarget {
    fn as_str(self) -> &'static str {
        match self {
            CodexRecommendTarget::Profession => "profession",
            CodexRecommendTarget::Hp => "hp",
        }
    }
}

// ── Response DTOs ───────────────────────────────────────────────────

/// One species in the codex listing: its base cost, the player's current
/// rank, and the next rank's derived category and cost (all `null` once
/// rank 25 is reached).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexSpecies {
    pub name: String,
    pub base_cost: f64,
    pub codex_type: Option<String>,
    pub current_rank: i64,
    pub next_rank: Option<i64>,
    pub next_category: Option<String>,
    pub next_cost: Option<f64>,
}

/// One rank in a species' breakdown: the derived cost / reward / category
/// fields, plus the player's claim state for that rank. Field order is
/// the wire order (the breakdown's own fields, then the claim overlay).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexRank {
    pub rank: i64,
    pub category: String,
    pub cost: f64,
    pub reward_ped: f64,
    pub cat4_bonus: bool,
    pub cat4_reward_ped: Option<f64>,
    pub skills: Vec<String>,
    pub cat4_skills: Vec<String>,
    pub claimed: bool,
    pub claimed_skill: Option<String>,
    pub claimed_ped: Option<f64>,
    pub is_next: bool,
}

/// A species' full 25-rank breakdown, cross-referenced with the player's
/// claims and current rank.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexSpeciesRanks {
    pub species_name: String,
    pub base_cost: f64,
    pub codex_type: Option<String>,
    pub current_rank: i64,
    pub ranks: Vec<CodexRank>,
}

/// One skill option in a rank recommendation: the reward it grants, the
/// levels that buys at the current point on the curve, and the profession
/// / HP contribution used to rank the list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexSkillOption {
    pub skill_name: String,
    pub category: String,
    pub reward_ped: f64,
    pub current_level: Option<f64>,
    pub levels_gained: f64,
    pub profession_weight: i64,
    pub prof_contribution: f64,
    pub hp_increase: Option<f64>,
    pub hp_gain: f64,
    pub recommend_rank: Option<i64>,
}

/// One meta attribute with its current calibrated level.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexMetaAttribute {
    pub name: String,
    pub current_level: Option<f64>,
}

/// The record a rank claim (or its reversal) returns.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexClaimResult {
    pub species_name: String,
    pub rank: i64,
    pub skill_name: String,
    pub ped_value: f64,
}

/// The record a manual rank calibration returns.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexCalibrateResult {
    pub species_name: String,
    pub rank: i64,
}

/// The record a meta claim returns.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexMetaClaimResult {
    pub attribute_name: String,
    pub ped_value: f64,
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// All mob species carrying a codex base cost, cross-referenced with
    /// the player's rank, sorted rank-descending then name-ascending.
    pub async fn codex_species(&self) -> Result<Vec<CodexSpecies>, ApiError> {
        let species = self
            .codex
            .get_all_species()
            .await
            .map_err(ApiError::internal("codex species read"))?;
        serde_json::from_value(Value::Array(species))
            .map_err(ApiError::internal("codex species shaping"))
    }

    /// The 25-rank breakdown for a species, cross-referenced with claims.
    /// A species absent from the catalogue is a not-found (the HTTP
    /// route's 404).
    pub async fn codex_species_ranks(
        &self,
        species_name: &str,
    ) -> Result<CodexSpeciesRanks, ApiError> {
        let ranks = self
            .codex
            .get_species_ranks(species_name)
            .await
            .map_err(ApiError::internal("codex species ranks read"))?;
        let Some(ranks) = ranks else {
            return Err(ApiError::not_found(format!(
                "Species '{species_name}' not found"
            )));
        };
        serde_json::from_value(ranks).map_err(ApiError::internal("codex species ranks shaping"))
    }

    /// The skill options for a rank, ranked by profession contribution or
    /// HP gain. An empty list when the species is not in the catalogue
    /// (the HTTP route's empty-200). The rank is bound to the codex
    /// table's 1..=25 domain, the query 422 the route validated now a
    /// typed `bad_request`.
    pub async fn codex_recommend(
        &self,
        species_name: &str,
        rank: i64,
        profession: Option<&str>,
        target: CodexRecommendTarget,
    ) -> Result<Vec<CodexSkillOption>, ApiError> {
        if !(1..=25).contains(&rank) {
            return Err(ApiError::bad_request("rank must be between 1 and 25"));
        }
        let options = self
            .codex
            .get_skill_options(species_name, rank, profession, target.as_str())
            .await
            .map_err(ApiError::internal("codex recommend read"))?;
        serde_json::from_value(Value::Array(options))
            .map_err(ApiError::internal("codex recommend shaping"))
    }

    /// The six meta attributes with their current calibrated levels.
    pub async fn codex_meta_attributes(&self) -> Result<Vec<CodexMetaAttribute>, ApiError> {
        let attributes = self
            .codex
            .get_meta_attributes()
            .await
            .map_err(ApiError::internal("codex meta attributes read"))?;
        serde_json::from_value(Value::Array(attributes))
            .map_err(ApiError::internal("codex meta attributes shaping"))
    }

    /// Set a species' codex rank directly (manual calibration, no side
    /// effects). An out-of-domain rank is the service's `bad_request`.
    pub async fn codex_calibrate(
        &self,
        species_name: &str,
        rank: i64,
    ) -> Result<CodexCalibrateResult, ApiError> {
        let result = self
            .codex
            .calibrate(species_name, rank)
            .await
            .map_err(codex_write_error("codex calibrate"))?;
        serde_json::from_value(result).map_err(ApiError::internal("codex calibrate shaping"))
    }

    /// Claim a codex rank reward. On success, an active session suppresses
    /// the claimed skill's next gain from dedup (`suppress_next`), exactly
    /// as the HTTP route did and only after the claim succeeds.
    pub async fn codex_claim(
        &self,
        species_name: &str,
        rank: i64,
        skill_name: &str,
    ) -> Result<CodexClaimResult, ApiError> {
        let result = self
            .codex
            .claim_rank(species_name, rank, skill_name)
            .await
            .map_err(codex_write_error("codex claim"))?;
        if self.tracker.is_tracking() {
            self.skill_tracker
                .suppress_next(skill_name, SUPPRESS_TIMEOUT_SECONDS);
        }
        serde_json::from_value(result).map_err(ApiError::internal("codex claim shaping"))
    }

    /// Revert a species' most recent rank claim. No session suppression:
    /// an unclaim removes a calibration rather than producing a gain.
    pub async fn codex_unclaim(&self, species_name: &str) -> Result<CodexClaimResult, ApiError> {
        let result = self
            .codex
            .unclaim_rank(species_name)
            .await
            .map_err(codex_write_error("codex unclaim"))?;
        serde_json::from_value(result).map_err(ApiError::internal("codex unclaim shaping"))
    }

    /// Claim a meta codex reward (1 PES into an attribute). On success, an
    /// active session suppresses the attribute's next gain.
    pub async fn codex_meta_claim(
        &self,
        attribute_name: &str,
    ) -> Result<CodexMetaClaimResult, ApiError> {
        let result = self
            .codex
            .meta_claim(attribute_name)
            .await
            .map_err(codex_write_error("codex meta claim"))?;
        if self.tracker.is_tracking() {
            self.skill_tracker
                .suppress_next(attribute_name, SUPPRESS_TIMEOUT_SECONDS);
        }
        serde_json::from_value(result).map_err(ApiError::internal("codex meta claim shaping"))
    }
}

/// The codex writes' error mapping: the service's invalid-input errors
/// carry a user-facing message the HTTP route answered as a 400, so they
/// map to `bad_request`; a driver or rollup failure is an internal error
/// whose detail stays server-side.
fn codex_write_error(context: &'static str) -> impl FnOnce(CodexError) -> ApiError {
    move |err| match err {
        CodexError::Invalid(message) => ApiError::bad_request(message),
        other => ApiError::internal(context)(other),
    }
}

/// Construct the codex service over the facade's shared handles. Kept a
/// free function so [`Api::new`] can build it before the struct is
/// assembled.
pub(crate) fn build_codex_service(
    db: eo_services::db::Db,
    game_data: std::sync::Arc<eo_services::game_data_store::GameDataStore>,
    clock: std::sync::Arc<dyn eo_services::clock::Clock>,
) -> CodexService {
    CodexService::new(db, game_data, clock)
}
