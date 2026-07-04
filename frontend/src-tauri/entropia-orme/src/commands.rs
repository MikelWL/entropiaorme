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

use eo_api::analytics::{
    AnalyticsActivity, AnalyticsOverview, InventoryItem, InventoryItemInput, InventoryPatch,
    InventorySellInput, InventorySellResult, LedgerEntryInput, LedgerItem, LedgerPage, LedgerPreset,
    LedgerPresetInput,
};
use eo_api::character::{
    CalibrationStatus, CharacterProspectOptions, ComputedCharacterStats, HpOptimizerResult,
    PathOptimizerResult, ProfessionLevel, ProfessionOptimizerResult, ProspectQuery, ProspectResult,
    SkillLevel,
};
use eo_api::codex::{
    CodexCalibrateResult, CodexClaimResult, CodexMetaAttribute, CodexMetaClaimResult,
    CodexRecommendTarget, CodexSkillOption, CodexSpecies, CodexSpeciesRanks,
};
use eo_api::equipment::{
    EquipmentDetail, EquipmentRequest, EquipmentSearchHit, EquipmentSummary, SearchKind,
};
use eo_api::quests::{
    PlaylistAnalyticsRow, PlaylistInput, Quest, QuestAnalyticsRow, QuestInput, QuestPlaylist,
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

/// The analytics fixture-backed read for the native-shell e2e build: the
/// live analytics surface migrated off `api_request` onto typed commands,
/// so the e2e build serves the same committed analytics fixture through
/// these commands (deserialised into their DTO), keeping the visual
/// baselines stable.
#[cfg(feature = "e2e-stub")]
fn e2e_analytics<T: serde::de::DeserializeOwned>(key: &str) -> Result<T, ApiError> {
    serde_json::from_value(crate::e2e_stub::analytics_fixture(key))
        .map_err(ApiError::internal("e2e analytics fixture"))
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
pub async fn settings_overlay_position(app: tauri::AppHandle) -> Result<OverlayPosition, ApiError> {
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

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_species(app: tauri::AppHandle) -> Result<Vec<CodexSpecies>, ApiError> {
    facade(&app)?.codex_species().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_species_ranks(
    app: tauri::AppHandle,
    species_name: String,
) -> Result<CodexSpeciesRanks, ApiError> {
    facade(&app)?.codex_species_ranks(&species_name).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_recommend(
    app: tauri::AppHandle,
    species_name: String,
    rank: i64,
    profession: Option<String>,
    target: CodexRecommendTarget,
) -> Result<Vec<CodexSkillOption>, ApiError> {
    facade(&app)?
        .codex_recommend(&species_name, rank, profession.as_deref(), target)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_meta_attributes(
    app: tauri::AppHandle,
) -> Result<Vec<CodexMetaAttribute>, ApiError> {
    facade(&app)?.codex_meta_attributes().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_calibrate(
    app: tauri::AppHandle,
    species_name: String,
    rank: i64,
) -> Result<CodexCalibrateResult, ApiError> {
    facade(&app)?.codex_calibrate(&species_name, rank).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_claim(
    app: tauri::AppHandle,
    species_name: String,
    rank: i64,
    skill_name: String,
) -> Result<CodexClaimResult, ApiError> {
    facade(&app)?
        .codex_claim(&species_name, rank, &skill_name)
        .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_unclaim(
    app: tauri::AppHandle,
    species_name: String,
) -> Result<CodexClaimResult, ApiError> {
    facade(&app)?.codex_unclaim(&species_name).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn codex_meta_claim(
    app: tauri::AppHandle,
    attribute_name: String,
) -> Result<CodexMetaClaimResult, ApiError> {
    facade(&app)?.codex_meta_claim(&attribute_name).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quests_list(app: tauri::AppHandle) -> Result<Vec<Quest>, ApiError> {
    facade(&app)?.quests_list().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_get(app: tauri::AppHandle, quest_id: i64) -> Result<Quest, ApiError> {
    facade(&app)?.quest_get(quest_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_create(app: tauri::AppHandle, input: QuestInput) -> Result<Quest, ApiError> {
    facade(&app)?.quest_create(input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_update(
    app: tauri::AppHandle,
    quest_id: i64,
    input: QuestInput,
) -> Result<Quest, ApiError> {
    facade(&app)?.quest_update(quest_id, input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_delete(app: tauri::AppHandle, quest_id: i64) -> Result<(), ApiError> {
    facade(&app)?.quest_delete(quest_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_start(app: tauri::AppHandle, quest_id: i64) -> Result<Quest, ApiError> {
    facade(&app)?.quest_start(quest_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_complete(app: tauri::AppHandle, quest_id: i64) -> Result<Quest, ApiError> {
    facade(&app)?.quest_complete(quest_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quest_cancel(
    app: tauri::AppHandle,
    quest_id: i64,
    undo_reward: bool,
) -> Result<Quest, ApiError> {
    facade(&app)?.quest_cancel(quest_id, undo_reward).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quests_mobs(app: tauri::AppHandle) -> Result<Vec<String>, ApiError> {
    facade(&app)?.quests_mobs().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn quests_analytics(app: tauri::AppHandle) -> Result<Vec<QuestAnalyticsRow>, ApiError> {
    facade(&app)?.quests_analytics().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn playlists_list(app: tauri::AppHandle) -> Result<Vec<QuestPlaylist>, ApiError> {
    facade(&app)?.playlists_list().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn playlist_create(
    app: tauri::AppHandle,
    input: PlaylistInput,
) -> Result<QuestPlaylist, ApiError> {
    facade(&app)?.playlist_create(input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn playlist_update(
    app: tauri::AppHandle,
    playlist_id: i64,
    input: PlaylistInput,
) -> Result<QuestPlaylist, ApiError> {
    facade(&app)?.playlist_update(playlist_id, input).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn playlist_delete(app: tauri::AppHandle, playlist_id: i64) -> Result<(), ApiError> {
    facade(&app)?.playlist_delete(playlist_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn playlists_analytics(
    app: tauri::AppHandle,
) -> Result<Vec<PlaylistAnalyticsRow>, ApiError> {
    facade(&app)?.playlists_analytics().await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn analytics_overview(
    app: tauri::AppHandle,
    period: String,
) -> Result<AnalyticsOverview, ApiError> {
    #[cfg(feature = "e2e-stub")]
    {
        let _ = (&app, &period);
        e2e_analytics("overview")
    }
    #[cfg(not(feature = "e2e-stub"))]
    {
        facade(&app)?.analytics_overview(&period).await
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn analytics_activity(app: tauri::AppHandle) -> Result<AnalyticsActivity, ApiError> {
    #[cfg(feature = "e2e-stub")]
    {
        let _ = &app;
        e2e_analytics("activity")
    }
    #[cfg(not(feature = "e2e-stub"))]
    {
        facade(&app)?.analytics_activity().await
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_list(
    app: tauri::AppHandle,
    cursor: Option<String>,
    limit: Option<i64>,
) -> Result<LedgerPage, ApiError> {
    #[cfg(feature = "e2e-stub")]
    {
        let _ = (&app, &cursor, &limit);
        Ok(LedgerPage {
            entries: e2e_analytics("ledger")?,
            next_cursor: None,
        })
    }
    #[cfg(not(feature = "e2e-stub"))]
    {
        facade(&app)?.ledger_list(cursor, limit).await
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_create(
    app: tauri::AppHandle,
    entry: LedgerEntryInput,
) -> Result<LedgerItem, ApiError> {
    facade(&app)?.ledger_create(entry).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_delete(app: tauri::AppHandle, entry_id: String) -> Result<(), ApiError> {
    facade(&app)?.ledger_delete(entry_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_presets_list(app: tauri::AppHandle) -> Result<Vec<LedgerPreset>, ApiError> {
    #[cfg(feature = "e2e-stub")]
    {
        let _ = &app;
        e2e_analytics("presets")
    }
    #[cfg(not(feature = "e2e-stub"))]
    {
        facade(&app)?.ledger_presets_list().await
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_preset_create(
    app: tauri::AppHandle,
    preset: LedgerPresetInput,
) -> Result<LedgerPreset, ApiError> {
    facade(&app)?.ledger_preset_create(preset).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn ledger_preset_delete(
    app: tauri::AppHandle,
    preset_id: String,
) -> Result<(), ApiError> {
    facade(&app)?.ledger_preset_delete(preset_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn inventory_list(app: tauri::AppHandle) -> Result<Vec<InventoryItem>, ApiError> {
    #[cfg(feature = "e2e-stub")]
    {
        let _ = &app;
        e2e_analytics("inventory")
    }
    #[cfg(not(feature = "e2e-stub"))]
    {
        facade(&app)?.inventory_list().await
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn inventory_create(
    app: tauri::AppHandle,
    item: InventoryItemInput,
) -> Result<InventoryItem, ApiError> {
    facade(&app)?.inventory_create(item).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn inventory_update(
    app: tauri::AppHandle,
    item_id: String,
    patch: InventoryPatch,
) -> Result<InventoryItem, ApiError> {
    facade(&app)?.inventory_update(item_id, patch).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn inventory_delete(app: tauri::AppHandle, item_id: String) -> Result<(), ApiError> {
    facade(&app)?.inventory_delete(item_id).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn inventory_sell(
    app: tauri::AppHandle,
    item_id: String,
    sale: InventorySellInput,
) -> Result<InventorySellResult, ApiError> {
    facade(&app)?.inventory_sell(item_id, sale).await
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
        "codex_species",
        "codex_species_ranks",
        "codex_recommend",
        "codex_meta_attributes",
        "codex_calibrate",
        "codex_claim",
        "codex_unclaim",
        "codex_meta_claim",
        "quests_list",
        "quest_get",
        "quest_create",
        "quest_update",
        "quest_delete",
        "quest_start",
        "quest_complete",
        "quest_cancel",
        "quests_mobs",
        "quests_analytics",
        "playlists_list",
        "playlist_create",
        "playlist_update",
        "playlist_delete",
        "playlists_analytics",
        "analytics_overview",
        "analytics_activity",
        "ledger_list",
        "ledger_create",
        "ledger_delete",
        "ledger_presets_list",
        "ledger_preset_create",
        "ledger_preset_delete",
        "inventory_list",
        "inventory_create",
        "inventory_update",
        "inventory_delete",
        "inventory_sell",
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
