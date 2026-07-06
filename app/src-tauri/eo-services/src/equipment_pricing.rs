//! Equipment pricing lookups over the equipment library: the per-shot
//! weapon cost, the healing-tool per-use cost and reload, and the
//! synchronous row lookups the input listeners resolve tools through.
//!
//! The composition root wires these into the hotbar resolver and the
//! tracker's equipment seam; the arithmetic composes [`crate::cost_engine`]
//! so the figures stay bit-identical to the frozen goldens that pin them
//! downstream.

use serde_json::Value;

use crate::cost_engine::{cost_per_shot_from_props, heal_cost_per_use, heal_reload_seconds};
use crate::db::DbError;

/// The per-shot cost in PED derived from a weapon's properties:
/// `totalCostPerUse / 100`, or 0 when the profile carries no figure.
pub fn cost_per_shot_ped(props: &Value) -> f64 {
    cost_per_shot_from_props(props, None)["totalCostPerUse"]
        .as_f64()
        .unwrap_or(0.0)
        / 100.0
}

/// The per-shot weapon cost in PED for a tool by name fragment
/// (`totalCostPerUse / 100`), or 0 when the tool is unknown. A read
/// failure degrades to 0 (the tool-unknown value), so a weapon slot
/// still resolves rather than dropping the tool change.
pub fn weapon_cost_by_name(conn: &rusqlite::Connection, name: &str) -> f64 {
    match weapon_properties_by_name_fragment_sync(conn, name)
        .ok()
        .flatten()
    {
        Some(properties_json) => {
            let props: Value = serde_json::from_str(&properties_json).unwrap_or(Value::Null);
            cost_per_shot_ped(&props)
        }
        None => 0.0,
    }
}

/// The healing-tool per-use cost (PED) and reload (seconds) from a row's
/// properties: a missing or empty tool entity falls back to `(0, 2.5)`;
/// otherwise the cost engine's per-use cost over the entity and its
/// markup, in PED, and the entity's reload.
pub fn heal_cost_from_props(properties_json: &str) -> (f64, f64) {
    let props: Value = serde_json::from_str(properties_json).unwrap_or(Value::Null);
    let tool = props
        .get("tool_entity")
        .filter(|value| !value.is_null() && value.as_object().is_none_or(|map| !map.is_empty()));
    let Some(tool) = tool else {
        return (0.0, 2.5);
    };
    let markup = props.get("markup").and_then(Value::as_f64).unwrap_or(100.0) / 100.0;
    (
        heal_cost_per_use(tool, markup) / 100.0,
        heal_reload_seconds(tool),
    )
}

/// One equipment-library row by id: `(name, item_type, properties JSON)`,
/// or None when absent. The synchronous reader-core counterpart of
/// [`crate::db::Db::hotbar_equipment_row`], for callers on a plain OS
/// thread (the hotbar listener's key thread).
pub fn hotbar_equipment_row_sync(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<Option<(String, String, String)>, DbError> {
    use rusqlite::OptionalExtension as _;
    let row = conn
        .query_row(
            "SELECT name, item_type, properties_json FROM equipment_library \
             WHERE id = ?",
            rusqlite::params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    Ok(row)
}

/// The first weapon-row `properties_json` whose name contains the
/// fragment, the synchronous reader-core counterpart of
/// [`crate::db::Db::weapon_properties_by_name_fragment`]: a
/// `LIKE '%fragment%'` over weapon rows with the fragment's own
/// `%` / `_` / `\` escaped under an explicit `ESCAPE '\'`, and the
/// fragment trimmed exactly as the async path trims it.
pub fn weapon_properties_by_name_fragment_sync(
    conn: &rusqlite::Connection,
    fragment: &str,
) -> Result<Option<String>, DbError> {
    use rusqlite::OptionalExtension as _;
    let safe = fragment
        .trim()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let row = conn
        .query_row(
            "SELECT properties_json FROM equipment_library \
             WHERE item_type = 'weapon' AND name LIKE ? ESCAPE '\\'",
            rusqlite::params![format!("%{safe}%")],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(row)
}
