//! Host function metadata regression tests.
//!
//! WHAT: exercises host return-slot derivation and registry uniqueness rules.
//! WHY: host metadata feeds both AST lowering and borrow-check call summaries, so small
//! regressions here can break multiple frontend stages at once.

use super::*;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;

#[test]
fn return_slots_preserve_alias_metadata() {
    let host_function = ExternalFunctionDef {
        name: "concat_like",
        parameters: vec![
            ExternalParameter {
                language_type: ExternalAbiType::Utf8Str,
                access_kind: ExternalAccessKind::Shared,
            },
            ExternalParameter {
                language_type: ExternalAbiType::Utf8Str,
                access_kind: ExternalAccessKind::Shared,
            },
        ],
        return_type: ExternalAbiType::Utf8Str,
        return_alias: ExternalReturnAlias::AliasArgs(vec![1]),
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
    let mut registry = ExternalPackageRegistry::new();
    registry
        .register_function(ExternalFunctionDef {
            name: "test_func",
            parameters: Vec::new(),
            return_type: ExternalAbiType::Void,
            return_alias: ExternalReturnAlias::Fresh,
        })
        .unwrap();

    let result = registry.register_function(ExternalFunctionDef {
        name: "test_func",
        parameters: Vec::new(),
        return_type: ExternalAbiType::Void,
        return_alias: ExternalReturnAlias::Fresh,
    });

    assert!(result.is_err());
}
