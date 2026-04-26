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
        receiver_type: None,
        receiver_access: ExternalAccessKind::Shared,
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
            receiver_type: None,
            receiver_access: ExternalAccessKind::Shared,
        })
        .unwrap();

    let result = registry.register_function(ExternalFunctionDef {
        name: "test_func",
        parameters: Vec::new(),
        return_type: ExternalAbiType::Void,
        return_alias: ExternalReturnAlias::Fresh,
        receiver_type: None,
        receiver_access: ExternalAccessKind::Shared,
    });

    assert!(result.is_err());
}

#[test]
fn collection_methods_have_receiver_type() {
    let registry = ExternalPackageRegistry::new();
    let push = registry.get_function(COLLECTION_PUSH_HOST_NAME).unwrap();
    assert!(
        push.receiver_type.is_some(),
        "collection push should have receiver_type"
    );
    assert_eq!(push.receiver_access, ExternalAccessKind::Mutable);

    let length = registry.get_function(COLLECTION_LENGTH_HOST_NAME).unwrap();
    assert!(
        length.receiver_type.is_some(),
        "collection length should have receiver_type"
    );
    assert_eq!(length.receiver_access, ExternalAccessKind::Shared);
}

#[test]
fn resolve_method_finds_collection_length() {
    let registry = ExternalPackageRegistry::new();
    let result = registry.resolve_method("Int Collection", COLLECTION_LENGTH_HOST_NAME);
    assert!(
        result.is_some(),
        "resolve_method should find collection length"
    );
}
