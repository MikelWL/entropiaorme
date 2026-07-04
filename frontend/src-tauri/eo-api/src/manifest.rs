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

use crate::character::{
    CalibrationStatus, CharacterProspectOptions, ComputedCharacterStats, HpOptimizerResult,
    PathOptimizerResult, ProfessionLevel, ProfessionOptimizerResult, ProspectQuery, ProspectResult,
    SkillLevel,
};
use crate::equipment::{
    EquipmentDetail, EquipmentRequest, EquipmentSearchHit, EquipmentSummary, SearchKind,
};
use crate::ApiError;

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
