//! The typed IPC commands: one thin `#[tauri::command]` per facade
//! operation, each delegating to the composed [`eo_api::Api`].
//!
//! Commands declare `rename_all = "snake_case"` so the invoke argument
//! keys are exactly the Rust parameter names, which is also what the
//! command manifest records and the generated TypeScript sends; the
//! parity test below holds the registered set and the manifest
//! together. Until composition publishes the facade the commands answer
//! the typed unavailable error, the same not-ready contract the
//! `api_request` dispatch has always had (the frontend re-drives its
//! reads on the substrate-installed event).

use std::sync::Arc;

use eo_api::character::{
    CalibrationStatus, CharacterProspectOptions, ComputedCharacterStats, HpOptimizerResult,
    PathOptimizerResult, ProfessionLevel, ProfessionOptimizerResult, ProspectQuery, ProspectResult,
    SkillLevel,
};
use eo_api::equipment::{
    EquipmentDetail, EquipmentRequest, EquipmentSearchHit, EquipmentSummary, SearchKind,
};
use eo_api::settings::{AppSettings, OverlayPosition, SettingsPatch};
use eo_api::ApiError;

/// Holds the composed facade for the typed commands, published by
/// `install_native_services` once every service is present.
pub struct ApiFacade(pub Arc<eo_api::Api>);

/// The composed facade, or the typed not-ready error during the startup
/// window (and permanently if composition declined).
fn facade(app: &tauri::AppHandle) -> Result<Arc<eo_api::Api>, ApiError> {
    use tauri::Manager as _;
    app.try_state::<ApiFacade>()
        .map(|state| state.0.clone())
        .ok_or(ApiError::Unavailable)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_search(
    app: tauri::AppHandle,
    q: String,
    kind: SearchKind,
) -> Result<Vec<EquipmentSearchHit>, ApiError> {
    facade(&app)?.equipment_search(&q, kind).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_library(app: tauri::AppHandle) -> Result<Vec<EquipmentSummary>, ApiError> {
    facade(&app)?.equipment_library().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_add(
    app: tauri::AppHandle,
    req: EquipmentRequest,
) -> Result<EquipmentSummary, ApiError> {
    facade(&app)?.equipment_add(&req).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_update(
    app: tauri::AppHandle,
    item_id: i64,
    req: EquipmentRequest,
) -> Result<EquipmentSummary, ApiError> {
    facade(&app)?.equipment_update(item_id, &req).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_delete(app: tauri::AppHandle, item_id: i64) -> Result<(), ApiError> {
    facade(&app)?.equipment_delete(item_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn equipment_detail(
    app: tauri::AppHandle,
    item_id: i64,
) -> Result<EquipmentDetail, ApiError> {
    facade(&app)?.equipment_detail(item_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_calibration(app: tauri::AppHandle) -> Result<CalibrationStatus, ApiError> {
    facade(&app)?.character_calibration().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_stats(app: tauri::AppHandle) -> Result<ComputedCharacterStats, ApiError> {
    facade(&app)?.character_stats().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_skills(app: tauri::AppHandle) -> Result<Vec<SkillLevel>, ApiError> {
    facade(&app)?.character_skills().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_professions(
    app: tauri::AppHandle,
) -> Result<Vec<ProfessionLevel>, ApiError> {
    facade(&app)?.character_professions().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_prospect_options(
    app: tauri::AppHandle,
) -> Result<CharacterProspectOptions, ApiError> {
    facade(&app)?.character_prospect_options().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_prospect(
    app: tauri::AppHandle,
    query: ProspectQuery,
) -> Result<ProspectResult, ApiError> {
    facade(&app)?.character_prospect(&query).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_profession_optimizer(
    app: tauri::AppHandle,
    profession: String,
) -> Result<ProfessionOptimizerResult, ApiError> {
    facade(&app)?
        .character_profession_optimizer(&profession)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_path_optimizer(
    app: tauri::AppHandle,
    profession: String,
    target_level: Option<f64>,
    ped_budget: Option<f64>,
) -> Result<PathOptimizerResult, ApiError> {
    facade(&app)?
        .character_path_optimizer(&profession, target_level, ped_budget)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn character_hp_optimizer(app: tauri::AppHandle) -> Result<HpOptimizerResult, ApiError> {
    facade(&app)?.character_hp_optimizer().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_get(app: tauri::AppHandle) -> Result<AppSettings, ApiError> {
    facade(&app)?.settings().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_overlay_position(
    app: tauri::AppHandle,
) -> Result<OverlayPosition, ApiError> {
    facade(&app)?.settings_overlay_position().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_set_overlay_position(
    app: tauri::AppHandle,
    x: i64,
    y: i64,
) -> Result<(), ApiError> {
    facade(&app)?.settings_set_overlay_position(x, y).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn settings_update(
    app: tauri::AppHandle,
    patch: SettingsPatch,
) -> Result<AppSettings, ApiError> {
    facade(&app)?.settings_update(patch).await
}

#[cfg(test)]
mod tests {
    /// The commands this module defines and the `generate_handler!`
    /// registration site hold in lock-step with the manifest (the
    /// single source the TypeScript bindings emit from); a command
    /// present on one side and absent on the other is a wiring bug
    /// this catches at test time.
    const TYPED_COMMANDS: &[&str] = &[
        "equipment_search",
        "equipment_library",
        "equipment_add",
        "equipment_update",
        "equipment_delete",
        "equipment_detail",
        "character_calibration",
        "character_stats",
        "character_skills",
        "character_professions",
        "character_prospect_options",
        "character_prospect",
        "character_profession_optimizer",
        "character_path_optimizer",
        "character_hp_optimizer",
        "settings_get",
        "settings_overlay_position",
        "settings_set_overlay_position",
        "settings_update",
    ];

    #[test]
    fn the_registered_commands_match_the_manifest() {
        let manifest: Vec<&str> = eo_api::manifest::manifest()
            .iter()
            .map(|spec| spec.name)
            .collect();
        assert_eq!(TYPED_COMMANDS, manifest.as_slice());
    }
}
