//! Host function metadata regression tests.
//!
//! WHAT: exercises host signature derivation and registry uniqueness rules.
//! WHY: host metadata feeds both AST lowering and borrow-check call summaries, so small
//! regressions here can break multiple frontend stages at once.

use super::*;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;

#[test]
fn params_to_signature_preserves_alias_metadata() {
    let mut string_table = StringTable::new();
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

    let signature = host_function.params_to_signature(&mut string_table);
    assert_eq!(signature.parameters.len(), 2);
    assert_eq!(signature.returns.len(), 1);
    assert!(matches!(
        &signature.returns[0].value,
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
