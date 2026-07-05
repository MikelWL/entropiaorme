//! The tracker's dependency seams: the equipment library and the
//! session-capture configuration, as named traits the composition
//! root implements once. Implementations may read the database or
//! config; calls run inline on the tracker's task.

use std::sync::Arc;

use serde_json::{Map, Value};

/// An equipment profile from the library lookup, when the tool is
/// known.
pub type EquipmentProfile = Option<Map<String, Value>>;

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
}

/// The session-capture configuration seam: the live settings the
/// tracker consults at session start, on reload, and per event.
pub trait TrackingConfig: Send + Sync {
    /// The configured input mode string (parsed to a mode at the
    /// session-capture boundary).
    fn mob_tracking_mode(&self) -> String;

    /// The configured tag-mode free-text tag.
    fn mob_tracking_tag(&self) -> String;

    /// Whether manual mob selection is enabled (mob mode).
    fn manual_mob_entry_enabled(&self) -> bool;

    /// The manually configured (species, maturity), when set.
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
}

/// The original's inert configuration fallbacks: mob mode, no tag,
/// manual entry enabled, hotbar attribution, empty blacklist.
pub struct DefaultTrackingConfig;

impl TrackingConfig for DefaultTrackingConfig {
    fn mob_tracking_mode(&self) -> String {
        "mob".to_string()
    }

    fn mob_tracking_tag(&self) -> String {
        String::new()
    }

    fn manual_mob_entry_enabled(&self) -> bool {
        true
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
