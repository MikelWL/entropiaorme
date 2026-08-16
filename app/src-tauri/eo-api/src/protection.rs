//! Protection setup, live selection, and limited-layer observations.
//!
//! These DTOs keep the armour/plate vocabulary closed at the IPC
//! boundary. The service owns persistence and reconciliation; this
//! facade maps domain outcomes into the generated frontend contract.

use eo_services::config_service::load_config_readonly;
use eo_services::protection::{
    ObservationOutcome as ServiceObservationOutcome, ObservationSource as ServiceObservationSource,
    ProtectionEconomyKind as ServiceEconomyKind, ProtectionError,
    ProtectionLoadout as ServiceLoadout, ProtectionObservation as ServiceObservation,
    ProtectionOverview as ServiceOverview, ProtectionReconciliation as ServiceReconciliation,
    ProtectionSet as ServiceSet, ProtectionSetKind as ServiceSetKind,
    ProtectionSetRef as ServiceSetRef, ReconciliationStatus as ServiceReconciliationStatus,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Api, ApiError, Nullable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionSetKind {
    Armour,
    Plates,
}

impl From<ProtectionSetKind> for ServiceSetKind {
    fn from(value: ProtectionSetKind) -> Self {
        match value {
            ProtectionSetKind::Armour => Self::Armour,
            ProtectionSetKind::Plates => Self::Plates,
        }
    }
}

impl From<ServiceSetKind> for ProtectionSetKind {
    fn from(value: ServiceSetKind) -> Self {
        match value {
            ServiceSetKind::Armour => Self::Armour,
            ServiceSetKind::Plates => Self::Plates,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionEconomyKind {
    Limited,
    Unlimited,
}

impl From<ProtectionEconomyKind> for ServiceEconomyKind {
    fn from(value: ProtectionEconomyKind) -> Self {
        match value {
            ProtectionEconomyKind::Limited => Self::Limited,
            ProtectionEconomyKind::Unlimited => Self::Unlimited,
        }
    }
}

impl From<ServiceEconomyKind> for ProtectionEconomyKind {
    fn from(value: ServiceEconomyKind) -> Self {
        match value {
            ServiceEconomyKind::Limited => Self::Limited,
            ServiceEconomyKind::Unlimited => Self::Unlimited,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionObservationSource {
    Ocr,
    Manual,
}

impl From<ProtectionObservationSource> for ServiceObservationSource {
    fn from(value: ProtectionObservationSource) -> Self {
        match value {
            ProtectionObservationSource::Ocr => Self::Ocr,
            ProtectionObservationSource::Manual => Self::Manual,
        }
    }
}

impl From<ServiceObservationSource> for ProtectionObservationSource {
    fn from(value: ServiceObservationSource) -> Self {
        match value {
            ServiceObservationSource::Ocr => Self::Ocr,
            ServiceObservationSource::Manual => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionObservation {
    pub id: String,
    pub set_id: String,
    pub tt_value_ped: f64,
    pub source: ProtectionObservationSource,
    pub raw_text: Nullable<String>,
    pub observed_at: f64,
    pub reset_reason: Nullable<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionSet {
    pub id: String,
    pub kind: ProtectionSetKind,
    pub name: String,
    pub economy_kind: ProtectionEconomyKind,
    pub markup_percent: Nullable<f64>,
    pub latest_observation: Nullable<ProtectionObservation>,
    pub pending_reconciliations: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionSetRef {
    pub id: String,
    pub name: String,
    pub economy_kind: ProtectionEconomyKind,
    pub markup_percent: Nullable<f64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionLoadout {
    pub id: String,
    pub name: String,
    pub armour: Nullable<ProtectionSetRef>,
    pub plates: Nullable<ProtectionSetRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProtectionReconciliationStatus {
    Booked,
    Pending,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionReconciliation {
    pub id: String,
    pub set_id: String,
    pub opening_observation_id: String,
    pub closing_observation_id: String,
    pub consumed_tt_ped: f64,
    pub markup_percent: f64,
    pub cost_ped: f64,
    pub status: ProtectionReconciliationStatus,
    pub session_id: Nullable<String>,
    pub reason: Nullable<String>,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionOverview {
    pub sets: Vec<ProtectionSet>,
    pub loadouts: Vec<ProtectionLoadout>,
    pub active_loadout_id: Nullable<String>,
    pub recent_reconciliations: Vec<ProtectionReconciliation>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionSetInput {
    pub kind: ProtectionSetKind,
    pub name: String,
    pub economy_kind: ProtectionEconomyKind,
    #[serde(default)]
    pub markup_percent: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionLoadoutInput {
    pub name: String,
    #[serde(default)]
    pub armour_set_id: Option<i64>,
    #[serde(default)]
    pub plate_set_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionObservationInput {
    pub set_id: i64,
    pub client_token: String,
    pub tt_value_ped: f64,
    pub source: ProtectionObservationSource,
    #[serde(default)]
    pub raw_text: Option<String>,
    #[serde(default)]
    pub reset_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionObservationOutcome {
    pub observation: ProtectionObservation,
    pub reconciliation: Nullable<ProtectionReconciliation>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionScanResult {
    pub value_ped: Nullable<f64>,
    pub raw_text: Nullable<String>,
    pub confidence: Nullable<f64>,
    pub error: Nullable<String>,
    pub calibrated: bool,
}

impl Api {
    pub async fn protection_overview(&self) -> Result<ProtectionOverview, ApiError> {
        self.protection
            .overview()
            .await
            .map(Into::into)
            .map_err(protection_error)
    }

    pub async fn protection_set_create(
        &self,
        input: &ProtectionSetInput,
    ) -> Result<ProtectionOverview, ApiError> {
        self.protection
            .create_set(
                input.kind.into(),
                &input.name,
                input.economy_kind.into(),
                input.markup_percent,
            )
            .await
            .map_err(protection_error)?;
        self.protection_overview().await
    }

    pub async fn protection_loadout_create(
        &self,
        input: &ProtectionLoadoutInput,
    ) -> Result<ProtectionOverview, ApiError> {
        self.protection
            .create_loadout(&input.name, input.armour_set_id, input.plate_set_id)
            .await
            .map_err(protection_error)?;
        self.protection_overview().await
    }

    pub async fn protection_set_archive(
        &self,
        set_id: i64,
    ) -> Result<ProtectionOverview, ApiError> {
        self.protection
            .archive_set(set_id)
            .await
            .map_err(protection_error)?;
        self.protection_overview().await
    }

    pub async fn protection_loadout_archive(
        &self,
        loadout_id: i64,
    ) -> Result<ProtectionOverview, ApiError> {
        self.protection
            .archive_loadout(loadout_id)
            .await
            .map_err(protection_error)?;
        self.protection_overview().await
    }

    pub async fn protection_select(&self, loadout_id: i64) -> Result<ProtectionOverview, ApiError> {
        let selection = self
            .protection
            .selection(loadout_id)
            .await
            .map_err(protection_error)?;
        if self.tracker.is_tracking() {
            match self.tracker.set_protection(selection).await {
                Ok(()) => {}
                Err(eo_services::tracker::TrackerCommandError::NoActiveSession) => {
                    self.protection
                        .persist_active_loadout(loadout_id)
                        .await
                        .map_err(protection_error)?;
                }
                Err(eo_services::tracker::TrackerCommandError::Persistence) => {
                    return Err(ApiError::invalid_state(
                        "live protection selection persistence failed",
                    ));
                }
            }
        } else {
            self.protection
                .persist_active_loadout(loadout_id)
                .await
                .map_err(protection_error)?;
        }
        self.protection_overview().await
    }

    pub async fn protection_observation_confirm(
        &self,
        input: &ProtectionObservationInput,
    ) -> Result<ProtectionObservationOutcome, ApiError> {
        self.protection
            .confirm_observation(
                input.set_id,
                &input.client_token,
                input.tt_value_ped,
                input.source.into(),
                input.raw_text.as_deref(),
                input.reset_reason.as_deref(),
            )
            .await
            .map(Into::into)
            .map_err(protection_error)
    }

    pub fn protection_trade_terminal_scan(&self) -> Result<ProtectionScanResult, ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("Trade Terminal scan config"))?;
        if !config.repair_ocr_enabled {
            return Err(ApiError::bad_request("Terminal OCR is disabled"));
        }
        let value = self.repair_ocr.scan_trade_terminal_value();
        Ok(ProtectionScanResult {
            value_ped: value
                .get("cost_ped")
                .and_then(serde_json::Value::as_f64)
                .into(),
            raw_text: value
                .get("raw_text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .filter(|text| !text.is_empty())
                .into(),
            confidence: value
                .get("confidence")
                .and_then(serde_json::Value::as_f64)
                .into(),
            error: value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .into(),
            calibrated: value
                .get("calibrated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        })
    }
}

fn protection_error(error: ProtectionError) -> ApiError {
    match error {
        ProtectionError::Invalid(message) => ApiError::bad_request(message),
        ProtectionError::NotFound(message) => ApiError::not_found(message),
        ProtectionError::Conflict(message) => ApiError::conflict(message),
        ProtectionError::Db(error) => ApiError::internal("protection service")(error),
        ProtectionError::Stored(message) => ApiError::invalid_state(message),
    }
}

impl From<ServiceObservation> for ProtectionObservation {
    fn from(value: ServiceObservation) -> Self {
        Self {
            id: value.id.to_string(),
            set_id: value.set_id.to_string(),
            tt_value_ped: value.tt_value_ped,
            source: value.source.into(),
            raw_text: value.raw_text.into(),
            observed_at: value.observed_at,
            reset_reason: value.reset_reason.into(),
        }
    }
}

impl From<ServiceSet> for ProtectionSet {
    fn from(value: ServiceSet) -> Self {
        Self {
            id: value.id.to_string(),
            kind: value.kind.into(),
            name: value.name,
            economy_kind: value.economy_kind.into(),
            markup_percent: value.markup_percent.into(),
            latest_observation: value.latest_observation.map(Into::into).into(),
            pending_reconciliations: value.pending_reconciliations,
        }
    }
}

impl From<ServiceSetRef> for ProtectionSetRef {
    fn from(value: ServiceSetRef) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name,
            economy_kind: value.economy_kind.into(),
            markup_percent: value.markup_percent.into(),
        }
    }
}

impl From<ServiceLoadout> for ProtectionLoadout {
    fn from(value: ServiceLoadout) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name,
            armour: value.armour.map(Into::into).into(),
            plates: value.plates.map(Into::into).into(),
        }
    }
}

impl From<ServiceReconciliation> for ProtectionReconciliation {
    fn from(value: ServiceReconciliation) -> Self {
        Self {
            id: value.id.to_string(),
            set_id: value.set_id.to_string(),
            opening_observation_id: value.opening_observation_id.to_string(),
            closing_observation_id: value.closing_observation_id.to_string(),
            consumed_tt_ped: value.consumed_tt_ped,
            markup_percent: value.markup_percent,
            cost_ped: value.cost_ped,
            status: match value.status {
                ServiceReconciliationStatus::Booked => ProtectionReconciliationStatus::Booked,
                ServiceReconciliationStatus::Pending => ProtectionReconciliationStatus::Pending,
            },
            session_id: value.session_id.into(),
            reason: value.reason.into(),
            created_at: value.created_at,
        }
    }
}

impl From<ServiceObservationOutcome> for ProtectionObservationOutcome {
    fn from(value: ServiceObservationOutcome) -> Self {
        Self {
            observation: value.observation.into(),
            reconciliation: value.reconciliation.map(Into::into).into(),
        }
    }
}

impl From<ServiceOverview> for ProtectionOverview {
    fn from(value: ServiceOverview) -> Self {
        Self {
            sets: value.sets.into_iter().map(Into::into).collect(),
            loadouts: value.loadouts.into_iter().map(Into::into).collect(),
            active_loadout_id: value.active_loadout_id.map(|id| id.to_string()).into(),
            recent_reconciliations: value
                .recent_reconciliations
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}
