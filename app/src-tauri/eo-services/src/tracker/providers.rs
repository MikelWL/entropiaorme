//! The tracker's dependency seams: the equipment library and the
//! session-capture configuration, as named traits the composition
//! root implements once. Implementations may read the database or
//! config; calls run inline on the tracker's task.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::harvest_yield::HarvestYieldTier;

/// An equipment profile from the library lookup, when the tool is
/// known.
pub type EquipmentProfile = Option<Map<String, Value>>;

/// The guardrail's alias for the shared board-yield vocabulary.
///
/// The name dates from when the guardrail was configured per physical tree
/// size. It now carries the yield tier evidenced by a swing's board output,
/// which is what the guardrail actually matches on. Renaming it reaches the
/// wire contract (`TreeSizeName`) and its generated bindings, so it is a
/// deliberate change rather than a drive-by.
pub type TreeSize = HarvestYieldTier;

/// One intended harvesting tool, resolved from the equipment library.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardrailTool {
    pub name: String,
    pub cost_per_use_ped: f64,
}

/// The resolved harvest guardrail: the intended tool per board class.
/// A class with no configured tool carries None and stays outside the
/// guardrail's reach.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HarvestGuardrailTools {
    pub short: Option<GuardrailTool>,
    pub long: Option<GuardrailTool>,
    pub huge: Option<GuardrailTool>,
}

impl HarvestGuardrailTools {
    /// The intended tool for a board-output class, when configured.
    pub fn for_size(&self, size: TreeSize) -> Option<&GuardrailTool> {
        match size {
            TreeSize::Short => self.short.as_ref(),
            TreeSize::Long => self.long.as_ref(),
            TreeSize::Huge => self.huge.as_ref(),
            TreeSize::Unknown => None,
        }
    }
}

/// The equipment-library seam: profile and cost lookups plus the
/// trifecta-preset resolution the attribution mode needs.
pub trait EquipmentLibrary: Send + Sync {
    /// The weapon profile whose name matches the tool fragment, when
    /// the library knows it.
    fn weapon_profile(&self, tool_name: &str) -> EquipmentProfile;

    /// The per-shot cost in PED, `0.0` when the tool is unknown.
    fn cost_per_shot(&self, tool_name: &str) -> f64;

    /// Resolve the active trifecta preset's attribution map (weapons,
    /// damage bands, heal tool), when one is configured and complete.
    fn resolve_trifecta(&self) -> Option<Map<String, Value>>;

    /// Resolve the harvest guardrail's intended tools, when the
    /// guardrail is enabled and at least one board class names a tool the
    /// library knows.
    fn resolve_harvest_guardrail(&self) -> Option<HarvestGuardrailTools>;
}

/// The session-capture configuration seam: the live settings the
/// tracker consults at session start, on reload, and per event.
pub trait TrackingConfig: Send + Sync {
    /// The configured session name (the designated facet a session
    /// snapshots at start; empty is "not declared").
    fn session_name(&self) -> String;

    /// The configured skill-boost percentage (zero or negative is "no
    /// boost").
    fn skill_boost_percent(&self) -> i64;

    /// The declared (species, maturity), when one is configured.
    fn manual_mob(&self) -> Option<(String, String)>;

    /// Whether weapon attribution runs in trifecta mode (vs hotbar).
    fn weapon_attribution_trifecta(&self) -> bool;

    /// The loot-filter blacklist.
    fn loot_filter_blacklist(&self) -> Vec<String>;
}

/// The tracker's wired dependencies. Defaults are the inert fallbacks
/// the original shipped (no equipment, mob mode, manual entry on).
pub struct Providers {
    pub equipment: Arc<dyn EquipmentLibrary>,
    pub config: Arc<dyn TrackingConfig>,
    /// The player's name for global/HoF correlation, fixed at
    /// construction (whitespace-trimmed there).
    pub player_name: String,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            equipment: Arc::new(InertEquipment),
            config: Arc::new(DefaultTrackingConfig),
            player_name: String::new(),
        }
    }
}

/// The inert equipment library: no profiles, no costs, no trifecta.
pub struct InertEquipment;

impl EquipmentLibrary for InertEquipment {
    fn weapon_profile(&self, _tool_name: &str) -> EquipmentProfile {
        None
    }

    fn cost_per_shot(&self, _tool_name: &str) -> f64 {
        0.0
    }

    fn resolve_trifecta(&self) -> Option<Map<String, Value>> {
        None
    }

    fn resolve_harvest_guardrail(&self) -> Option<HarvestGuardrailTools> {
        None
    }
}

/// The inert configuration fallbacks: no declared facets, manual mob
/// declaration enabled, hotbar attribution, empty blacklist.
pub struct DefaultTrackingConfig;

impl TrackingConfig for DefaultTrackingConfig {
    fn session_name(&self) -> String {
        String::new()
    }

    fn skill_boost_percent(&self) -> i64 {
        0
    }

    fn manual_mob(&self) -> Option<(String, String)> {
        None
    }

    fn weapon_attribution_trifecta(&self) -> bool {
        false
    }

    fn loot_filter_blacklist(&self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_equipment_offers_nothing() {
        let equipment = InertEquipment;
        assert_eq!(equipment.weapon_profile("Opalo"), None);
        assert_eq!(equipment.cost_per_shot("Opalo"), 0.0);
        assert_eq!(equipment.resolve_trifecta(), None);
        assert_eq!(equipment.resolve_harvest_guardrail(), None);
    }

    #[test]
    fn default_tracking_config_is_the_inert_fallback() {
        let config = DefaultTrackingConfig;
        assert_eq!(config.session_name(), "");
        assert_eq!(config.skill_boost_percent(), 0);
        assert_eq!(config.manual_mob(), None);
        assert!(!config.weapon_attribution_trifecta());
        assert!(config.loot_filter_blacklist().is_empty());
    }
}
