//! The command manifest: the single machine-readable description of the
//! typed IPC surface, walked by `cargo xtask gen-ts` to emit the
//! TypeScript bindings.
//!
//! Every entry names one Tauri command (declared with
//! `rename_all = "snake_case"`, so the invoke argument keys are exactly
//! the names recorded here) together with the JSON Schemas of its
//! arguments and return type, read straight off the DTOs' serde
//! attributes. The shell asserts its registered command set against
//! this manifest, so a command cannot ship unbound and a binding cannot
//! outlive its command.

use schemars::schema_for;
use serde_json::Value;

use crate::analytics::{
    AnalyticsActivity, AnalyticsOverview, InventoryItem, InventoryItemInput, InventoryPatch,
    InventorySellInput, InventorySellResult, LedgerEntryInput, LedgerItem, LedgerPage,
    LedgerPreset, LedgerPresetInput,
};
use crate::character::{
    ActivityRecommenderQuery, ActivityRecommenderResult, CalibrationStatus,
    CharacterProspectOptions, ComputedCharacterStats, HpOptimizerResult, PathOptimizerResult,
    ProfessionLevel, ProfessionOptimizerResult, ProspectQuery, ProspectResult, SkillLevel,
};
use crate::codex::{
    CodexCalibrateResult, CodexClaimResult, CodexMasteryClaimResult, CodexMetaAttribute,
    CodexMetaClaimResult, CodexRecommendTarget, CodexSkillOption, CodexSpecies, CodexSpeciesRanks,
};
use crate::dev::{CompactResult, CrashReportingStatus, MetricsSnapshot, RebuildReport};
use crate::equipment::{
    EquipmentDetail, EquipmentRequest, EquipmentSearchHit, EquipmentSummary, SearchKind,
};
use crate::market::{
    MarketBreakEven, MarketCommitResult, MarketContributionBatch, MarketHistoryPoint,
    MarketHorizon, MarketMobRankingRow, MarketOverviewRow, MarketPastePreview,
};
use crate::quests::{
    PlaylistAnalyticsRow, PlaylistInput, Quest, QuestAnalyticsRow, QuestInput, QuestPlaylist,
};
use crate::scan::{
    AcceptResult, CaptureResult, RejectResult, ScanStatus, SkillScanPending, SpacebarResult,
    UndoResult,
};
use crate::settings::{AppSettings, OverlayPosition, SettingsPatch};
use crate::tracking::{
    ArmourCostResult, LootItemEditResult, ManualMobLockResult, ManualMobSuggestion, MobEditResult,
    QuestLinkDecision, ReleaseResult, RepairScanResult, SessionDetail, SessionQuestLinkSuggestion,
    StartResult, StopResult, TagLockResult, TrackingSession, TrackingSnapshot,
};
use crate::ApiError;
use crate::Nullable;

/// One argument of a typed command.
pub struct ArgSpec {
    pub name: &'static str,
    pub schema: Value,
}

/// One typed command: its invoke name, arguments, and return schema
/// (`None` for a void return). Schemas are plain JSON values so the
/// generator needs no schema-crate dependency of its own.
pub struct CommandSpec {
    pub name: &'static str,
    pub args: Vec<ArgSpec>,
    pub returns: Option<Value>,
}

/// The full typed command surface, in emission order.
pub fn manifest() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            name: "equipment_search",
            args: vec![
                ArgSpec {
                    name: "q",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "kind",
                    schema: schema(schema_for!(SearchKind)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<EquipmentSearchHit>))),
        },
        CommandSpec {
            name: "equipment_library",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<EquipmentSummary>))),
        },
        CommandSpec {
            name: "equipment_add",
            args: vec![ArgSpec {
                name: "req",
                schema: schema(schema_for!(EquipmentRequest)),
            }],
            returns: Some(schema(schema_for!(EquipmentSummary))),
        },
        CommandSpec {
            name: "equipment_update",
            args: vec![
                ArgSpec {
                    name: "item_id",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "req",
                    schema: schema(schema_for!(EquipmentRequest)),
                },
            ],
            returns: Some(schema(schema_for!(EquipmentSummary))),
        },
        CommandSpec {
            name: "equipment_delete",
            args: vec![ArgSpec {
                name: "item_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "equipment_detail",
            args: vec![ArgSpec {
                name: "item_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: Some(schema(schema_for!(EquipmentDetail))),
        },
        CommandSpec {
            name: "character_calibration",
            args: Vec::new(),
            returns: Some(schema(schema_for!(CalibrationStatus))),
        },
        CommandSpec {
            name: "character_stats",
            args: Vec::new(),
            returns: Some(schema(schema_for!(ComputedCharacterStats))),
        },
        CommandSpec {
            name: "character_skills",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<SkillLevel>))),
        },
        CommandSpec {
            name: "character_professions",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<ProfessionLevel>))),
        },
        CommandSpec {
            name: "character_prospect_options",
            args: Vec::new(),
            returns: Some(schema(schema_for!(CharacterProspectOptions))),
        },
        CommandSpec {
            name: "character_prospect",
            args: vec![ArgSpec {
                name: "query",
                schema: schema(schema_for!(ProspectQuery)),
            }],
            returns: Some(schema(schema_for!(ProspectResult))),
        },
        CommandSpec {
            name: "character_profession_optimizer",
            args: vec![ArgSpec {
                name: "profession",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(ProfessionOptimizerResult))),
        },
        CommandSpec {
            name: "character_path_optimizer",
            args: vec![
                ArgSpec {
                    name: "profession",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "target_level",
                    schema: schema(schema_for!(Option<f64>)),
                },
                ArgSpec {
                    name: "ped_budget",
                    schema: schema(schema_for!(Option<f64>)),
                },
            ],
            returns: Some(schema(schema_for!(PathOptimizerResult))),
        },
        CommandSpec {
            name: "character_hp_optimizer",
            args: Vec::new(),
            returns: Some(schema(schema_for!(HpOptimizerResult))),
        },
        CommandSpec {
            name: "character_activity_recommender",
            args: vec![ArgSpec {
                name: "query",
                schema: schema(schema_for!(ActivityRecommenderQuery)),
            }],
            returns: Some(schema(schema_for!(ActivityRecommenderResult))),
        },
        CommandSpec {
            name: "settings_get",
            args: Vec::new(),
            returns: Some(schema(schema_for!(AppSettings))),
        },
        CommandSpec {
            name: "settings_overlay_position",
            args: Vec::new(),
            returns: Some(schema(schema_for!(OverlayPosition))),
        },
        CommandSpec {
            name: "settings_set_overlay_position",
            args: vec![
                ArgSpec {
                    name: "x",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "y",
                    schema: schema(schema_for!(i64)),
                },
            ],
            returns: None,
        },
        CommandSpec {
            name: "settings_update",
            args: vec![ArgSpec {
                name: "patch",
                schema: schema(schema_for!(SettingsPatch)),
            }],
            returns: Some(schema(schema_for!(AppSettings))),
        },
        CommandSpec {
            name: "codex_species",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<CodexSpecies>))),
        },
        CommandSpec {
            name: "codex_species_ranks",
            args: vec![ArgSpec {
                name: "species_name",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(CodexSpeciesRanks))),
        },
        CommandSpec {
            name: "codex_recommend",
            args: vec![
                ArgSpec {
                    name: "species_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "rank",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "professions",
                    schema: schema(schema_for!(Vec<String>)),
                },
                ArgSpec {
                    name: "target",
                    schema: schema(schema_for!(CodexRecommendTarget)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<CodexSkillOption>))),
        },
        CommandSpec {
            name: "codex_meta_attributes",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<CodexMetaAttribute>))),
        },
        CommandSpec {
            name: "codex_calibrate",
            args: vec![
                ArgSpec {
                    name: "species_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "rank",
                    schema: schema(schema_for!(i64)),
                },
            ],
            returns: Some(schema(schema_for!(CodexCalibrateResult))),
        },
        CommandSpec {
            name: "codex_claim",
            args: vec![
                ArgSpec {
                    name: "species_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "rank",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "skill_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(CodexClaimResult))),
        },
        CommandSpec {
            name: "codex_unclaim",
            args: vec![ArgSpec {
                name: "species_name",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(CodexClaimResult))),
        },
        CommandSpec {
            name: "codex_meta_claim",
            args: vec![ArgSpec {
                name: "attribute_name",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(CodexMetaClaimResult))),
        },
        CommandSpec {
            name: "codex_mastery_options",
            args: vec![
                ArgSpec {
                    name: "professions",
                    schema: schema(schema_for!(Vec<String>)),
                },
                ArgSpec {
                    name: "target",
                    schema: schema(schema_for!(CodexRecommendTarget)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<CodexSkillOption>))),
        },
        CommandSpec {
            name: "codex_mastery_claim",
            args: vec![
                ArgSpec {
                    name: "species_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "skill_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(CodexMasteryClaimResult))),
        },
        CommandSpec {
            name: "codex_mastery_unclaim",
            args: vec![ArgSpec {
                name: "species_name",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(CodexMasteryClaimResult))),
        },
        CommandSpec {
            name: "quests_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<Quest>))),
        },
        CommandSpec {
            name: "quest_get",
            args: vec![ArgSpec {
                name: "quest_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quest_create",
            args: vec![ArgSpec {
                name: "input",
                schema: schema(schema_for!(QuestInput)),
            }],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quest_update",
            args: vec![
                ArgSpec {
                    name: "quest_id",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "input",
                    schema: schema(schema_for!(QuestInput)),
                },
            ],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quest_delete",
            args: vec![ArgSpec {
                name: "quest_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "quest_start",
            args: vec![ArgSpec {
                name: "quest_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quest_complete",
            args: vec![ArgSpec {
                name: "quest_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quest_cancel",
            args: vec![
                ArgSpec {
                    name: "quest_id",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "undo_reward",
                    schema: schema(schema_for!(bool)),
                },
            ],
            returns: Some(schema(schema_for!(Quest))),
        },
        CommandSpec {
            name: "quests_mobs",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<String>))),
        },
        CommandSpec {
            name: "quests_analytics",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<QuestAnalyticsRow>))),
        },
        CommandSpec {
            name: "playlists_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<QuestPlaylist>))),
        },
        CommandSpec {
            name: "playlist_create",
            args: vec![ArgSpec {
                name: "input",
                schema: schema(schema_for!(PlaylistInput)),
            }],
            returns: Some(schema(schema_for!(QuestPlaylist))),
        },
        CommandSpec {
            name: "playlist_update",
            args: vec![
                ArgSpec {
                    name: "playlist_id",
                    schema: schema(schema_for!(i64)),
                },
                ArgSpec {
                    name: "input",
                    schema: schema(schema_for!(PlaylistInput)),
                },
            ],
            returns: Some(schema(schema_for!(QuestPlaylist))),
        },
        CommandSpec {
            name: "playlist_delete",
            args: vec![ArgSpec {
                name: "playlist_id",
                schema: schema(schema_for!(i64)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "playlists_analytics",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<PlaylistAnalyticsRow>))),
        },
        CommandSpec {
            name: "analytics_overview",
            args: vec![ArgSpec {
                name: "period",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(AnalyticsOverview))),
        },
        CommandSpec {
            name: "analytics_activity",
            args: Vec::new(),
            returns: Some(schema(schema_for!(AnalyticsActivity))),
        },
        CommandSpec {
            name: "ledger_list",
            args: vec![
                ArgSpec {
                    name: "cursor",
                    schema: schema(schema_for!(Option<String>)),
                },
                ArgSpec {
                    name: "limit",
                    schema: schema(schema_for!(Option<i64>)),
                },
            ],
            returns: Some(schema(schema_for!(LedgerPage))),
        },
        CommandSpec {
            name: "ledger_create",
            args: vec![ArgSpec {
                name: "entry",
                schema: schema(schema_for!(LedgerEntryInput)),
            }],
            returns: Some(schema(schema_for!(LedgerItem))),
        },
        CommandSpec {
            name: "ledger_delete",
            args: vec![ArgSpec {
                name: "entry_id",
                schema: schema(schema_for!(String)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "ledger_presets_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<LedgerPreset>))),
        },
        CommandSpec {
            name: "ledger_preset_create",
            args: vec![ArgSpec {
                name: "preset",
                schema: schema(schema_for!(LedgerPresetInput)),
            }],
            returns: Some(schema(schema_for!(LedgerPreset))),
        },
        CommandSpec {
            name: "ledger_preset_delete",
            args: vec![ArgSpec {
                name: "preset_id",
                schema: schema(schema_for!(String)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "inventory_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<InventoryItem>))),
        },
        CommandSpec {
            name: "inventory_create",
            args: vec![ArgSpec {
                name: "item",
                schema: schema(schema_for!(InventoryItemInput)),
            }],
            returns: Some(schema(schema_for!(InventoryItem))),
        },
        CommandSpec {
            name: "inventory_update",
            args: vec![
                ArgSpec {
                    name: "item_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "patch",
                    schema: schema(schema_for!(InventoryPatch)),
                },
            ],
            returns: Some(schema(schema_for!(InventoryItem))),
        },
        CommandSpec {
            name: "inventory_delete",
            args: vec![ArgSpec {
                name: "item_id",
                schema: schema(schema_for!(String)),
            }],
            returns: None,
        },
        CommandSpec {
            name: "inventory_sell",
            args: vec![
                ArgSpec {
                    name: "item_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "sale",
                    schema: schema(schema_for!(InventorySellInput)),
                },
            ],
            returns: Some(schema(schema_for!(InventorySellResult))),
        },
        CommandSpec {
            name: "market_paste_preview",
            args: vec![ArgSpec {
                name: "text",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(MarketPastePreview))),
        },
        CommandSpec {
            name: "market_paste_commit",
            args: vec![ArgSpec {
                name: "text",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(MarketCommitResult))),
        },
        CommandSpec {
            name: "market_overview",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<MarketOverviewRow>))),
        },
        CommandSpec {
            name: "market_contribution_batch",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Nullable<MarketContributionBatch>))),
        },
        CommandSpec {
            name: "market_break_even",
            args: Vec::new(),
            returns: Some(schema(schema_for!(MarketBreakEven))),
        },
        CommandSpec {
            name: "market_mob_ranking",
            args: vec![ArgSpec {
                name: "horizon",
                schema: schema(schema_for!(MarketHorizon)),
            }],
            returns: Some(schema(schema_for!(Vec<MarketMobRankingRow>))),
        },
        CommandSpec {
            name: "market_item_history",
            args: vec![
                ArgSpec {
                    name: "item_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "horizon",
                    schema: schema(schema_for!(MarketHorizon)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<MarketHistoryPoint>))),
        },
        CommandSpec {
            name: "scan_status",
            args: Vec::new(),
            returns: Some(schema(schema_for!(ScanStatus))),
        },
        CommandSpec {
            name: "scan_start",
            args: vec![ArgSpec {
                name: "page_count",
                schema: schema(schema_for!(Option<i64>)),
            }],
            returns: Some(schema(schema_for!(ScanStatus))),
        },
        CommandSpec {
            name: "scan_capture",
            args: Vec::new(),
            returns: Some(schema(schema_for!(CaptureResult))),
        },
        CommandSpec {
            name: "scan_cancel",
            args: Vec::new(),
            returns: Some(schema(schema_for!(ScanStatus))),
        },
        CommandSpec {
            name: "scan_undo",
            args: Vec::new(),
            returns: Some(schema(schema_for!(UndoResult))),
        },
        CommandSpec {
            name: "scan_process",
            args: Vec::new(),
            returns: Some(schema(schema_for!(ScanStatus))),
        },
        CommandSpec {
            name: "scan_accept",
            args: Vec::new(),
            returns: Some(schema(schema_for!(AcceptResult))),
        },
        CommandSpec {
            name: "scan_reject",
            args: Vec::new(),
            returns: Some(schema(schema_for!(RejectResult))),
        },
        CommandSpec {
            name: "scan_pending",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Option<SkillScanPending>))),
        },
        CommandSpec {
            name: "scan_spacebar_capture",
            args: vec![ArgSpec {
                name: "enabled",
                schema: schema(schema_for!(bool)),
            }],
            returns: Some(schema(schema_for!(SpacebarResult))),
        },
        CommandSpec {
            name: "tracking_sessions",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<TrackingSession>))),
        },
        CommandSpec {
            name: "tracking_session_detail",
            args: vec![ArgSpec {
                name: "session_id",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(SessionDetail))),
        },
        CommandSpec {
            name: "tracking_tag_suggestions",
            args: vec![
                ArgSpec {
                    name: "q",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "limit",
                    schema: schema(schema_for!(Option<i64>)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<String>))),
        },
        CommandSpec {
            name: "tracking_manual_mob_suggestions",
            args: vec![
                ArgSpec {
                    name: "q",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "limit",
                    schema: schema(schema_for!(Option<i64>)),
                },
            ],
            returns: Some(schema(schema_for!(Vec<ManualMobSuggestion>))),
        },
        CommandSpec {
            name: "tracking_snapshot",
            args: Vec::new(),
            returns: Some(schema(schema_for!(TrackingSnapshot))),
        },
        CommandSpec {
            name: "tracking_quest_link_suggestion",
            args: vec![ArgSpec {
                name: "session_id",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(SessionQuestLinkSuggestion))),
        },
        CommandSpec {
            name: "tracking_start",
            args: Vec::new(),
            returns: Some(schema(schema_for!(StartResult))),
        },
        CommandSpec {
            name: "tracking_stop",
            args: Vec::new(),
            returns: Some(schema(schema_for!(StopResult))),
        },
        CommandSpec {
            name: "tracking_release_mob",
            args: Vec::new(),
            returns: Some(schema(schema_for!(ReleaseResult))),
        },
        CommandSpec {
            name: "tracking_manual_mob_lock",
            args: vec![
                ArgSpec {
                    name: "species",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "maturity",
                    schema: schema(schema_for!(Option<String>)),
                },
            ],
            returns: Some(schema(schema_for!(ManualMobLockResult))),
        },
        CommandSpec {
            name: "tracking_tag_lock",
            args: vec![ArgSpec {
                name: "tag",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(TagLockResult))),
        },
        CommandSpec {
            name: "tracking_rename_mob",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "from_mob_name",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "to_mob_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(MobEditResult))),
        },
        CommandSpec {
            name: "tracking_restore_mob",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "current_mob_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(MobEditResult))),
        },
        CommandSpec {
            name: "tracking_loot_item_activate",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "item_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(LootItemEditResult))),
        },
        CommandSpec {
            name: "tracking_loot_item_deactivate",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "item_name",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(LootItemEditResult))),
        },
        CommandSpec {
            name: "tracking_armour_cost",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "cost",
                    schema: schema(schema_for!(f64)),
                },
            ],
            returns: Some(schema(schema_for!(ArmourCostResult))),
        },
        CommandSpec {
            name: "tracking_quest_link",
            args: vec![
                ArgSpec {
                    name: "session_id",
                    schema: schema(schema_for!(String)),
                },
                ArgSpec {
                    name: "action",
                    schema: schema(schema_for!(String)),
                },
            ],
            returns: Some(schema(schema_for!(QuestLinkDecision))),
        },
        CommandSpec {
            name: "tracking_repair_scan",
            args: vec![ArgSpec {
                name: "session_id",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(RepairScanResult))),
        },
        CommandSpec {
            name: "tracking_session_delete",
            args: vec![ArgSpec {
                name: "session_id",
                schema: schema(schema_for!(String)),
            }],
            returns: None,
        },
        // The guide-mode demo reads: typed commands sharing the live
        // analytics and tracking DTOs, served over the parallel demo state.
        CommandSpec {
            name: "demo_analytics_overview",
            args: vec![ArgSpec {
                name: "period",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(AnalyticsOverview))),
        },
        CommandSpec {
            name: "demo_analytics_activity",
            args: Vec::new(),
            returns: Some(schema(schema_for!(AnalyticsActivity))),
        },
        CommandSpec {
            name: "demo_ledger_list",
            args: vec![
                ArgSpec {
                    name: "cursor",
                    schema: schema(schema_for!(Option<String>)),
                },
                ArgSpec {
                    name: "limit",
                    schema: schema(schema_for!(Option<i64>)),
                },
            ],
            returns: Some(schema(schema_for!(LedgerPage))),
        },
        CommandSpec {
            name: "demo_ledger_presets_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<LedgerPreset>))),
        },
        CommandSpec {
            name: "demo_inventory_list",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<InventoryItem>))),
        },
        CommandSpec {
            name: "demo_tracking_sessions",
            args: Vec::new(),
            returns: Some(schema(schema_for!(Vec<TrackingSession>))),
        },
        CommandSpec {
            name: "demo_tracking_session_detail",
            args: vec![ArgSpec {
                name: "session_id",
                schema: schema(schema_for!(String)),
            }],
            returns: Some(schema(schema_for!(SessionDetail))),
        },
        CommandSpec {
            name: "demo_tracking_snapshot",
            args: Vec::new(),
            returns: Some(schema(schema_for!(TrackingSnapshot))),
        },
        // The hidden developer-tools family: native-only, each gated on
        // developer mode (a gate-off command answers the not-found the HTTP
        // route's 404 stood for).
        CommandSpec {
            name: "dev_metrics",
            args: Vec::new(),
            returns: Some(schema(schema_for!(MetricsSnapshot))),
        },
        CommandSpec {
            name: "dev_crash_reporting",
            args: Vec::new(),
            returns: Some(schema(schema_for!(CrashReportingStatus))),
        },
        CommandSpec {
            name: "dev_set_crash_reporting",
            args: vec![ArgSpec {
                name: "enabled",
                schema: schema(schema_for!(bool)),
            }],
            returns: Some(schema(schema_for!(CrashReportingStatus))),
        },
        CommandSpec {
            name: "dev_compact_database",
            args: Vec::new(),
            returns: Some(schema(schema_for!(CompactResult))),
        },
        CommandSpec {
            name: "dev_rebuild_projections",
            args: Vec::new(),
            returns: Some(schema(schema_for!(RebuildReport))),
        },
    ]
}

/// The IPC error contract's schema, emitted alongside the commands.
pub fn error_schema() -> Value {
    schema(schema_for!(ApiError))
}

/// A derived schema as its plain JSON value.
fn schema(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).expect("a derived schema serialises")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_unique_and_snake_case() {
        let specs = manifest();
        let mut names: Vec<&str> = specs.iter().map(|spec| spec.name).collect();
        names.sort_unstable();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "duplicate command name");
        for name in names {
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name} is not snake_case"
            );
        }
    }
}
