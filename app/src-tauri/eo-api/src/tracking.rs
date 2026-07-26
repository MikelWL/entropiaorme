//! The tracking family: the session-read surface (list, detail, tag /
//! manual-mob suggestions, the quest-link suggestion), the live producer
//! surface (start / stop / release-mob / manual-mob-lock / tag-lock / the
//! consolidated dashboard snapshot), the post-hoc session edits (rename /
//! restore mob, loot-item activate / deactivate, armour cost, quest-link
//! decision), and the one-shot repair-cost OCR read.
//!
//! Typed DTOs over the composed services. The family's SQL and its wire
//! shaping live in `eo_services::tracking_reads`; this facade owns the
//! DTO definitions and orchestration, bridging each computed value into
//! its declared DTO, whose serde field order is the golden-pinned wire
//! order.
//!
//! Contract lineage (ADR-0019):
//!
//! * The **snapshot** and the **quest-link decision** were served
//!   `response_model_exclude_unset` (a field explicitly set to null stayed
//!   on the wire; an unset field was omitted). A single typed struct with
//!   `#[serde(skip_serializing_if = "Option::is_none")]` cannot tell a
//!   present-null from an absent key, so both collapse to "omitted": the
//!   projection narrows from **exclude-unset to exclude-none** (a
//!   present-null field is dropped rather than serialised `null`). The
//!   generated TypeScript type is all-optional and every consumer reads
//!   these fields defensively, so null and absent are equivalent to them.
//! * Structurally-impossible transport legs retire with no consumer: the
//!   `tainted` surrogate-500 (a Rust `String` argument is always valid
//!   UTF-8), the decoded-slash framework 404 (typed args carry the value
//!   directly), the 503 substrate-unavailable floor (every handle is
//!   present by construction), the ETag conditional-GET contract (a typed
//!   command answers a body, not a status + headers), and the body-taint /
//!   int-parse 422s (typed args are pre-validated).

use eo_services::config_service::{active_trifecta_preset, load_config_readonly, AppConfig};
use eo_services::db::Db;
use eo_services::mob_lookup_service::{python_whitespace, MobLookupService};
use eo_services::quests::QuestError;
use eo_services::time::{local_isoformat, naive_to_epoch};
use eo_services::tracker::HuntTracker;
use eo_services::trifecta_service::{validate_trifecta, TrifectaPreset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use eo_services::tracking_reads::*;

use crate::Nullable;
use crate::{Api, ApiError};

// ── Constants ───────────────────────────────────────────────────────

/// The `TrackingSnapshot` response-model field order (the polymorphic
/// dashboard hydration shape). The snake-case status trio sits among the
/// camelCase headline numbers exactly as the model declares them.
const SNAPSHOT_FIELDS: [&str; 40] = [
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
    "harvestSwings",
    "harvestSuccesses",
    "harvestLoot",
    "harvestCost",
    "harvestGuardrail",
    "warnings",
];

/// The repair-scan response-model field order (`exclude_unset`).
const REPAIR_FIELDS: [&str; 4] = ["cost_ped", "raw_text", "confidence", "error"];

fn edit_error(context: &'static str) -> impl Fn(EditError) -> ApiError {
    move |error| match error {
        EditError::NotFound(message) => ApiError::not_found(message),
        EditError::Conflict(message) => ApiError::conflict(message),
        EditError::BadRequest(message) => ApiError::bad_request(message),
        EditError::Internal => ApiError::invalid_state(format!("{context} failed")),
    }
}

// ── Closed vocabularies ─────────────────────────────────────────────
//
// The string fields whose value sets are closed in code, stated as serde
// enums so the generated TypeScript carries the literal unions and the
// compiler owns the vocabulary on both sides of the boundary. Each
// variant's serialised form is byte-identical to the string it replaces.

/// The session state a tracking readout reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TrackingState {
    Idle,
    Active,
}

/// Which attribution source prices weapon shots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WeaponAttribution {
    Hotbar,
    Trifecta,
}

/// Mob-attribution input mode a session is captured under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MobEntryMode {
    Mob,
    Tag,
}

/// How the current mob label was locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MobSource {
    Manual,
    Tag,
}

/// The broad notable-event family (drives styling on the frontend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NotableEventCategory {
    Global,
    Hof,
    Quest,
    Warning,
}

/// The canonical notable-event subtype the services store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotableEventType {
    GlobalKill,
    GlobalItem,
    HofKill,
    HofItem,
    QuestStarted,
    QuestCompleted,
    QuestCompletedPes,
}

/// What the quest-link suggestion proposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum QuestLinkSuggestionType {
    Quest,
    Playlist,
    None,
}

/// Why the quest-link suggestion took its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestLinkReason {
    SingleQuest,
    ExactPlaylist,
    NoCompletions,
    Unclean,
    AmbiguousPlaylist,
    Declined,
    AlreadyLinked,
}

/// The quest-link decision's outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum QuestLinkStatus {
    Linked,
    Declined,
}

/// Which entity a linked decision bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum QuestLinkType {
    Quest,
    Playlist,
}

// ── Response DTOs ───────────────────────────────────────────────────

/// One row of the session list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackingSession {
    pub id: String,
    pub start_time: Nullable<String>,
    pub end_time: Nullable<String>,
    pub duration: i64,
    pub primary_mobs: Vec<String>,
    pub primary_weapons: Vec<String>,
    pub cost: f64,
    pub returns: f64,
    pub net: f64,
    pub return_rate: f64,
    pub globals: i64,
    pub hofs: i64,
}

/// A keyset page of session-list rows plus the opaque cursor for the
/// next page (`null` on the last page) and the whole-table session
/// count, mirroring the ledger's [`crate::analytics::LedgerPage`] shape.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub sessions: Vec<TrackingSession>,
    pub next_cursor: Nullable<String>,
    pub total: i64,
}

/// The session-detail cost split.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    pub weapon_cost: f64,
    pub heal_cost: f64,
    pub enhancer_cost: f64,
    pub armour_cost: f64,
    /// Harvesting (tree cutting) swing decay.
    pub harvest_cost: f64,
}

/// The session-detail headline summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub cost: f64,
    pub returns: f64,
    pub pes: f64,
    pub net: f64,
    pub return_rate: f64,
    pub kills: i64,
    pub duration: i64,
    pub cost_breakdown: CostBreakdown,
}

/// One notable event in a session detail.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotableEvent {
    #[serde(rename = "type")]
    pub kind: NotableEventCategory,
    pub event_type: NotableEventType,
    pub target: Nullable<String>,
    pub item: Nullable<String>,
    pub value: f64,
}

/// One aggregated loot line.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LootEntry {
    pub name: String,
    pub quantity: i64,
    pub tt_value: f64,
}

/// One per-mob breakdown row.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MobBreakdownRow {
    pub current_name: String,
    pub original_name: Nullable<String>,
    pub kill_count: i64,
}

/// One per-tool aggregate.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub weapon_name: String,
    pub shots_fired: i64,
    pub damage_dealt: f64,
    pub crits: i64,
    pub cost_attributed: f64,
}

/// One per-skill gain (attributes excluded).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillGain {
    pub skill_name: String,
    pub level: f64,
    pub tt_value_gained: f64,
}

/// A session's harvesting (tree cutting) totals: every swing is a
/// counted event (successes arrive as wood loot groups, fails as the
/// explicit harvest-fail line).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestSummary {
    pub swings: i64,
    pub successes: i64,
    pub loot_tt: f64,
    pub cost: f64,
}

/// The full session detail.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session_id: String,
    pub summary: SessionSummary,
    pub harvest: HarvestSummary,
    pub mob_entry_mode: MobEntryMode,
    pub notable_events: Vec<NotableEvent>,
    pub loot_breakdown: Vec<LootEntry>,
    pub deactivated_loot_breakdown: Vec<LootEntry>,
    pub mob_breakdown: Vec<MobBreakdownRow>,
    pub effective_loot: f64,
    pub tool_stats: Vec<ToolStat>,
    pub skill_gains: Vec<SkillGain>,
}

/// One manual-mob autocomplete suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManualMobSuggestion {
    pub display: String,
    pub species: String,
    pub maturity: String,
}

/// The quest-link suggestion (all seven fields always present).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionQuestLinkSuggestion {
    pub session_id: String,
    pub suggestion_type: Nullable<QuestLinkSuggestionType>,
    pub reason: Nullable<QuestLinkReason>,
    pub quest_id: Nullable<String>,
    pub quest_name: Nullable<String>,
    pub playlist_id: Nullable<String>,
    pub playlist_name: Nullable<String>,
}

/// One preset reference inside the trifecta attribution summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrifectaPresetRef {
    pub id: String,
    pub name: String,
}

/// The trifecta attribution summary (present when trifecta mode is active
/// and a preset or binding exists). Its members are always emitted (a
/// null binding stays on the wire), so none skip.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrifectaAttribution {
    pub active_preset_id: Nullable<String>,
    pub preset_name: Nullable<String>,
    pub presets: Vec<TrifectaPresetRef>,
    pub small_weapon: Nullable<String>,
    pub big_weapon: Nullable<String>,
    pub heal_tool: Nullable<String>,
}

/// One recent event in the active-session snapshot feed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecentEvent {
    #[serde(rename = "type")]
    pub kind: NotableEventCategory,
    pub description: String,
    pub value: f64,
    #[serde(rename = "eventType")]
    pub event_type: NotableEventType,
    pub timestamp: Nullable<String>,
    pub id: String,
}

/// One active-session warning.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    #[serde(rename = "type")]
    pub kind: NotableEventCategory,
    pub description: String,
    pub value: f64,
}

/// The consolidated dashboard hydration snapshot: the polymorphic idle /
/// active shape, in the model's declaration order. Every field is optional
/// and skipped when absent; under the ratified exclude-unset -> exclude-none
/// movement a present-null field is dropped rather than serialised null.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrackingSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TrackingState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotbar_listener_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_attribution: Option<WeaponAttribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_ocr_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_session_armour_reminder_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mob_entry_mode: Option<MobEntryMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_mob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mob_source: Option<MobSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trifecta_attribution: Option<TrifectaAttribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_events: Option<Vec<RecentEvent>>,
    #[serde(rename = "session_id", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "started_at", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "kill_count", skip_serializing_if = "Option::is_none")]
    pub kill_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage_dealt_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_damage_dealt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weapon_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shots_fired_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_hits_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_damage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub globals_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hofs_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_kill_loot: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_avg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier_history: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cumulative_net_history: Option<Vec<f64>>,
    /// Harvesting swings this session (successes plus explicit fails).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_swings: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_successes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_loot: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_cost: Option<f64>,
    /// The standing harvest-guardrail disagreement; present only while
    /// the loot evidence contradicts the hotbar-equipped tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_guardrail: Option<HarvestGuardrailAlert>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<Warning>>,
}

/// A harvest-guardrail disagreement on the snapshot: the tool the loot
/// evidence expects for the board-output class, the tool the hotbar believed
/// (null when none was equipped), and when the evidence arrived.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarvestGuardrailAlert {
    pub expected_tool: String,
    pub observed_tool: Nullable<String>,
    pub tree_size: TreeSizeName,
    pub at_epoch: f64,
}

/// The guardrail's closed board-yield vocabulary: the yield tier evidenced by
/// a swing's board output. The type name is the guardrail alias described on
/// `TreeSize` in the services layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TreeSizeName {
    Short,
    Long,
    Huge,
}

/// The start lifecycle acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartResult {
    pub session_id: String,
    pub started_at: String,
    pub status: TrackingState,
}

/// The stop lifecycle acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StopResult {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Nullable<String>,
    pub kill_count: i64,
}

/// The release-mob acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseResult {
    pub released: Nullable<String>,
}

/// The manual-mob-lock acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualMobLockResult {
    pub mob_name: String,
    pub species: String,
    pub maturity: String,
}

/// The tag-lock acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagLockResult {
    pub tag: String,
}

/// The mob rename / restore result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MobEditResult {
    pub session_id: String,
    pub mob_name: String,
    pub kill_count: i64,
}

/// The loot-item activate / deactivate result.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LootItemEditResult {
    pub session_id: String,
    pub item_name: String,
    pub affected_rows: i64,
    pub total_value_delta: f64,
    pub session_total_returns: f64,
}

/// The armour-cost result (echoes the submitted value, not the new total).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArmourCostResult {
    pub session_id: String,
    pub armour_cost: f64,
}

/// The quest-link decision: `accept` carries the full link object, `decline`
/// only `sessionId` / `status`. The accept-only fields skip when absent
/// (exclude-unset -> exclude-none movement: a present-null link field is
/// dropped rather than serialised null).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuestLinkDecision {
    pub session_id: String,
    pub status: QuestLinkStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type: Option<QuestLinkType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quest_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_name: Option<String>,
}

/// The one-shot repair-cost read (`exclude_unset`): the cost / raw text /
/// confidence on success, plus `error` on a logical refusal.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepairScanResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_ped: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Facade methods ──────────────────────────────────────────────────

impl Api {
    /// One keyset page of sessions (newest first), each shaped from its
    /// summary or raw tables. Heals ended-session summaries first (a write
    /// on the read path, preserved from the reference). A malformed cursor
    /// is a bad-request.
    pub async fn tracking_sessions(
        &self,
        cursor: Option<String>,
        limit: Option<i64>,
    ) -> Result<SessionPage, ApiError> {
        let seek = match cursor.as_deref() {
            None => None,
            Some(token) => match decode_session_cursor(token) {
                Some(key) => Some(key),
                None => return Err(ApiError::bad_request("Invalid cursor")),
            },
        };
        let now = naive_to_epoch(self.clock.now());
        let page = list_sessions_impl(&self.db, now, seek, limit)
            .await
            .map_err(ApiError::internal("tracking sessions"))?;
        let sessions: Vec<TrackingSession> = serde_json::from_value(page.sessions)
            .map_err(ApiError::internal("tracking sessions shaping"))?;
        Ok(SessionPage {
            sessions,
            next_cursor: page.next_cursor.into(),
            total: page.total,
        })
    }

    /// One session's full detail; an absent session is a not-found.
    pub async fn tracking_session_detail(
        &self,
        session_id: String,
    ) -> Result<SessionDetail, ApiError> {
        let now = naive_to_epoch(self.clock.now());
        match get_session_impl(&self.db, &session_id, now)
            .await
            .map_err(ApiError::internal("tracking session detail"))?
        {
            Some(value) => serde_json::from_value(value)
                .map_err(ApiError::internal("tracking session detail shaping")),
            None => Err(ApiError::not_found("Session not found")),
        }
    }

    /// Free-text tag autocomplete over species-less kills.
    pub async fn tracking_tag_suggestions(
        &self,
        q: String,
        limit: Option<i64>,
    ) -> Result<Vec<String>, ApiError> {
        tag_suggestions_impl(&self.db, &q, limit.unwrap_or(10))
            .await
            .map_err(ApiError::internal("tracking tag suggestions"))
    }

    /// Catalogue mob-name autocomplete. The tag-mode 409 fires first (before
    /// the empty-query short-circuit), consulting the per-session captured
    /// mode when tracking, else the live config.
    pub async fn tracking_manual_mob_suggestions(
        &self,
        q: String,
        limit: Option<i64>,
    ) -> Result<Vec<ManualMobSuggestion>, ApiError> {
        let tag_mode = if self.tracker.is_tracking() {
            self.tracker.is_session_tag_mode()
        } else {
            load_config_readonly(&self.data_dir)
                .map_err(ApiError::internal("manual mob suggestions config"))?
                .mob_tracking_mode
                == "tag"
        };
        if tag_mode {
            return Err(ApiError::conflict("Tag mode disables manual mob selection"));
        }

        let query = q.trim_matches(python_whitespace);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let bounded = limit.unwrap_or(10).clamp(1, 20) as usize;
        let lookup = MobLookupService::new(&self.game_data);
        lookup
            .search_mob_names(query, bounded)
            .into_iter()
            .map(|value| {
                serde_json::from_value(value)
                    .map_err(ApiError::internal("manual mob suggestions shaping"))
            })
            .collect()
    }

    /// The consolidated dashboard hydration snapshot.
    pub async fn tracking_snapshot(&self) -> Result<TrackingSnapshot, ApiError> {
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("tracking snapshot config"))?;
        let value =
            build_snapshot_value(&self.db, &self.tracker, &config, self.hotbar.is_running())
                .await?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking snapshot shaping"))
    }

    /// The curated quest-link suggestion for a completed session; an absent
    /// session is a not-found.
    pub async fn tracking_quest_link_suggestion(
        &self,
        session_id: String,
    ) -> Result<SessionQuestLinkSuggestion, ApiError> {
        if !self.tracking_session_exists(&session_id).await? {
            return Err(ApiError::not_found("Session not found"));
        }
        let suggestion = self
            .quests
            .get_session_link_suggestion(&session_id)
            .await
            .map_err(ApiError::internal("quest-link suggestion"))?;
        let value = format_quest_link_suggestion(&session_id, &suggestion);
        serde_json::from_value(value).map_err(ApiError::internal("quest-link suggestion shaping"))
    }

    /// Begin a tracking session. 409 if one is already active (before the
    /// attribution gate), 400 if the attribution requirement is unmet.
    pub async fn tracking_start(&self) -> Result<StartResult, ApiError> {
        if self.tracker.is_tracking() {
            return Err(ApiError::conflict("Session already active"));
        }
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("tracking start config"))?;
        let (ready, message) = if config.hotbar_hooks_enabled {
            validate_hotbar(&config)
        } else {
            let preset = active_trifecta_preset(&config).map(|p| TrifectaPreset {
                small_weapon_id: p.small_weapon_id,
                big_weapon_id: p.big_weapon_id,
                heal_id: p.heal_id,
            });
            let (ready, reason) = validate_trifecta(&self.db, preset.as_ref())
                .await
                .map_err(ApiError::internal("tracking start validate"))?;
            (
                ready,
                reason.or_else(|| {
                    Some(
                        "Configure the trifecta in the Equipment page before tracking.".to_string(),
                    )
                }),
            )
        };
        if !ready {
            let detail_message = message.unwrap_or_else(|| {
                "Configure the trifecta in the Equipment page before tracking.".to_string()
            });
            return Err(ApiError::bad_request(detail_message));
        }
        let session = self
            .tracker
            .start_session()
            .await
            .map_err(ApiError::internal("tracking start"))?;
        Ok(StartResult {
            session_id: session.id,
            started_at: local_isoformat(session.start_time),
            status: TrackingState::Active,
        })
    }

    /// End the active tracking session. 409 if none is active.
    pub async fn tracking_stop(&self) -> Result<StopResult, ApiError> {
        if !self.tracker.is_tracking() {
            return Err(ApiError::conflict("No active session"));
        }
        match self
            .tracker
            .stop_session()
            .await
            .map_err(ApiError::internal("tracking stop"))?
        {
            Some(session) => Ok(StopResult {
                session_id: session.id.clone(),
                started_at: local_isoformat(session.start_time),
                ended_at: session.end_time.map(local_isoformat).into(),
                kill_count: session.kills.len() as i64,
            }),
            // Defensive: `is_tracking` was true above, so a None is a broken
            // invariant. The transport's 500 message does not survive (the
            // boundary reply is fixed); logged server-side.
            None => Err(ApiError::invalid_state("tracking stop returned no session")),
        }
    }

    /// Clear the locked mob or tag, echoing what was released.
    pub async fn tracking_release_mob(&self) -> Result<ReleaseResult, ApiError> {
        // The (non-`Send`) config guard is scoped around each read/write
        // so it is never held across the tracker's await points; the
        // release-then-write order within each branch is unchanged.
        let lock_config = || {
            self.config_service
                .lock()
                .map_err(|_| ApiError::invalid_state("release mob: poisoned config lock"))
        };
        let released = if self.tracker.is_tracking() && self.tracker.is_session_tag_mode() {
            let released = self.tracker.release_current_mob().await;
            lock_config()?
                .update(&clear_tag())
                .map_err(ApiError::internal("release mob"))?;
            released.map(Value::from).unwrap_or(Value::Null)
        } else if !self.tracker.is_tracking() {
            let mut guard = lock_config()?;
            if guard.get().mob_tracking_mode == "tag" {
                let trimmed = guard.get().mob_tracking_tag.trim().to_string();
                let released = if trimmed.is_empty() {
                    Value::Null
                } else {
                    Value::String(trimmed)
                };
                guard
                    .update(&clear_tag())
                    .map_err(ApiError::internal("release mob"))?;
                released
            } else {
                let species = guard.get().manual_mob_species.trim().to_string();
                let maturity = guard.get().manual_mob_maturity.trim().to_string();
                let released = mob_display(&species, &maturity);
                guard
                    .update(&clear_manual_mob())
                    .map_err(ApiError::internal("release mob"))?;
                released
            }
        } else {
            let released = self.tracker.release_current_mob().await;
            lock_config()?
                .update(&clear_manual_mob())
                .map_err(ApiError::internal("release mob"))?;
            released.map(Value::from).unwrap_or(Value::Null)
        };
        Ok(ReleaseResult {
            released: opt_str(&released).into(),
        })
    }

    /// Lock a catalogue mob for manual kill stamping. 409 in tag mode, 400
    /// when the mob is absent from the catalogue.
    pub async fn tracking_manual_mob_lock(
        &self,
        species: String,
        maturity: Option<String>,
    ) -> Result<ManualMobLockResult, ApiError> {
        let maturity = maturity.unwrap_or_default();
        let species = species.trim();
        let maturity = maturity.trim();
        let display = if maturity.is_empty() {
            species.to_string()
        } else {
            format!("{maturity} {species}")
        };
        // Validate and write inside a block so the (non-`Send`) config
        // guard is gone before the tracker await below.
        {
            let Ok(mut guard) = self.config_service.lock() else {
                return Err(ApiError::invalid_state(
                    "manual mob lock: poisoned config lock",
                ));
            };
            let idle_tag_mode =
                !self.tracker.is_tracking() && guard.get().mob_tracking_mode == "tag";
            if (self.tracker.is_tracking() && self.tracker.is_session_tag_mode()) || idle_tag_mode {
                return Err(ApiError::conflict("Tag mode disables manual mob selection"));
            }
            if !MobLookupService::new(&self.game_data).has_mob_name(species, maturity) {
                return Err(ApiError::bad_request("Mob is not present in the catalogue"));
            }
            let mut updates = Map::new();
            updates.insert("manual_mob_species".into(), json!(species));
            updates.insert("manual_mob_maturity".into(), json!(maturity));
            guard
                .update(&updates)
                .map_err(ApiError::internal("manual mob lock"))?;
        }
        if self.tracker.is_tracking()
            && self
                .tracker
                .set_manual_mob(&display, species, maturity)
                .await
                .is_err()
        {
            // The gate cleared an active non-tag session; the only reachable
            // error is the live config having flipped to tag mode since the
            // session started. Mirror the reference's post-write 500.
            return Err(ApiError::invalid_state(
                "manual mob lock: session flipped to tag mode",
            ));
        }
        Ok(ManualMobLockResult {
            mob_name: display,
            species: species.to_string(),
            maturity: maturity.to_string(),
        })
    }

    /// Set the active free-text tag. 409 when not in tag mode, 400 on an
    /// empty tag.
    pub async fn tracking_tag_lock(&self, tag: String) -> Result<TagLockResult, ApiError> {
        let tag = tag.trim();
        // Validate and write inside a block so the (non-`Send`) config
        // guard is gone before the tracker await below.
        {
            let Ok(mut guard) = self.config_service.lock() else {
                return Err(ApiError::invalid_state("tag lock: poisoned config lock"));
            };
            if self.tracker.is_tracking() {
                if !self.tracker.is_session_tag_mode() {
                    return Err(ApiError::conflict("Active session is not in tag mode"));
                }
            } else if guard.get().mob_tracking_mode != "tag" {
                return Err(ApiError::conflict("Tag mode is not enabled"));
            }
            if tag.is_empty() {
                return Err(ApiError::bad_request("Tag cannot be empty"));
            }
            let mut updates = Map::new();
            updates.insert("mob_tracking_tag".into(), json!(tag));
            guard
                .update(&updates)
                .map_err(ApiError::internal("tag lock"))?;
        }
        if self.tracker.is_tracking() {
            let _ = self.tracker.set_manual_tag(tag).await;
        }
        Ok(TagLockResult {
            tag: tag.to_string(),
        })
    }

    /// Rename a mob across an ended session.
    pub async fn tracking_rename_mob(
        &self,
        session_id: String,
        from_mob_name: String,
        to_mob_name: String,
    ) -> Result<MobEditResult, ApiError> {
        let value = rename_session_mob_impl(&self.db, &session_id, &from_mob_name, &to_mob_name)
            .await
            .map_err(edit_error("tracking rename mob"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking rename mob shaping"))
    }

    /// Restore a renamed mob to its preserved original.
    pub async fn tracking_restore_mob(
        &self,
        session_id: String,
        current_mob_name: String,
    ) -> Result<MobEditResult, ApiError> {
        let value = restore_session_mob_impl(&self.db, &session_id, &current_mob_name)
            .await
            .map_err(edit_error("tracking restore mob"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking restore mob shaping"))
    }

    /// Re-activate a deactivated loot line.
    pub async fn tracking_loot_item_activate(
        &self,
        session_id: String,
        item_name: String,
    ) -> Result<LootItemEditResult, ApiError> {
        let value = bulk_flip_loot_item(&self.db, &session_id, &item_name, "active")
            .await
            .map_err(edit_error("tracking loot item activate"))?;
        serde_json::from_value(value)
            .map_err(ApiError::internal("tracking loot item activate shaping"))
    }

    /// Deactivate a loot line.
    pub async fn tracking_loot_item_deactivate(
        &self,
        session_id: String,
        item_name: String,
    ) -> Result<LootItemEditResult, ApiError> {
        let value = bulk_flip_loot_item(&self.db, &session_id, &item_name, "deactivated")
            .await
            .map_err(edit_error("tracking loot item deactivate"))?;
        serde_json::from_value(value)
            .map_err(ApiError::internal("tracking loot item deactivate shaping"))
    }

    /// Add an armour cost to a session (no active-session guard; 404 only
    /// when absent). Echoes the submitted value.
    pub async fn tracking_armour_cost(
        &self,
        session_id: String,
        cost: f64,
    ) -> Result<ArmourCostResult, ApiError> {
        let value = set_armour_cost_impl(&self.db, &session_id, cost)
            .await
            .map_err(edit_error("tracking armour cost"))?;
        serde_json::from_value(value).map_err(ApiError::internal("tracking armour cost shaping"))
    }

    /// Accept or decline the curated quest-link suggestion. 404 for an absent
    /// session, 400 for an unknown action; accept with no linkable suggestion
    /// is a 409.
    pub async fn tracking_quest_link(
        &self,
        session_id: String,
        action: String,
    ) -> Result<QuestLinkDecision, ApiError> {
        if !self.tracking_session_exists(&session_id).await? {
            return Err(ApiError::not_found("Session not found"));
        }
        let action = action.trim().to_lowercase();
        if action == "accept" {
            return match self
                .quests
                .accept_session_link_suggestion(&session_id)
                .await
            {
                Ok(suggestion) => Ok(QuestLinkDecision {
                    session_id: session_id.clone(),
                    status: QuestLinkStatus::Linked,
                    // Parses the service's untyped suggestion value: the two
                    // link kinds map to their variants, and the link type is
                    // absent for any other shape.
                    link_type: match suggestion["suggestion_type"].as_str() {
                        Some("quest") => Some(QuestLinkType::Quest),
                        Some("playlist") => Some(QuestLinkType::Playlist),
                        _ => None,
                    },
                    quest_id: opt_str(&str_id_or_null(&suggestion["quest_id"])),
                    quest_name: opt_str(&suggestion["quest_name"]),
                    playlist_id: opt_str(&str_id_or_null(&suggestion["playlist_id"])),
                    playlist_name: opt_str(&suggestion["playlist_name"]),
                }),
                Err(QuestError::Invalid(message)) => Err(ApiError::conflict(message)),
                Err(_) => Err(ApiError::invalid_state("quest-link accept")),
            };
        }
        if action == "decline" {
            self.quests
                .decline_session_link(&session_id)
                .await
                .map_err(ApiError::internal("quest-link decline"))?;
            return Ok(QuestLinkDecision {
                session_id,
                status: QuestLinkStatus::Declined,
                link_type: None,
                quest_id: None,
                quest_name: None,
                playlist_id: None,
                playlist_name: None,
            });
        }
        Err(ApiError::bad_request(
            "Action must be 'accept' or 'decline'",
        ))
    }

    /// The one-shot repair-cost OCR read, gated on `repair_ocr_enabled`
    /// (400 when disabled). The `session_id` is unused (the reference
    /// ignores it too); it stays in the signature for the route mapping.
    pub fn tracking_repair_scan(&self, session_id: String) -> Result<RepairScanResult, ApiError> {
        let _ = session_id;
        let config = load_config_readonly(&self.data_dir)
            .map_err(ApiError::internal("repair scan config"))?;
        if !config.repair_ocr_enabled {
            return Err(ApiError::bad_request("Repair OCR is disabled"));
        }
        let value = project(&self.repair_ocr.scan_repair_cost(), &REPAIR_FIELDS);
        serde_json::from_value(value).map_err(ApiError::internal("repair scan shaping"))
    }

    /// Delete a session and all of its data (an active session cannot be
    /// deleted; a missing one is a not-found). The rollups are repaired for
    /// the days it touched.
    pub async fn tracking_session_delete(&self, session_id: String) -> Result<(), ApiError> {
        delete_session_impl(&self.db, &session_id)
            .await
            .map_err(edit_error("tracking session delete"))
    }

    // ── Private snapshot assembly ────────────────────────────────────

    /// The session-existence precondition the quest-link operations apply.
    async fn tracking_session_exists(&self, session_id: &str) -> Result<bool, ApiError> {
        session_exists(&self.db, session_id)
            .await
            .map_err(ApiError::internal("session existence"))
    }
}

// ── Snapshot assembly (shared by the live and demo snapshots) ────────

/// Assemble the projected snapshot value from the tracker readout, the
/// resolved config, and the hotbar listener's running state. A free
/// function (rather than an `Api` method) so both the live snapshot and
/// the guide-mode demo snapshot, which runs over its own parallel tracker
/// and database, share one assembly.
pub(crate) async fn build_snapshot_value(
    db: &Db,
    tracker: &HuntTracker,
    config: &AppConfig,
    hotbar_active: bool,
) -> Result<Value, ApiError> {
    let weapon_attribution = if config.hotbar_hooks_enabled {
        "hotbar"
    } else {
        "trifecta"
    };
    let trifecta_attribution = if weapon_attribution == "trifecta" {
        trifecta_attribution_summary(db, config)
            .await
            .map_err(ApiError::internal("snapshot trifecta summary"))?
    } else {
        Value::Null
    };
    let readout = tracker
        .snapshot()
        .await
        .map_err(ApiError::internal("snapshot readout"))?;
    let current_tool = match &readout.current_tool {
        Some(tool) => Value::String(tool.clone()),
        None => Value::Null,
    };
    // The stored mode is free text (config / session row); recover
    // anything outside the two modes to the "mob" default, exactly as
    // the settings read does, so the snapshot's closed vocabulary holds
    // for a hand-edited store rather than failing the whole hydration.
    let mob_entry_mode = |stored: &str| if stored == "tag" { "tag" } else { "mob" };

    let value = match &readout.active {
        None => {
            let (current_mob, mob_source) = configured_manual_label(config);
            json!({
                "status": "idle",
                "hotbarListenerActive": hotbar_active,
                "weaponAttribution": weapon_attribution,
                "repairOcrEnabled": config.repair_ocr_enabled,
                "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                "currentTool": current_tool,
                "trifectaAttribution": trifecta_attribution,
                "mobEntryMode": mob_entry_mode(&config.mob_tracking_mode),
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
                        json!({
                            "type": notable_event_category(event_type),
                            "description": notable_event_description(event_type, mob_or_item, *value_ped),
                            "value": *value_ped,
                            "eventType": event_type.clone(),
                            "timestamp": event_ts_to_iso(*ts),
                            "id": format!("ne-{index}"),
                        })
                    })
                    .collect();
            let warnings: Vec<Value> = active
                .warnings
                .iter()
                .map(|message| json!({"type": "warning", "description": message, "value": 0.0}))
                .collect();
            // The guardrail key rides only while a disagreement stands
            // (`exclude_unset`): absent is the quiet steady state.
            let harvest_guardrail = active.harvest_guardrail_mismatch.as_ref().map(|mismatch| {
                json!({
                    "expectedTool": mismatch.expected_tool.clone(),
                    "observedTool": mismatch.observed_tool.clone(),
                    "treeSize": mismatch.tree_size.clone(),
                    "atEpoch": mismatch.at_epoch,
                })
            });
            let mut value = json!({
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
                "harvestSwings": active.harvest_swings,
                "harvestSuccesses": active.harvest_successes,
                "harvestLoot": active.harvest_loot,
                "harvestCost": active.harvest_cost,
                "hotbarListenerActive": hotbar_active,
                "weaponAttribution": weapon_attribution,
                "repairOcrEnabled": config.repair_ocr_enabled,
                "endOfSessionArmourReminderEnabled": config.end_of_session_armour_reminder_enabled,
                "currentTool": current_tool,
                "trifectaAttribution": trifecta_attribution,
                "mobEntryMode": mob_entry_mode(&active.mob_entry_mode),
                "currentMob": active.current_mob.clone(),
                "mobSource": active.mob_source.clone(),
                "recentEvents": recent_events,
                "warnings": warnings,
            });
            if let (Some(mismatch), Some(object)) = (harvest_guardrail, value.as_object_mut()) {
                object.insert("harvestGuardrail".to_string(), mismatch);
            }
            value
        }
    };
    Ok(project(&value, &SNAPSHOT_FIELDS))
}
