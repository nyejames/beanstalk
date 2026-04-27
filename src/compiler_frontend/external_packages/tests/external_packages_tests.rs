//! Host function metadata regression tests.
//!
//! WHAT: exercises host return-slot derivation and registry uniqueness rules.
//! WHY: host metadata feeds both AST lowering and borrow-check call summaries, so small
//! regressions here can break multiple frontend stages at once.

use crate::compiler_frontend::ast::statements::functions::FunctionReturn;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalFunctionDef, ExternalFunctionId,
    ExternalFunctionLowerings, ExternalPackageRegistry, ExternalParameter, ExternalReturnAlias,
};

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
        lowerings: ExternalFunctionLowerings::default(),
    };

    let returns = host_function.return_slots();
    assert_eq!(returns.len(), 1);
    assert!(matches!(
        &returns[0].value,
        FunctionReturn::AliasCandidates {
            parameter_indices,
            data_type
        } if parameter_indices == &[1usize] && data_type == &DataType::StringSlice
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
            lowerings: ExternalFunctionLowerings::default(),
        })
        .unwrap();

    let result = registry.register_function(ExternalFunctionDef {
        name: "test_func",
        parameters: Vec::new(),
        return_type: ExternalAbiType::Void,
        return_alias: ExternalReturnAlias::Fresh,
        receiver_type: None,
        receiver_access: ExternalAccessKind::Shared,
        lowerings: ExternalFunctionLowerings::default(),
    });

    assert!(result.is_err());
}

#[test]
fn collection_methods_have_receiver_type() {
    let registry = ExternalPackageRegistry::new();
    let push = registry
        .get_function_by_id(ExternalFunctionId::CollectionPush)
        .unwrap();
    assert!(
        push.receiver_type.is_some(),
        "collection push should have receiver_type"
    );
    assert_eq!(push.receiver_access, ExternalAccessKind::Mutable);

    let length = registry
        .get_function_by_id(ExternalFunctionId::CollectionLength)
        .unwrap();
    assert!(
        length.receiver_type.is_some(),
        "collection length should have receiver_type"
    );
    assert_eq!(length.receiver_access, ExternalAccessKind::Shared);
}

#[test]
fn same_symbol_name_across_packages_is_allowed() {
    let registry = ExternalPackageRegistry::new().with_test_packages_for_integration();

    // Both @test/pkg-a and @test/pkg-b expose a function named "open".
    let a_result = registry.resolve_package_function("@test/pkg-a", "open");
    assert!(a_result.is_some(), "@test/pkg-a/open should resolve");

    let b_result = registry.resolve_package_function("@test/pkg-b", "open");
    assert!(b_result.is_some(), "@test/pkg-b/open should resolve");

    // They must map to distinct IDs.
    let (a_id, _) = a_result.unwrap();
    let (b_id, _) = b_result.unwrap();
    assert_ne!(
        a_id, b_id,
        "same symbol in different packages must have distinct IDs"
    );
}

#[test]
fn resolve_package_function_selects_correct_package() {
    let registry = ExternalPackageRegistry::new().with_test_packages_for_integration();

    let (a_id, a_def) = registry
        .resolve_package_function("@test/pkg-a", "open")
        .unwrap();
    let (b_id, b_def) = registry
        .resolve_package_function("@test/pkg-b", "open")
        .unwrap();

    assert_eq!(a_def.name, "open");
    assert_eq!(b_def.name, "open");
    assert_ne!(a_id, b_id);
}
