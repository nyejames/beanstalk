//! Host function metadata regression tests.
//!
//! WHAT: exercises host return-slot derivation and registry uniqueness rules.
//! WHY: host metadata feeds both AST lowering and borrow-check call summaries, so small
//! regressions here can break multiple frontend stages at once.

use super::*;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;

#[test]
fn return_slots_preserve_alias_metadata() {
    let host_function = HostFunctionDef {
        name: "concat_like",
        parameters: vec![
            HostParameter {
                language_type: DataType::StringSlice,
                access_kind: HostAccessKind::Shared,
            },
            HostParameter {
                language_type: DataType::StringSlice,
                access_kind: HostAccessKind::Shared,
            },
        ],
        return_type: HostAbiType::Utf8Str,
        return_alias: HostReturnAlias::AliasArgs(vec![1]),
    };

    let returns = host_function.return_slots();
    assert_eq!(returns.len(), 1);
    assert!(matches!(
        &returns[0].value,
        FunctionReturn::AliasCandidates {
            parameter_indices,
            data_type
        } if parameter_indices == &vec![1] && data_type == &DataType::StringSlice
    ));
}

#[test]
fn register_function_rejects_duplicates() {
    let mut registry = HostRegistry::new();
    let result = registry.register_function(HostFunctionDef {
        name: IO_FUNC_NAME,
        parameters: Vec::new(),
        return_type: HostAbiType::Void,
        return_alias: HostReturnAlias::Fresh,
    });

    assert!(result.is_err());
}
