//! Cost formula engine.
//!
//! Per-use cost (decay + ammo + markups) and reference damage / heal ranges
//! from equipment-catalogue payloads, at maxed skill. Pure arithmetic, no
//! clock, no DB, no events: the canonical leaf and the runner's per-unit
//! `cargo test` proving target. The engine operates on `serde_json::Value`
//! equipment dicts (the stored catalogue payload shape) and rounds every
//! intermediate figure through the shared `round_half_even` banker's
//! rounding, keeping the figures bit-identical to the frozen goldens that
//! pin them. These figures carry no own fingerprint golden of their own; their
//! byte-equality is asserted where they fold into a downstream service's
//! tracker fingerprint, so it is proven there rather than here.

use eo_wire::normalizer::round_half_even;
use serde_json::{json, Value};

const DAMAGE_TYPES: [&str; 9] = [
    "impact",
    "cut",
    "stab",
    "penetration",
    "shrapnel",
    "burn",
    "cold",
    "acid",
    "electric",
];

fn round4(x: f64) -> f64 {
    round_half_even(x, 4)
}

/// Python truthiness for the equipment-dict checks (`if absorber:` / `if
/// scope:`): null/false/0/empty-string/empty-collection are falsy.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `entity.get("economy") or {}`: the economy subdict, defaulting to empty.
fn economy(entity: &Value) -> Value {
    match entity.get("economy") {
        Some(eco) if is_truthy(eco) => eco.clone(),
        _ => json!({}),
    }
}

/// `value.get(key) or 0.0` over a numeric field: the number if present and
/// truthy, else 0.0. (Every default in the engine is 0.0, so a stored 0
/// collapses to the same value either way.)
fn num_or_zero(value: &Value, key: &str) -> f64 {
    match value.get(key).and_then(Value::as_f64) {
        Some(n) if n != 0.0 => n,
        _ => 0.0,
    }
}

/// True if the entity name contains "(L)", indicating a limited item.
pub fn is_limited(entity: &Value) -> bool {
    entity
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .contains("(L)")
}

/// Sum the per-type damage fields published on the entity; `None` if the
/// entity is absent or the total is zero (`total or None`).
fn sum_damage(entity: Option<&Value>) -> Option<f64> {
    let entity = entity?;
    let damage = match entity.get("damage") {
        Some(d) if is_truthy(d) => d.clone(),
        _ => json!({}),
    };
    let total: f64 = DAMAGE_TYPES.iter().map(|t| num_or_zero(&damage, t)).sum();
    if total == 0.0 {
        None
    } else {
        Some(total)
    }
}

/// Total weapon damage from base + amp + damage enhancers.
pub fn weapon_total_damage(
    weapon: &Value,
    amp: Option<&Value>,
    damage_enhancers: i64,
) -> Option<f64> {
    let base_damage = sum_damage(Some(weapon))?;
    let mut total_damage = base_damage * (1.0 + damage_enhancers as f64 * 0.1);
    if let Some(amp_damage) = sum_damage(amp) {
        total_damage += (base_damage / 2.0).min(amp_damage);
    }
    Some(total_damage)
}

/// Damage range at maxed skill: `[0.5 * total, total]`.
pub fn damage_range_at_max_skill(total_damage: f64) -> Value {
    json!({"min": total_damage * 0.5, "max": total_damage})
}

/// Derived damage profile suitable for tool inference / display.
pub fn get_weapon_damage_profile(
    weapon: &Value,
    amp: Option<&Value>,
    damage_enhancers: i64,
) -> Option<Value> {
    let total_damage = weapon_total_damage(weapon, amp, damage_enhancers)?;
    Some(json!({
        "totalDamage": total_damage,
        "damageMin": total_damage * 0.5,
        "damageMax": total_damage,
    }))
}

/// Heal range at maxed skill: the tool's published `min_heal` / `max_heal`.
pub fn heal_range_at_max_skill(tool: &Value) -> Option<Value> {
    let max_heal = tool.get("max_heal").filter(|v| !v.is_null())?;
    let min_heal = tool.get("min_heal").filter(|v| !v.is_null())?;
    Some(json!({"min": min_heal, "max": max_heal}))
}

/// Reload at maxed skill: mindforce cooldown if present, else `60 / uses_per_minute`.
pub fn heal_reload_seconds(tool: &Value) -> f64 {
    let cooldown = tool
        .get("mindforce")
        .filter(|v| is_truthy(v))
        .and_then(|m| m.get("cooldown"))
        .and_then(Value::as_f64);
    if let Some(c) = cooldown {
        if c != 0.0 {
            return c;
        }
    }
    let uses_per_minute = tool.get("uses_per_minute").and_then(Value::as_f64);
    match uses_per_minute {
        Some(u) if u != 0.0 => 60.0 / u,
        _ => 60.0 / 24.0,
    }
}

/// Manually configured decay-split devices on a stored entry: a Mindforce
/// implant routing a fraction of the tool's decay to itself, and an
/// extender routing a fraction of the post-implant remainder. Measured
/// in-game behaviour: the catalogue decay is conserved and redistributed
/// (a plain implant takes ~2%, an absorbing implant its stated share; the
/// extender's share applies to what the tool would otherwise keep), so the
/// splits compose multiplicatively, implant first. Shares are fractions
/// (0..=1); markups are multipliers (1.0 = 100%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecaySplits {
    pub implant_share: f64,
    pub implant_markup: f64,
    pub extender_share: f64,
    pub extender_markup: f64,
}

impl Default for DecaySplits {
    fn default() -> Self {
        DecaySplits {
            implant_share: 0.0,
            implant_markup: 1.0,
            extender_share: 0.0,
            extender_markup: 1.0,
        }
    }
}

impl DecaySplits {
    /// Read the stored `implant` / `extender` objects from a
    /// `properties_json` payload. Percent fields convert to fractions and
    /// clamp to `[0, 1]`; an absent or falsy device contributes no split.
    pub fn from_props(props: &Value) -> Self {
        let device = |key: &str, share_key: &str| -> (f64, f64) {
            match props.get(key).filter(|v| is_truthy(v)) {
                Some(device) => {
                    let share = (num_or_zero(device, share_key) / 100.0).clamp(0.0, 1.0);
                    let markup = match device.get("markup_percent").and_then(Value::as_f64) {
                        Some(m) if m != 0.0 => m / 100.0,
                        _ => 1.0,
                    };
                    (share, markup)
                }
                None => (0.0, 1.0),
            }
        };
        let (implant_share, implant_markup) = device("implant", "decay_share_percent");
        let (extender_share, extender_markup) = device("extender", "absorption_percent");
        DecaySplits {
            implant_share,
            implant_markup,
            extender_share,
            extender_markup,
        }
    }
}

/// Calculate the cost breakdown for a weapon configuration, returning
/// `{"costBreakdown": [...], "totalCostPerUse": float}`.
#[allow(clippy::too_many_arguments)]
pub fn cost_per_shot(
    weapon: &Value,
    amp: Option<&Value>,
    scope: Option<&Value>,
    absorber: Option<&Value>,
    damage_enhancers: i64,
    weapon_markup: f64,
    amp_markup: f64,
    scope_markup: f64,
    absorber_markup: f64,
) -> Value {
    cost_per_shot_with_splits(
        weapon,
        amp,
        scope,
        absorber,
        damage_enhancers,
        weapon_markup,
        amp_markup,
        scope_markup,
        absorber_markup,
        &DecaySplits::default(),
    )
}

/// [`cost_per_shot`] with manual decay splits: the implant's share leaves
/// the weapon's decay first, the extender's share leaves the remainder,
/// and each split becomes its own priced line at that device's markup.
/// With zero shares the figures are bit-identical to [`cost_per_shot`].
#[allow(clippy::too_many_arguments)]
pub fn cost_per_shot_with_splits(
    weapon: &Value,
    amp: Option<&Value>,
    scope: Option<&Value>,
    absorber: Option<&Value>,
    damage_enhancers: i64,
    weapon_markup: f64,
    amp_markup: f64,
    scope_markup: f64,
    absorber_markup: f64,
    splits: &DecaySplits,
) -> Value {
    let eco = economy(weapon);
    let base_decay = num_or_zero(&eco, "decay");
    let base_ammo_pec = num_or_zero(&eco, "ammo_burn") / 100.0;

    let enhancer_mult = 1.0 + damage_enhancers as f64 * 0.1;
    let mut weapon_decay = base_decay * enhancer_mult;
    let weapon_ammo = base_ammo_pec * enhancer_mult;

    let implant_decay = weapon_decay * splits.implant_share;
    weapon_decay -= implant_decay;
    let extender_decay = weapon_decay * splits.extender_share;
    weapon_decay -= extender_decay;

    // `if absorber:` is a truthiness check (empty dict is falsy).
    let absorber_truthy = absorber.map(is_truthy).unwrap_or(false);
    let mut absorber_decay = 0.0;
    if absorber_truthy {
        let absorption = num_or_zero(&economy(absorber.unwrap()), "absorption");
        absorber_decay = weapon_decay * absorption;
        weapon_decay -= absorber_decay;
    }

    // `if amp is not None:` is an explicit None check (empty dict still runs).
    let mut amp_decay = 0.0;
    let mut amp_ammo = 0.0;
    if let Some(amp_value) = amp {
        let amp_eco = economy(amp_value);
        amp_decay = num_or_zero(&amp_eco, "decay");
        amp_ammo = num_or_zero(&amp_eco, "ammo_burn") / 100.0;
    }

    let mut breakdown: Vec<Value> = Vec::new();
    let mut total = 0.0;
    let mut add_line = |component: &str, cost_pec: f64, markup: f64| {
        let effective = round4(cost_pec * markup);
        breakdown.push(json!({
            "component": component,
            "costPec": round4(cost_pec),
            "markupMultiplier": round4(markup),
            "effectiveCostPec": effective,
        }));
        total += effective;
    };

    if implant_decay > 0.0 {
        add_line("Implant decay", implant_decay, splits.implant_markup);
    }
    if extender_decay > 0.0 {
        add_line("Extender decay", extender_decay, splits.extender_markup);
    }
    if absorber_truthy && absorber_decay > 0.0 {
        add_line("Absorber decay", absorber_decay, absorber_markup);
    }
    add_line("Weapon decay", weapon_decay, weapon_markup);
    if amp.is_some() {
        add_line("Amp decay", amp_decay, amp_markup);
    }
    if scope.map(is_truthy).unwrap_or(false) {
        let scope_decay = num_or_zero(&economy(scope.unwrap()), "decay");
        add_line("Scope decay", scope_decay, scope_markup);
    }
    if weapon_ammo > 0.0 {
        let label = if amp.is_some() {
            "Ammo (weapon)"
        } else {
            "Ammo"
        };
        add_line(label, weapon_ammo, 1.0);
    }
    if amp.is_some() && amp_ammo > 0.0 {
        add_line("Ammo (amp)", amp_ammo, 1.0);
    }

    json!({
        "costBreakdown": breakdown,
        "totalCostPerUse": round4(total),
    })
}

/// Calculate weapon cost from an `equipment_library` `properties_json` payload.
pub fn cost_per_shot_from_props(props: &Value, damage_enhancers: Option<i64>) -> Value {
    let configured: f64 = match damage_enhancers {
        Some(de) => de as f64,
        None => props
            .get("damage_enhancers")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    };
    // `max(0, int(configured or 0))`.
    let enhancers = (configured as i64).max(0);

    let opt = |key: &str| props.get(key).filter(|v| !v.is_null());
    let markup = |key: &str| props.get(key).and_then(Value::as_f64).unwrap_or(100.0) / 100.0;

    // `weapon_entity` is mandatory, mirroring the Python `props["weapon_entity"]`
    // (which raises on a missing key, and on a null value when `_economy` calls
    // `.get`). Fail fast rather than defaulting a missing/null weapon to an empty
    // economy, which would silently diverge from the oracle.
    let weapon = props
        .get("weapon_entity")
        .filter(|v| !v.is_null())
        .expect("cost_per_shot_from_props requires a non-null weapon_entity");

    cost_per_shot_with_splits(
        weapon,
        opt("amp_entity"),
        opt("scope_entity"),
        opt("absorber_entity"),
        enhancers,
        markup("weapon_markup"),
        markup("amp_markup"),
        markup("scope_markup"),
        markup("absorber_markup"),
        &DecaySplits::from_props(props),
    )
}

/// Cost per use for a medical tool: `(decay + ammo) * markup` in PEC, rounded.
pub fn heal_cost_per_use(tool: &Value, markup: f64) -> f64 {
    heal_cost_per_use_with_splits(tool, markup, &DecaySplits::default())
}

/// [`heal_cost_per_use`] with manual decay splits: the implant's share of
/// the tool's decay and the extender's share of the remainder are priced
/// at their own markups; the tool keeps the rest (and its ammo) at the
/// tool's markup. With zero shares the figure is bit-identical to
/// [`heal_cost_per_use`].
pub fn heal_cost_per_use_with_splits(tool: &Value, markup: f64, splits: &DecaySplits) -> f64 {
    let eco = economy(tool);
    let decay = num_or_zero(&eco, "decay");
    let ammo_pec = num_or_zero(&eco, "ammo_burn") / 100.0;
    let implant_decay = decay * splits.implant_share;
    let remainder = decay - implant_decay;
    let extender_decay = remainder * splits.extender_share;
    let tool_decay = remainder - extender_decay;
    round4(
        implant_decay * splits.implant_markup
            + extender_decay * splits.extender_markup
            + (tool_decay + ammo_pec) * markup,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values below are the Python cost_engine's output for the same
    // payloads (the ported numeric expectations).

    #[test]
    fn weapon_only_tt_cost() {
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 200}});
        let result = cost_per_shot(&weapon, None, None, None, 0, 1.0, 1.0, 1.0, 1.0);
        // decay 0.05 @ 1.0 + ammo 2.0 @ 1.0 = 2.05
        assert_eq!(result["totalCostPerUse"], json!(2.05));
        assert_eq!(
            result["costBreakdown"][0]["component"],
            json!("Weapon decay")
        );
        assert_eq!(result["costBreakdown"][0]["effectiveCostPec"], json!(0.05));
        assert_eq!(result["costBreakdown"][1]["component"], json!("Ammo"));
        assert_eq!(result["costBreakdown"][1]["effectiveCostPec"], json!(2.0));
    }

    #[test]
    fn weapon_with_markup_rounds_each_line() {
        let weapon = json!({"economy": {"decay": 0.123456, "ammo_burn": 0}});
        let result = cost_per_shot(&weapon, None, None, None, 0, 1.15, 1.0, 1.0, 1.0);
        // costPec round(0.123456,4)=0.1235; effective round(0.123456*1.15,4)=0.142
        assert_eq!(result["costBreakdown"][0]["costPec"], json!(0.1235));
        assert_eq!(result["costBreakdown"][0]["markupMultiplier"], json!(1.15));
        assert_eq!(result["costBreakdown"][0]["effectiveCostPec"], json!(0.142));
        assert_eq!(result["totalCostPerUse"], json!(0.142));
    }

    #[test]
    fn amp_present_relabels_ammo_and_adds_amp_lines() {
        let weapon = json!({"economy": {"decay": 0.1, "ammo_burn": 100}});
        let amp = json!({"economy": {"decay": 0.02, "ammo_burn": 50}});
        let result = cost_per_shot(&weapon, Some(&amp), None, None, 0, 1.0, 1.0, 1.0, 1.0);
        let components: Vec<&str> = result["costBreakdown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["component"].as_str().unwrap())
            .collect();
        assert_eq!(
            components,
            vec!["Weapon decay", "Amp decay", "Ammo (weapon)", "Ammo (amp)"]
        );
        // 0.1 + 0.02 + 1.0 + 0.5 = 1.62
        assert_eq!(result["totalCostPerUse"], json!(1.62));
    }

    #[test]
    fn absorber_splits_weapon_decay() {
        let weapon = json!({"economy": {"decay": 0.1, "ammo_burn": 0}});
        let absorber = json!({"economy": {"absorption": 0.3}});
        let result = cost_per_shot(&weapon, None, None, Some(&absorber), 0, 1.0, 1.0, 1.0, 1.2);
        let components: Vec<&str> = result["costBreakdown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["component"].as_str().unwrap())
            .collect();
        assert_eq!(components, vec!["Absorber decay", "Weapon decay"]);
        // absorber_decay 0.03 @ 1.2 = 0.036 ; remaining weapon 0.07 @ 1.0 = 0.07
        assert_eq!(result["costBreakdown"][0]["effectiveCostPec"], json!(0.036));
        assert_eq!(result["costBreakdown"][1]["effectiveCostPec"], json!(0.07));
        assert_eq!(result["totalCostPerUse"], json!(0.106));
    }

    #[test]
    fn empty_absorber_dict_is_falsy_no_split() {
        let weapon = json!({"economy": {"decay": 0.1, "ammo_burn": 0}});
        let absorber = json!({}); // empty dict is falsy in Python
        let result = cost_per_shot(&weapon, None, None, Some(&absorber), 0, 1.0, 1.0, 1.0, 1.0);
        assert_eq!(result["costBreakdown"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["costBreakdown"][0]["component"],
            json!("Weapon decay")
        );
    }

    #[test]
    fn from_props_applies_markups_and_enhancer_clamp() {
        let props = json!({
            "weapon_entity": {"economy": {"decay": 0.1, "ammo_burn": 100}},
            "weapon_markup": 120,
            "damage_enhancers": -3,
        });
        let result = cost_per_shot_from_props(&props, None);
        // enhancers clamp to 0; decay 0.1 @ 1.2 = 0.12 ; ammo 1.0 @ 1.0 = 1.0
        assert_eq!(result["totalCostPerUse"], json!(1.12));
    }

    #[test]
    #[should_panic(expected = "weapon_entity")]
    fn from_props_requires_a_weapon_entity() {
        // The Python oracle raises on a missing weapon_entity; the port mirrors
        // that fail-fast rather than defaulting to a zero-cost empty economy.
        cost_per_shot_from_props(&json!({"weapon_markup": 100}), None);
    }

    #[test]
    fn heal_cost_rounds_to_four_places() {
        let tool = json!({"economy": {"decay": 0.0512, "ammo_burn": 30}});
        // (0.0512 + 0.3) * 1.0 = 0.3512
        assert_eq!(heal_cost_per_use(&tool, 1.0), 0.3512);
    }

    #[test]
    fn damage_enhancers_add_ten_percent_each() {
        let weapon = json!({"damage": {"impact": 50.0}});
        assert_eq!(weapon_total_damage(&weapon, None, 0), Some(50.0));
        assert_eq!(weapon_total_damage(&weapon, None, 2), Some(60.0));
        // No damage -> None.
        assert_eq!(weapon_total_damage(&json!({}), None, 0), None);
    }

    #[test]
    fn heal_reload_prefers_mindforce_cooldown() {
        assert_eq!(
            heal_reload_seconds(&json!({"mindforce": {"cooldown": 2.5}, "uses_per_minute": 30})),
            2.5
        );
        assert_eq!(heal_reload_seconds(&json!({"uses_per_minute": 30})), 2.0);
        assert_eq!(heal_reload_seconds(&json!({})), 60.0 / 24.0);
    }

    #[test]
    fn limited_items_are_detected_by_name_marker() {
        assert!(is_limited(&json!({"name": "Breaker (L)"})));
        assert!(!is_limited(&json!({"name": "Breaker"})));
        assert!(!is_limited(&json!({})));
    }

    #[test]
    fn damage_range_halves_the_total() {
        assert_eq!(
            damage_range_at_max_skill(10.0),
            json!({"min": 5.0, "max": 10.0})
        );
    }

    #[test]
    fn amp_damage_adds_capped_at_half_the_base() {
        let weapon = json!({"damage": {"impact": 10.0}});
        // Below the cap: 10 + 3 = 13.
        assert_eq!(
            weapon_total_damage(&weapon, Some(&json!({"damage": {"burn": 3.0}})), 0),
            Some(13.0)
        );
        // Above the cap: 10 + min(5, 8) = 15.
        assert_eq!(
            weapon_total_damage(&weapon, Some(&json!({"damage": {"burn": 8.0}})), 0),
            Some(15.0)
        );
    }

    #[test]
    fn enhancers_scale_decay_and_ammo_with_hand_computed_totals() {
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 200}});
        let result = cost_per_shot(&weapon, None, None, None, 2, 1.5, 1.0, 1.0, 1.0);
        // mult 1.2: decay 0.06 at markup 1.5 -> 0.09; ammo 2.4 at 1.0.
        assert_eq!(result["totalCostPerUse"], 2.49);
        let breakdown = result["costBreakdown"].as_array().unwrap();
        assert_eq!(breakdown[0]["costPec"], 0.06);
        assert_eq!(breakdown[0]["effectiveCostPec"], 0.09);
        assert_eq!(breakdown[1]["costPec"], 2.4);
    }

    #[test]
    fn falsy_scope_shapes_add_no_breakdown_line() {
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 0}});
        for falsy in [json!(0.0), json!(""), json!([]), json!({}), json!(false)] {
            let result = cost_per_shot(&weapon, None, Some(&falsy), None, 0, 1.0, 1.0, 1.0, 1.0);
            let components: Vec<&str> = result["costBreakdown"]
                .as_array()
                .unwrap()
                .iter()
                .map(|line| line["component"].as_str().unwrap())
                .collect();
            assert_eq!(components, ["Weapon decay"], "scope {falsy}");
        }
    }

    #[test]
    fn truthy_scope_adds_a_marked_up_scope_decay_line_after_amp_before_ammo() {
        // A real scope with positive decay emits a "Scope decay" line at
        // effectiveCostPec = round4(scope_decay * scope_markup), positioned
        // after "Amp decay" and before the ammo lines. The falsy-scope case
        // (no line) is pinned separately; this freezes the positive branch,
        // its ordering, and the scope-markup application.
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 100}});
        let amp = json!({"economy": {"decay": 0.02, "ammo_burn": 0}});
        let scope = json!({"economy": {"decay": 0.04}});
        let result = cost_per_shot(
            &weapon,
            Some(&amp),
            Some(&scope),
            None,
            0,
            1.0,
            1.0,
            1.5,
            1.0,
        );
        let breakdown = result["costBreakdown"].as_array().unwrap();
        let components: Vec<&str> = breakdown
            .iter()
            .map(|line| line["component"].as_str().unwrap())
            .collect();
        assert_eq!(
            components,
            ["Weapon decay", "Amp decay", "Scope decay", "Ammo (weapon)"]
        );
        // scope decay 0.04 @ 1.5 markup = 0.06.
        assert_eq!(breakdown[2]["costPec"], json!(0.04));
        assert_eq!(breakdown[2]["markupMultiplier"], json!(1.5));
        assert_eq!(breakdown[2]["effectiveCostPec"], json!(0.06));
        // 0.05 + 0.02 + 0.06 + 1.0.
        assert_eq!(result["totalCostPerUse"], json!(1.13));
    }

    #[test]
    fn zero_absorption_absorber_adds_no_line_and_keeps_weapon_decay() {
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 0}});
        let absorber = json!({"economy": {"absorption": 0}});
        let result = cost_per_shot(&weapon, None, None, Some(&absorber), 0, 1.0, 1.0, 1.0, 1.0);
        let breakdown = result["costBreakdown"].as_array().unwrap();
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0]["component"], "Weapon decay");
        assert_eq!(breakdown[0]["costPec"], 0.05);
    }

    #[test]
    fn amp_with_zero_ammo_adds_no_amp_ammo_line() {
        let weapon = json!({"economy": {"decay": 0.05, "ammo_burn": 100}});
        let amp = json!({"economy": {"decay": 0.01, "ammo_burn": 0}});
        let result = cost_per_shot(&weapon, Some(&amp), None, None, 0, 1.0, 1.0, 1.0, 1.0);
        let components: Vec<&str> = result["costBreakdown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line["component"].as_str().unwrap())
            .collect();
        assert_eq!(components, ["Weapon decay", "Amp decay", "Ammo (weapon)"]);
    }

    #[test]
    fn heal_reload_defaults_when_uses_per_minute_is_zero() {
        assert_eq!(heal_reload_seconds(&json!({"uses_per_minute": 0})), 2.5);
    }

    #[test]
    fn heal_cost_multiplies_by_markup() {
        let tool = json!({"economy": {"decay": 0.08, "ammo_burn": 0}});
        assert_eq!(heal_cost_per_use(&tool, 2.0), 0.16);
    }

    #[test]
    #[should_panic(expected = "weapon_entity")]
    fn from_props_rejects_a_null_weapon_entity() {
        let _ = cost_per_shot_from_props(&json!({"weapon_entity": null}), None);
    }

    // The split expectations below pin the measured in-game decay-split
    // model (catalogue decay conserved and redistributed; implant share
    // first, extender share on the remainder, multiplicative).

    #[test]
    fn splits_route_decay_shares_at_their_own_markups() {
        // The measured 20% implant + 20% extender case at three markups:
        // implant 20% @ 1.10, extender 20% of the remainder = 16% @ 1.08,
        // weapon keeps 64% @ 15.00 (the 1500% limited-chip scenario).
        let weapon = json!({"economy": {"decay": 1.0, "ammo_burn": 0}});
        let splits = DecaySplits {
            implant_share: 0.2,
            implant_markup: 1.1,
            extender_share: 0.2,
            extender_markup: 1.08,
        };
        let result =
            cost_per_shot_with_splits(&weapon, None, None, None, 0, 15.0, 1.0, 1.0, 1.0, &splits);
        let breakdown = result["costBreakdown"].as_array().unwrap();
        let components: Vec<&str> = breakdown
            .iter()
            .map(|line| line["component"].as_str().unwrap())
            .collect();
        assert_eq!(
            components,
            ["Implant decay", "Extender decay", "Weapon decay"]
        );
        assert_eq!(breakdown[0]["costPec"], json!(0.2));
        assert_eq!(breakdown[0]["effectiveCostPec"], json!(0.22));
        assert_eq!(breakdown[1]["costPec"], json!(0.16));
        assert_eq!(breakdown[1]["effectiveCostPec"], json!(0.1728));
        assert_eq!(breakdown[2]["costPec"], json!(0.64));
        assert_eq!(breakdown[2]["effectiveCostPec"], json!(9.6));
        // Effective markup ~999% of base decay, not the additive 943%.
        assert_eq!(result["totalCostPerUse"], json!(9.9928));
    }

    #[test]
    fn plain_implant_share_takes_its_slice_of_the_decay() {
        // A no-effect implant still takes ~2% of the action's decay.
        let weapon = json!({"economy": {"decay": 0.572, "ammo_burn": 523}});
        let splits = DecaySplits {
            implant_share: 0.02,
            implant_markup: 1.0,
            extender_share: 0.0,
            extender_markup: 1.0,
        };
        let result =
            cost_per_shot_with_splits(&weapon, None, None, None, 0, 1.0, 1.0, 1.0, 1.0, &splits);
        let breakdown = result["costBreakdown"].as_array().unwrap();
        assert_eq!(breakdown[0]["component"], json!("Implant decay"));
        assert_eq!(breakdown[0]["costPec"], json!(0.0114));
        assert_eq!(breakdown[1]["component"], json!("Weapon decay"));
        assert_eq!(breakdown[1]["costPec"], json!(0.5606));
        // Total decay is conserved (0.572) and the ammo line is untouched.
        assert_eq!(result["totalCostPerUse"], json!(5.802));
    }

    #[test]
    fn absorber_applies_to_the_post_split_remainder() {
        let weapon = json!({"economy": {"decay": 1.0, "ammo_burn": 0}});
        let absorber = json!({"economy": {"absorption": 0.5}});
        let splits = DecaySplits {
            implant_share: 0.2,
            implant_markup: 1.0,
            extender_share: 0.0,
            extender_markup: 1.0,
        };
        let result = cost_per_shot_with_splits(
            &weapon,
            None,
            None,
            Some(&absorber),
            0,
            1.0,
            1.0,
            1.0,
            1.0,
            &splits,
        );
        let breakdown = result["costBreakdown"].as_array().unwrap();
        let components: Vec<&str> = breakdown
            .iter()
            .map(|line| line["component"].as_str().unwrap())
            .collect();
        assert_eq!(
            components,
            ["Implant decay", "Absorber decay", "Weapon decay"]
        );
        // Implant 0.2; absorber takes 50% of the 0.8 remainder; weapon keeps 0.4.
        assert_eq!(breakdown[1]["costPec"], json!(0.4));
        assert_eq!(breakdown[2]["costPec"], json!(0.4));
    }

    #[test]
    fn zero_splits_are_bit_identical_to_the_unsplit_engine() {
        let weapon = json!({"economy": {"decay": 0.123456, "ammo_burn": 200}});
        let unsplit = cost_per_shot(&weapon, None, None, None, 2, 1.15, 1.0, 1.0, 1.0);
        let split = cost_per_shot_with_splits(
            &weapon,
            None,
            None,
            None,
            2,
            1.15,
            1.0,
            1.0,
            1.0,
            &DecaySplits::default(),
        );
        assert_eq!(unsplit, split);
    }

    #[test]
    fn splits_parse_from_stored_props() {
        let props = json!({
            "implant": {"name": "NeoPsion 85-B Mindforce Implant (L)",
                         "decay_share_percent": 20.0, "markup_percent": 110},
            "extender": {"name": null, "absorption_percent": 20.0, "markup_percent": 108},
        });
        assert_eq!(
            DecaySplits::from_props(&props),
            DecaySplits {
                implant_share: 0.2,
                implant_markup: 1.1,
                extender_share: 0.2,
                extender_markup: 1.08,
            }
        );
        // Absent, null and empty devices contribute nothing; shares clamp.
        assert_eq!(DecaySplits::from_props(&json!({})), DecaySplits::default());
        assert_eq!(
            DecaySplits::from_props(&json!({"implant": null, "extender": {}})),
            DecaySplits::default()
        );
        let clamped = DecaySplits::from_props(&json!({
            "implant": {"decay_share_percent": 250.0},
        }));
        assert_eq!(clamped.implant_share, 1.0);
        assert_eq!(clamped.implant_markup, 1.0);
    }

    #[test]
    fn from_props_applies_stored_splits() {
        let props = json!({
            "weapon_entity": {"economy": {"decay": 1.0, "ammo_burn": 0}},
            "weapon_markup": 1500,
            "implant": {"decay_share_percent": 20.0, "markup_percent": 110},
            "extender": {"absorption_percent": 20.0, "markup_percent": 108},
        });
        let result = cost_per_shot_from_props(&props, None);
        assert_eq!(result["totalCostPerUse"], json!(9.9928));
    }

    #[test]
    fn heal_splits_price_shares_and_keep_the_legacy_figure_at_zero() {
        let tool = json!({"economy": {"decay": 0.0512, "ammo_burn": 30}});
        // Zero splits reproduce heal_cost_per_use exactly.
        assert_eq!(
            heal_cost_per_use_with_splits(&tool, 1.1, &DecaySplits::default()),
            heal_cost_per_use(&tool, 1.1)
        );
        // 20%/20%: implant 0.01024 @ 1.1, extender 0.008192 @ 1.08,
        // tool keeps 0.032768 + ammo 0.3 @ 1.5.
        let splits = DecaySplits {
            implant_share: 0.2,
            implant_markup: 1.1,
            extender_share: 0.2,
            extender_markup: 1.08,
        };
        let expected = round4(0.01024 * 1.1 + 0.008192 * 1.08 + (0.032768 + 0.3) * 1.5);
        assert_eq!(heal_cost_per_use_with_splits(&tool, 1.5, &splits), expected);
    }

    #[test]
    fn from_props_passes_optional_components_through() {
        let props = json!({
            "weapon_entity": {"economy": {"decay": 0.05, "ammo_burn": 0}},
            "amp_entity": {"economy": {"decay": 0.02, "ammo_burn": 0}},
        });
        let result = cost_per_shot_from_props(&props, None);
        // Weapon decay 0.05 + amp decay 0.02, both at default markup.
        assert_eq!(result["totalCostPerUse"], 0.07);
        let components: Vec<&str> = result["costBreakdown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line["component"].as_str().unwrap())
            .collect();
        assert_eq!(components, ["Weapon decay", "Amp decay"]);
    }
}
