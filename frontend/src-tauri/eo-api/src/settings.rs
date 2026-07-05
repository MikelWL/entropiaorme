//! The settings family: the assembled settings read, the overlay-position
//! read/write, and the partial settings update that re-signals the live
//! producers.
//!
//! Ported from the HTTP route handlers onto typed DTOs. The stored
//! `settings.json` bytes are unchanged: the native `ConfigService`
//! remains the sole writer, saving whole-file with the reference JSON
//! formatting, so the on-disk state is identical either side of the
//! transport migration. The read response shapes match the frontend's
//! hand-written contract (`$lib/types/settings.ts`) field for field.
//!
//! Two behaviours retire with the migration, ratified under
//! ADR-0017/0019. The pydantic-era `exclude_unset` partial the HTTP layer
//! parsed is now the all-`Option` [`SettingsPatch`] DTO, so the framework
//! 422/500 envelopes it produced (a non-integer overlay coordinate, a
//! structurally-malformed `hotbar`/`trifecta_presets` container, an
//! unrenderable surrogate string) become unrepresentable over the typed
//! command rather than validated. And the dead `POST /api/settings/reset`
//! retires unconverted: it has no frontend caller, exactly as the
//! character codex read and the equipment cost endpoint retired with
//! their families.

use std::path::Path;

use eo_services::config_service::{load_config_readonly, AppConfig};
use eo_services::paths::DB_FILE_NAME;
use eo_services::trifecta_service::{validate_trifecta, TrifectaPreset};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};

use crate::{Api, ApiError};

/// The version the settings response stamps. The crate inherits the
/// workspace version, which the version-stamp parity guard holds in
/// lock-step with the packaged artefacts.
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Response DTOs ───────────────────────────────────────────────────

/// The game-connection block: the configured chat-log path, whether it
/// currently resolves to a file, and the player name.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameConnection {
    pub chat_log_path: String,
    pub chat_log_valid: bool,
    pub player_name: String,
}

/// One trifecta preset in the settings view: the stored equipment ids
/// plus the live readiness validation against the library.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrifectaPresetView {
    pub id: String,
    pub name: String,
    pub small_weapon_id: Option<i64>,
    pub big_weapon_id: Option<i64>,
    pub heal_id: Option<i64>,
    pub ready: bool,
    pub message: Option<String>,
}

/// The trifecta block: every preset validated, with the active preset's
/// readiness lifted to the top level.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrifectaSettings {
    pub active_preset_id: Option<String>,
    pub active_preset_name: Option<String>,
    pub presets: Vec<TrifectaPresetView>,
    pub ready: bool,
    pub message: Option<String>,
}

/// The full assembled settings response. Field order is the wire order
/// the frontend contract expects (and the HTTP body carried).
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub game_connection: GameConnection,
    pub hotbar_hooks_enabled: bool,
    pub repair_ocr_enabled: bool,
    pub end_of_session_armour_reminder_enabled: bool,
    pub developer_mode_enabled: bool,
    pub mob_tracking_mode: String,
    pub mob_tracking_tag: String,
    /// The slot-to-equipment map, carried through in its stored insertion
    /// order (`serde_json`'s `preserve_order`), so slot "0" stays last.
    pub hotbar: Map<String, Value>,
    pub trifecta: TrifectaSettings,
    pub loot_filter_blacklist: Vec<String>,
    pub db_path: String,
    pub app_version: String,
}

/// GET overlay-position: the persisted overlay window coordinates (null
/// until first placed).
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverlayPosition {
    pub x: Option<i64>,
    pub y: Option<i64>,
}

// ── Request DTOs ────────────────────────────────────────────────────

/// One trifecta preset in a settings update. Field names stay in the
/// stored snake_case the config writer re-normalises.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrifectaPresetInput {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub small_weapon_id: Option<i64>,
    #[serde(default)]
    pub big_weapon_id: Option<i64>,
    #[serde(default)]
    pub heal_id: Option<i64>,
}

/// The partial settings update: every field optional, only the present
/// ones applied (the `exclude_unset` semantics the pydantic model had).
/// `active_trifecta_preset_id` is a double option so an explicit `null`
/// (clear the active preset) stays distinct from an absent field (leave
/// it untouched); every other field is nullless, so a plain `Option`
/// carries the present/absent distinction.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct SettingsPatch {
    #[serde(default)]
    pub chatlog_path: Option<String>,
    #[serde(default)]
    pub player_name: Option<String>,
    #[serde(default)]
    pub hotbar_hooks_enabled: Option<bool>,
    #[serde(default)]
    pub repair_ocr_enabled: Option<bool>,
    #[serde(default)]
    pub end_of_session_armour_reminder_enabled: Option<bool>,
    #[serde(default)]
    pub developer_mode_enabled: Option<bool>,
    #[serde(default)]
    pub mob_tracking_mode: Option<String>,
    #[serde(default)]
    pub mob_tracking_tag: Option<String>,
    #[serde(default)]
    pub hotbar: Option<Map<String, Value>>,
    #[serde(default, deserialize_with = "double_option")]
    pub active_trifecta_preset_id: Option<Option<String>>,
    #[serde(default)]
    pub trifecta_presets: Option<Vec<TrifectaPresetInput>>,
    #[serde(default)]
    pub loot_filter_blacklist: Option<Vec<String>>,
}

/// Deserialize a present-but-`null` field to `Some(None)` and an absent
/// field to `None` (paired with `#[serde(default)]`), the distinction a
/// bare `Option<Option<T>>` collapses.
fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl SettingsPatch {
    /// The present fields as the update map the config writer applies
    /// (stored snake_case keys). Absent fields are omitted; the `hotbar`
    /// and `trifecta_presets` containers pass through as their raw JSON
    /// value, which the writer re-normalises.
    fn into_updates(self) -> Map<String, Value> {
        let mut updates = Map::new();
        if let Some(value) = self.chatlog_path {
            updates.insert("chatlog_path".into(), Value::String(value));
        }
        if let Some(value) = self.player_name {
            updates.insert("player_name".into(), Value::String(value));
        }
        if let Some(value) = self.hotbar_hooks_enabled {
            updates.insert("hotbar_hooks_enabled".into(), Value::Bool(value));
        }
        if let Some(value) = self.repair_ocr_enabled {
            updates.insert("repair_ocr_enabled".into(), Value::Bool(value));
        }
        if let Some(value) = self.end_of_session_armour_reminder_enabled {
            updates.insert(
                "end_of_session_armour_reminder_enabled".into(),
                Value::Bool(value),
            );
        }
        if let Some(value) = self.developer_mode_enabled {
            updates.insert("developer_mode_enabled".into(), Value::Bool(value));
        }
        if let Some(value) = self.mob_tracking_mode {
            updates.insert("mob_tracking_mode".into(), Value::String(value));
        }
        if let Some(value) = self.mob_tracking_tag {
            updates.insert("mob_tracking_tag".into(), Value::String(value));
        }
        if let Some(value) = self.hotbar {
            updates.insert("hotbar".into(), Value::Object(value));
        }
        if let Some(value) = self.active_trifecta_preset_id {
            updates.insert(
                "active_trifecta_preset_id".into(),
                value.map(Value::String).unwrap_or(Value::Null),
            );
        }
        if let Some(value) = self.trifecta_presets {
            updates.insert("trifecta_presets".into(), json!(value));
        }
        if let Some(value) = self.loot_filter_blacklist {
            updates.insert("loot_filter_blacklist".into(), json!(value));
        }
        updates
    }
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// The full settings assembly: the config fields, the live chat-log
    /// validity, the per-preset trifecta readiness, the resolved db path,
    /// and the version stamp. Reads the config fresh from disk, so a read
    /// after a write is coherent (the writer saves before responding).
    pub async fn settings(&self) -> Result<AppSettings, ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("settings config read"))?;
        let trifecta = self.trifecta_block(&config).await?;
        Ok(AppSettings {
            game_connection: GameConnection {
                chat_log_path: config.chatlog_path.clone(),
                chat_log_valid: Path::new(&config.chatlog_path).is_file(),
                player_name: config.player_name.clone(),
            },
            hotbar_hooks_enabled: config.hotbar_hooks_enabled,
            repair_ocr_enabled: config.repair_ocr_enabled,
            end_of_session_armour_reminder_enabled: config.end_of_session_armour_reminder_enabled,
            developer_mode_enabled: config.developer_mode_enabled,
            mob_tracking_mode: config.mob_tracking_mode.clone(),
            mob_tracking_tag: config.mob_tracking_tag.clone(),
            hotbar: config.hotbar.clone(),
            trifecta,
            loot_filter_blacklist: config.loot_filter_blacklist.clone(),
            db_path: python_path_str(&self.data_dir.join(DB_FILE_NAME)),
            app_version: APP_VERSION.to_string(),
        })
    }

    /// The persisted overlay window position.
    pub async fn settings_overlay_position(&self) -> Result<OverlayPosition, ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("overlay position read"))?;
        Ok(OverlayPosition {
            x: config.overlay_x,
            y: config.overlay_y,
        })
    }

    /// Persist the overlay window position. Unlike the PATCH / reset
    /// writes it carries no producer side effects, so no watcher / hotbar
    /// / tracker signal follows.
    pub async fn settings_set_overlay_position(&self, x: i64, y: i64) -> Result<(), ApiError> {
        let mut updates = Map::new();
        updates.insert("overlay_x".into(), json!(x));
        updates.insert("overlay_y".into(), json!(y));
        let mut guard = self
            .config_service
            .lock()
            .map_err(|_| ApiError::invalid_state("config service lock poisoned"))?;
        guard
            .update(&updates)
            .map_err(ApiError::internal("overlay position write"))?;
        Ok(())
    }

    /// Apply a partial settings update: validate and write the present
    /// fields, signal the producers (the watcher on a `chatlog_path`
    /// change, the hotbar gate on a `hotbar_hooks_enabled` change, the
    /// tracker unconditionally so an in-flight session re-reads its
    /// config), and reply with the full assembled settings.
    pub async fn settings_update(&self, patch: SettingsPatch) -> Result<AppSettings, ApiError> {
        let mut updates = patch.into_updates();
        // An empty patch is the backend's 400 (nothing to update).
        if updates.is_empty() {
            return Err(ApiError::bad_request("No fields to update"));
        }
        // Lock, validate, and write inside a block so the (non-`Send`)
        // guard is gone before the `.await` below (the response assembly).
        let (validated_chatlog, hooks_present, hooks_value) = {
            let mut guard = self
                .config_service
                .lock()
                .map_err(|_| ApiError::invalid_state("config service lock poisoned"))?;
            // The candidate validates without mutating live state (the
            // backend's `clone_with_updates`); used for the mob-mode gate.
            let candidate = guard.clone_with_updates(&updates);
            // chatlog_path first (the backend's order): the 400 chain, then
            // the validated/expanduser-normalised path replaces the
            // submitted one so the write and the watcher restart use the
            // canonical form.
            let validated_chatlog = if updates.contains_key("chatlog_path") {
                let raw = updates
                    .get("chatlog_path")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let normalised = validate_chatlog_path(raw)?;
                updates.insert("chatlog_path".into(), Value::String(normalised.clone()));
                Some(normalised)
            } else {
                None
            };
            if candidate.mob_tracking_mode != "mob" && candidate.mob_tracking_mode != "tag" {
                return Err(ApiError::bad_request("Unknown mob tracking mode"));
            }
            let hooks_present = updates.contains_key("hotbar_hooks_enabled");
            guard
                .update(&updates)
                .map_err(ApiError::internal("settings write"))?;
            (
                validated_chatlog,
                hooks_present,
                guard.get().hotbar_hooks_enabled,
            )
        };

        if let Some(path) = validated_chatlog {
            self.watcher.restart(path);
        }
        if hooks_present {
            self.hotbar.set_hotbar_hooks_enabled(hooks_value);
        }
        self.tracker.reload_config();
        self.settings().await
    }

    /// The trifecta block: every preset validated against the live
    /// equipment library, with the active preset's readiness lifted to
    /// the top level (mirrors the backend's `_build_trifecta_response`).
    async fn trifecta_block(&self, config: &AppConfig) -> Result<TrifectaSettings, ApiError> {
        let mut presets = Vec::new();
        let mut active_ready = false;
        let mut active_message: Option<String> = None;
        let mut active_name: Option<String> = None;

        for preset in &config.trifecta_presets {
            let service_preset = TrifectaPreset {
                small_weapon_id: preset.small_weapon_id,
                big_weapon_id: preset.big_weapon_id,
                heal_id: preset.heal_id,
            };
            let (ready, message) = validate_trifecta(&self.db, Some(&service_preset))
                .await
                .map_err(ApiError::internal("trifecta validation"))?;
            presets.push(TrifectaPresetView {
                id: preset.id.clone(),
                name: preset.name.clone(),
                small_weapon_id: preset.small_weapon_id,
                big_weapon_id: preset.big_weapon_id,
                heal_id: preset.heal_id,
                ready,
                message: message.clone(),
            });
            if Some(preset.id.as_str()) == config.active_trifecta_preset_id.as_deref() {
                active_ready = ready;
                active_message = message;
                active_name = Some(preset.name.clone());
            }
        }

        Ok(TrifectaSettings {
            active_preset_id: config.active_trifecta_preset_id.clone(),
            active_preset_name: active_name,
            presets,
            ready: active_ready,
            message: active_message,
        })
    }
}

/// Mirror the backend's `_validate_chatlog_path`: a non-empty path whose
/// basename is `chat.log` (case-insensitive) and which is an existing
/// file. Returns the expanduser-normalised `str(Path(...))` on success
/// (so the stored value and the watcher restart both use the canonical
/// form), or the bad-request the frontend renders inline.
fn validate_chatlog_path(value: &str) -> Result<String, ApiError> {
    if value.is_empty() {
        return Err(ApiError::bad_request("chat.log path is required"));
    }
    let expanded = expanduser(value);
    let path = Path::new(&expanded);
    let basename_is_chatlog = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("chat.log"));
    if !basename_is_chatlog {
        return Err(ApiError::bad_request(
            "chat.log path must point to a chat.log file",
        ));
    }
    if !path.is_file() {
        return Err(ApiError::bad_request("chat.log path does not exist"));
    }
    Ok(python_path_str(path))
}

/// Expand a leading `~` to the user's home directory, mirroring
/// `pathlib.Path.expanduser` for the case the path picker produces (a
/// bare `~` or a `~/...` / `~\...` prefix). Other forms pass through
/// unchanged.
fn expanduser(value: &str) -> String {
    if let Some(rest) = value.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
            {
                return format!("{}{}", home.to_string_lossy(), rest);
            }
        }
    }
    value.to_string()
}

/// `str(pathlib.Path(...))` over the absolute forms the data-dir
/// resolution produces: Windows renders every separator as a backslash (a
/// forward-slash env override still reads back in the native form, as the
/// Python reference's `pathlib` normalisation does); other platforms keep
/// the path as built.
fn python_path_str(path: &Path) -> String {
    #[cfg(windows)]
    {
        use std::path::Component;
        let mut out = String::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    out.push_str(&prefix.as_os_str().to_string_lossy());
                }
                Component::RootDir => out.push('\\'),
                part => {
                    if !out.is_empty() && !out.ends_with('\\') {
                        out.push('\\');
                    }
                    out.push_str(&part.as_os_str().to_string_lossy());
                }
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn paths_render_in_the_native_windows_form() {
        assert_eq!(
            python_path_str(Path::new("E:/x/data/entropia_orme.db")),
            "E:\\x\\data\\entropia_orme.db",
        );
        assert_eq!(
            python_path_str(Path::new("E:\\already\\native")),
            "E:\\already\\native",
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn paths_render_as_built() {
        assert_eq!(
            python_path_str(Path::new("/tmp/data/x.db")),
            "/tmp/data/x.db"
        );
    }

    #[test]
    fn an_absent_field_is_omitted_while_an_explicit_null_preset_clears() {
        // A bare Option field: absent stays absent.
        let patch = SettingsPatch {
            player_name: Some("Mikel".into()),
            ..SettingsPatch::default()
        };
        let updates = patch.into_updates();
        assert_eq!(updates.get("player_name"), Some(&json!("Mikel")));
        assert!(!updates.contains_key("chatlog_path"));

        // The double-option preset id: present-null lands as a null in the
        // update map (clear the active preset), distinct from absent.
        let cleared: SettingsPatch =
            serde_json::from_value(json!({ "active_trifecta_preset_id": null })).unwrap();
        assert_eq!(
            cleared.into_updates().get("active_trifecta_preset_id"),
            Some(&Value::Null)
        );
        let untouched: SettingsPatch = serde_json::from_value(json!({})).unwrap();
        assert!(!untouched
            .into_updates()
            .contains_key("active_trifecta_preset_id"));
    }
}
