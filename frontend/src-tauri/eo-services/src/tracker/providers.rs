//! The provider callbacks the composition root wires in.

use std::sync::Arc;

use serde_json::{Map, Value};

/// An equipment profile from the library lookup, when the tool is
/// known.
pub type EquipmentProfile = Option<Map<String, Value>>;

/// The provider callbacks the composition root wires in; every field
/// defaults to the original's inert fallback. The lookups may read
/// the database (the lock order allows a provider read under the
/// tracker lock); the resolver is invoked outside the lock.
pub struct Providers {
    pub equipment_cost_lookup: Arc<dyn Fn(&str) -> f64 + Send + Sync>,
    pub equipment_profile_lookup: Arc<dyn Fn(&str) -> EquipmentProfile + Send + Sync>,
    pub player_name: String,
    pub loot_filter_blacklist: Vec<String>,
    pub loot_filter_blacklist_provider: Option<Arc<dyn Fn() -> Vec<String> + Send + Sync>>,
    pub weapon_attribution_trifecta: Arc<dyn Fn() -> bool + Send + Sync>,
    pub mob_tracking_mode: Arc<dyn Fn() -> String + Send + Sync>,
    pub mob_tracking_tag: Arc<dyn Fn() -> String + Send + Sync>,
    pub manual_mob_entry_enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    pub manual_mob: Arc<dyn Fn() -> Option<(String, String)> + Send + Sync>,
    pub trifecta_resolver: Arc<dyn Fn() -> Option<Map<String, Value>> + Send + Sync>,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            equipment_cost_lookup: Arc::new(|_| 0.0),
            equipment_profile_lookup: Arc::new(|_| None),
            player_name: String::new(),
            loot_filter_blacklist: Vec::new(),
            loot_filter_blacklist_provider: None,
            weapon_attribution_trifecta: Arc::new(|| false),
            mob_tracking_mode: Arc::new(|| "mob".to_string()),
            mob_tracking_tag: Arc::new(String::new),
            manual_mob_entry_enabled: Arc::new(|| true),
            manual_mob: Arc::new(|| None),
            trifecta_resolver: Arc::new(|| None),
        }
    }
}
