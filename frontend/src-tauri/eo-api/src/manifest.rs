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

use schemars::{schema_for, Schema};

use crate::equipment::{
    EquipmentDetail, EquipmentRequest, EquipmentSearchHit, EquipmentSummary, SearchKind,
};
use crate::ApiError;

/// One argument of a typed command.
pub struct ArgSpec {
    pub name: &'static str,
    pub schema: Schema,
}

/// One typed command: its invoke name, arguments, and return schema
/// (`None` for a void return).
pub struct CommandSpec {
    pub name: &'static str,
    pub args: Vec<ArgSpec>,
    pub returns: Option<Schema>,
}

/// The full typed command surface, in emission order.
pub fn manifest() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            name: "equipment_search",
            args: vec![
                ArgSpec {
                    name: "q",
                    schema: schema_for!(String),
                },
                ArgSpec {
                    name: "kind",
                    schema: schema_for!(SearchKind),
                },
            ],
            returns: Some(schema_for!(Vec<EquipmentSearchHit>)),
        },
        CommandSpec {
            name: "equipment_library",
            args: Vec::new(),
            returns: Some(schema_for!(Vec<EquipmentSummary>)),
        },
        CommandSpec {
            name: "equipment_add",
            args: vec![ArgSpec {
                name: "req",
                schema: schema_for!(EquipmentRequest),
            }],
            returns: Some(schema_for!(EquipmentSummary)),
        },
        CommandSpec {
            name: "equipment_update",
            args: vec![
                ArgSpec {
                    name: "item_id",
                    schema: schema_for!(i64),
                },
                ArgSpec {
                    name: "req",
                    schema: schema_for!(EquipmentRequest),
                },
            ],
            returns: Some(schema_for!(EquipmentSummary)),
        },
        CommandSpec {
            name: "equipment_delete",
            args: vec![ArgSpec {
                name: "item_id",
                schema: schema_for!(i64),
            }],
            returns: None,
        },
        CommandSpec {
            name: "equipment_detail",
            args: vec![ArgSpec {
                name: "item_id",
                schema: schema_for!(i64),
            }],
            returns: Some(schema_for!(EquipmentDetail)),
        },
    ]
}

/// The IPC error contract's schema, emitted alongside the commands.
pub fn error_schema() -> Schema {
    schema_for!(ApiError)
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
