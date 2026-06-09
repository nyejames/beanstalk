//! Host function metadata regression tests.
//!
//! WHAT: exercises host return-slot derivation and registry uniqueness rules.
//! WHY: host metadata feeds both AST lowering and borrow-check call summaries, so small
//! regressions here can break multiple frontend stages at once.

use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalAccessKind, ExternalFunctionDef, ExternalFunctionId,
    ExternalFunctionLowerings, ExternalPackageOrigin, ExternalPackageRegistry, ExternalParameter,
    ExternalReturnAlias, ExternalReturnSlot, ExternalSignatureType, external_success_returns,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

fn import_path(components: &[&str], string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_components(
        components
            .iter()
            .map(|component| string_table.intern(component))
            .collect(),
    )
}

#[test]
fn return_slots_preserve_alias_metadata() {
    let host_function = ExternalFunctionDef {
        name: "concat_like".to_owned(),
        parameters: vec![
            ExternalParameter {
                language_type: ExternalSignatureType::Abi(ExternalAbiType::Utf8Str),
                access_kind: ExternalAccessKind::Shared,
            },
            ExternalParameter {
                language_type: ExternalSignatureType::Abi(ExternalAbiType::Utf8Str),
                access_kind: ExternalAccessKind::Shared,
            },
        ],
        returns: vec![ExternalReturnSlot::alias_args(
            ExternalAbiType::Utf8Str,
            vec![1],
        )],
        error_return_type: None,
        receiver_type: None,
        receiver_access: ExternalAccessKind::Shared,
        lowerings: ExternalFunctionLowerings::default(),
    };

    let returns = &host_function.returns;
    assert_eq!(returns.len(), 1);
    assert_eq!(
        returns[0].value_type,
        ExternalSignatureType::Abi(ExternalAbiType::Utf8Str)
    );
    assert!(matches!(
        &returns[0].alias,
        ExternalReturnAlias::AliasArgs(parameter_indices) if parameter_indices == &[1usize]
    ));
}

#[test]
fn register_function_rejects_duplicates() {
    let mut registry = ExternalPackageRegistry::new();
    registry
        .register_function(ExternalFunctionDef {
            name: "test_func".to_owned(),
            parameters: Vec::new(),
            returns: external_success_returns(ExternalAbiType::Void, ExternalReturnAlias::Fresh),
            error_return_type: None,
            receiver_type: None,
            receiver_access: ExternalAccessKind::Shared,
            lowerings: ExternalFunctionLowerings::default(),
        })
        .unwrap();

    let result = registry.register_function(ExternalFunctionDef {
        name: "test_func".to_owned(),
        parameters: Vec::new(),
        returns: external_success_returns(ExternalAbiType::Void, ExternalReturnAlias::Fresh),
        error_return_type: None,
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

    let set = registry
        .get_function_by_id(ExternalFunctionId::CollectionSet)
        .unwrap();
    assert!(
        set.receiver_type.is_some(),
        "collection set should have receiver_type"
    );
    assert_eq!(set.receiver_access, ExternalAccessKind::Mutable);

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

// ------------------------------------------------------------------
// Package identity refactor tests
// ------------------------------------------------------------------

#[test]
fn builtin_packages_resolve_by_path_and_symbol_name() {
    let registry = ExternalPackageRegistry::new();

    let io = registry.resolve_package_function("@core/io", "io");
    assert!(io.is_some(), "@core/io/io should resolve by path and name");

    let get = registry.resolve_package_function("@core/collections", "__bs_collection_get");
    assert!(
        get.is_some(),
        "@core/collections/__bs_collection_get should resolve"
    );
}

#[test]
fn package_ids_are_stable_within_one_registry_build() {
    let registry_a = ExternalPackageRegistry::new();
    let registry_b = ExternalPackageRegistry::new();

    let io_a = registry_a.get_package("@core/io").unwrap();
    let io_b = registry_b.get_package("@core/io").unwrap();

    assert_eq!(
        io_a.id, io_b.id,
        "builtin package IDs must be deterministic"
    );
}

#[test]
fn package_origin_recorded_for_builtins() {
    let registry = ExternalPackageRegistry::new();

    let io = registry.get_package("@core/io").unwrap();
    assert_eq!(io.origin, ExternalPackageOrigin::Builtin);
    assert_eq!(io.path, "@core/io");

    let collections = registry.get_package("@core/collections").unwrap();
    assert_eq!(collections.origin, ExternalPackageOrigin::Builtin);
}

#[test]
fn package_origin_recorded_for_integration_test_packages() {
    let registry = ExternalPackageRegistry::new().with_test_packages_for_integration();

    let pkg_a = registry.get_package("@test/pkg-a").unwrap();
    assert_eq!(pkg_a.origin, ExternalPackageOrigin::BuilderRuntime);

    let pkg_b = registry.get_package("@test/pkg-b").unwrap();
    assert_eq!(pkg_b.origin, ExternalPackageOrigin::BuilderRuntime);
}

#[test]
fn resolve_function_package_returns_readable_path() {
    let registry = ExternalPackageRegistry::new();

    let package_path = registry.resolve_function_package(ExternalFunctionId::Io);
    assert_eq!(package_path, Some("@core/io"));
}

#[test]
fn package_path_to_id_index_is_consistent() {
    let registry = ExternalPackageRegistry::new();

    let io_id = registry.resolve_package_id("@core/io");
    assert!(io_id.is_some());

    let by_id = registry.get_package_by_id(io_id.unwrap());
    assert!(by_id.is_some());
    assert_eq!(by_id.unwrap().path, "@core/io");
}

#[test]
fn package_prefix_lookup_returns_longest_registered_package() {
    let mut registry = ExternalPackageRegistry::new();
    registry
        .register_package("@test", ExternalPackageOrigin::BuilderRuntime)
        .expect("parent test package should register");
    let child_id = registry
        .register_package("@test/pkg", ExternalPackageOrigin::BuilderRuntime)
        .expect("child test package should register");

    let mut string_table = StringTable::new();
    let path = import_path(&["test", "pkg", "open"], &mut string_table);
    let matched = registry
        .longest_package_prefix_for_import(&path, &string_table)
        .expect("package prefix should match");

    assert_eq!(matched.package_path, "@test/pkg");
    assert_eq!(matched.package_id, child_id);
    assert_eq!(matched.matched_component_count, 2);
}

#[test]
fn package_prefix_lookup_supports_exact_namespace_imports() {
    let mut registry = ExternalPackageRegistry::new();
    crate::libraries::core::register_core_math_package(&mut registry);

    let mut string_table = StringTable::new();
    let path = import_path(&["core", "math"], &mut string_table);
    let matched = registry
        .longest_package_prefix_for_import(&path, &string_table)
        .expect("core math package should match");

    assert_eq!(matched.package_path, "@core/math");
    assert_eq!(matched.matched_component_count, path.len());
}

#[test]
fn virtual_package_detection_uses_symbol_suffixes() {
    let mut registry = ExternalPackageRegistry::new();
    crate::libraries::core::register_core_math_package(&mut registry);

    let mut string_table = StringTable::new();
    let package_symbol = import_path(&["core", "math", "sin"], &mut string_table);
    let source_path = import_path(&["core", "missing", "sin"], &mut string_table);

    assert!(registry.is_virtual_package_import(&package_symbol, &string_table));
    assert!(!registry.is_virtual_package_import(&source_path, &string_table));
}
