//! Focused hidden-invariant tests for the canonical closed-type identity vocabulary and
//! projection.
//!
//! WHAT: exercises the structural invariants of `CanonicalTypeIdentity` values and the
//! `TypeId -> CanonicalTypeIdentity` projection that integration output cannot inspect:
//! equality/stability of every closed-type shape, and `CompilerError` for every unsupported or
//! incomplete state.
//! WHY: these are pure value and projection invariants owned by
//! `compiler_frontend::canonical_type_identity`, so they own a focused test beside the module
//! rather than an end-to-end case.

use crate::compiler_frontend::canonical_type_identity::{
    CanonicalBuiltinType, CanonicalTypeIdentity, CanonicalTypeProjectionContext,
    CollectionTypeIdentity, ExternalOpaqueTypeIdentity, FallibleCarrierTypeIdentity,
    GenericInstanceTypeIdentity, NominalOriginResolver, OrderedMapTypeIdentity,
    project_type_id_to_canonical_identity,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantDefinition, FieldDefinition, FunctionParameterDefinition,
    FunctionTypeDefinition, StructTypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{
    BuiltinTypeConstructor, GenericParameterId, GenericParameterListId, NominalTypeId,
    TypeConstructor, TypeId,
};
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalPackageRegistry, ExternalSymbolPath, ExternalTypeDef, ExternalTypeId,
    IO_INPUT_EXTERNAL_TYPE_ID,
};
use crate::compiler_frontend::semantic_identity::{
    ModuleRootRole, OriginTypeCategory, OriginTypeId, StableModuleOriginIdentity,
    StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use rustc_hash::FxHashMap;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
//  Test fixtures
// ---------------------------------------------------------------------------

/// Map-backed nominal origin resolver for focused tests.
struct MapNominalOriginResolver {
    origins: FxHashMap<NominalTypeId, OriginTypeId>,
}

impl MapNominalOriginResolver {
    fn new() -> Self {
        Self {
            origins: FxHashMap::default(),
        }
    }

    fn register(&mut self, nominal_id: NominalTypeId, origin: OriginTypeId) {
        self.origins.insert(nominal_id, origin);
    }
}

impl NominalOriginResolver for MapNominalOriginResolver {
    fn resolve_nominal_origin(
        &self,
        nominal_id: NominalTypeId,
    ) -> Result<OriginTypeId, CompilerError> {
        self.origins.get(&nominal_id).cloned().ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "no source-nominal origin registered for NominalTypeId({})",
                nominal_id.0
            ))
        })
    }
}

fn module_origin(logical_path: &str) -> StableModuleOriginIdentity {
    StableModuleOriginIdentity::from_portable_path(
        StablePackageIdentity::project_local("test-project"),
        logical_path.to_owned(),
        ModuleRootRole::Normal,
    )
}

fn struct_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(
        module_origin("shapes"),
        name.to_owned(),
        OriginTypeCategory::Struct,
    )
}

fn choice_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(
        module_origin("shapes"),
        name.to_owned(),
        OriginTypeCategory::Choice,
    )
}

fn empty_fields() -> Box<[FieldDefinition]> {
    Box::new([])
}

fn no_variants() -> Box<[ChoiceVariantDefinition]> {
    Box::new([])
}

fn location() -> SourceLocation {
    SourceLocation::default()
}

/// Registers a struct in the environment and returns its `NominalTypeId` and `TypeId`.
fn register_struct(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    name: &str,
) -> (NominalTypeId, TypeId) {
    let path = InternedPath::from_single_str(name, string_table);
    env.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path,
        fields: empty_fields(),
        generic_parameters: None,
        const_record: false,
    })
}

/// Registers a struct with a single generic parameter and returns its `NominalTypeId` and
/// `TypeId`.
fn register_generic_struct(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    name: &str,
    param_list_id: GenericParameterListId,
) -> (NominalTypeId, TypeId) {
    let path = InternedPath::from_single_str(name, string_table);
    env.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path,
        fields: empty_fields(),
        generic_parameters: Some(param_list_id),
        const_record: false,
    })
}

fn register_choice(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    name: &str,
) -> (NominalTypeId, TypeId) {
    let path = InternedPath::from_single_str(name, string_table);
    env.register_nominal_choice(ChoiceTypeDefinition {
        id: NominalTypeId(0),
        path,
        variants: no_variants(),
        generic_parameters: None,
    })
}

/// Registers a single generic parameter list with one parameter named `T`.
fn register_single_param_list(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
) -> GenericParameterListId {
    use crate::compiler_frontend::datatypes::generic_parameters::{
        GenericParameter, GenericParameterList, TypeParameterId,
    };
    let list = GenericParameterList {
        parameters: vec![GenericParameter {
            id: TypeParameterId(0),
            name: string_table.intern("T"),
            location: location(),
            trait_bounds: Vec::new(),
        }],
    };
    env.register_generic_parameter_list(&list, &FxHashMap::default())
        .list_id
}

/// Builds a test registry with one registered external type at `@test/canvas.Canvas`.
fn test_registry_with_canvas_type() -> ExternalPackageRegistry {
    let mut registry = ExternalPackageRegistry::default();
    let package_id = registry
        .register_package(
            "@test/canvas",
            crate::builder_surface::PackageOrigin::Builder,
        )
        .expect("test package should register");
    registry
        .register_type_in_package(
            package_id,
            ExternalTypeId(100),
            ExternalTypeDef {
                name: "Canvas".to_owned(),
                package_id,
                abi_type: ExternalAbiType::Handle,
            },
        )
        .expect("test type should register");
    registry
}

/// Builds a test registry using the built-in `@core/io` package, which already registers
/// `io.input.Input` at `IO_INPUT_EXTERNAL_TYPE_ID`.
fn builtin_registry() -> ExternalPackageRegistry {
    ExternalPackageRegistry::new()
}

fn projection_context<'a>(
    resolver: &'a MapNominalOriginResolver,
    registry: &'a ExternalPackageRegistry,
) -> CanonicalTypeProjectionContext<'a> {
    CanonicalTypeProjectionContext::new(resolver, registry)
}

// ---------------------------------------------------------------------------
//  Builtin projection
// ---------------------------------------------------------------------------

#[test]
fn projects_every_builtin_scalar() {
    let env = TypeEnvironment::new();
    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let cases = [
        (env.builtins().bool, CanonicalBuiltinType::Bool),
        (env.builtins().int, CanonicalBuiltinType::Int),
        (env.builtins().float, CanonicalBuiltinType::Float),
        (env.builtins().decimal, CanonicalBuiltinType::Decimal),
        (env.builtins().string, CanonicalBuiltinType::String),
        (env.builtins().char, CanonicalBuiltinType::Char),
        (env.builtins().range, CanonicalBuiltinType::Range),
        (env.builtins().none, CanonicalBuiltinType::None),
    ];

    for (type_id, expected_builtin) in cases {
        let identity = project_type_id_to_canonical_identity(type_id, &env, &context)
            .expect("builtin scalar projection should succeed");
        assert_eq!(
            identity,
            CanonicalTypeIdentity::Builtin(expected_builtin),
            "builtin TypeId({}) should project to {:?}",
            type_id.0,
            expected_builtin
        );
    }
}

#[test]
fn builtin_none_identity_is_canonical() {
    let env = TypeEnvironment::new();
    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let none_identity = project_type_id_to_canonical_identity(env.builtins().none, &env, &context)
        .expect("None builtin should project");

    assert_eq!(
        none_identity,
        CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::None),
        "the semantically seeded None identity must be a canonical builtin"
    );
}

// ---------------------------------------------------------------------------
//  Source nominal projection
// ---------------------------------------------------------------------------

#[test]
fn projects_direct_struct_to_source_nominal_origin() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let (nominal_id, type_id) = register_struct(&mut env, &mut string_table, "Button");

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(nominal_id, struct_origin("Button"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect("struct projection should succeed");

    assert_eq!(
        identity,
        CanonicalTypeIdentity::SourceNominal(struct_origin("Button")),
        "direct struct must project to its stable source-nominal origin"
    );
}

#[test]
fn projects_direct_choice_to_source_nominal_origin() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let (nominal_id, type_id) = register_choice(&mut env, &mut string_table, "Status");

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(nominal_id, choice_origin("Status"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect("choice projection should succeed");

    assert_eq!(
        identity,
        CanonicalTypeIdentity::SourceNominal(choice_origin("Status")),
        "direct choice must project to its stable source-nominal origin"
    );
}

#[test]
fn struct_and_choice_with_same_origin_category_are_distinct() {
    let struct_origin_id = struct_origin("Shape");
    let choice_origin_id = choice_origin("Shape");
    assert_ne!(
        struct_origin_id, choice_origin_id,
        "struct and choice origins with the same name must differ by category"
    );
}

// ---------------------------------------------------------------------------
//  External opaque projection
// ---------------------------------------------------------------------------

#[test]
fn projects_external_opaque_to_owned_package_and_symbol_path() {
    let mut env = TypeEnvironment::new();
    let registry = builtin_registry();
    let external_type_id = IO_INPUT_EXTERNAL_TYPE_ID;
    let type_id = env.intern_external(external_type_id);

    let resolver = MapNominalOriginResolver::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect("external opaque projection should succeed");

    let expected_symbol_path =
        ExternalSymbolPath::from_components(vec!["input".to_owned(), "Input".to_owned()]);
    match &identity {
        CanonicalTypeIdentity::ExternalOpaque(opaque) => {
            assert_eq!(
                opaque.package_path(),
                "@core/io",
                "external opaque must carry the owned package path"
            );
            assert_eq!(
                opaque.symbol_path(),
                &expected_symbol_path,
                "external opaque must carry the structured symbol path"
            );
        }
        other => panic!("expected ExternalOpaque, got {other:?}"),
    }
}

#[test]
fn external_opaque_identity_is_equal_for_equal_package_and_symbol() {
    let a = ExternalOpaqueTypeIdentity::new(
        "@core/io".to_owned(),
        ExternalSymbolPath::from_components(vec!["input".to_owned(), "Input".to_owned()]),
    );
    let b = ExternalOpaqueTypeIdentity::new(
        "@core/io".to_owned(),
        ExternalSymbolPath::from_components(vec!["input".to_owned(), "Input".to_owned()]),
    );
    assert_eq!(
        a, b,
        "equal package path and symbol path must yield equal identity"
    );

    let mut set = HashSet::new();
    set.insert(a.clone());
    assert!(
        set.contains(&b),
        "equal identity must hash to the same slot"
    );
}

#[test]
fn external_opaque_identity_distinguishes_different_packages() {
    let a = ExternalOpaqueTypeIdentity::new(
        "@core/io".to_owned(),
        ExternalSymbolPath::from_single("Input"),
    );
    let b = ExternalOpaqueTypeIdentity::new(
        "@test/canvas".to_owned(),
        ExternalSymbolPath::from_single("Input"),
    );
    assert_ne!(a, b, "same symbol path in different packages must differ");
}

#[test]
fn projects_external_opaque_from_test_registry() {
    let mut env = TypeEnvironment::new();
    let registry = test_registry_with_canvas_type();
    let type_id = env.intern_external(ExternalTypeId(100));

    let resolver = MapNominalOriginResolver::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect("test external opaque projection should succeed");

    match &identity {
        CanonicalTypeIdentity::ExternalOpaque(opaque) => {
            assert_eq!(opaque.package_path(), "@test/canvas");
            assert_eq!(
                opaque.symbol_path(),
                &ExternalSymbolPath::from_single("Canvas")
            );
        }
        other => panic!("expected ExternalOpaque, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
//  Option, collection, map and fallible carrier projection
// ---------------------------------------------------------------------------

#[test]
fn projects_option_of_builtin() {
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let option_id = env.intern_option(int_id);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(option_id, &env, &context)
        .expect("option projection should succeed");

    assert_eq!(
        identity,
        CanonicalTypeIdentity::Option(Box::new(CanonicalTypeIdentity::Builtin(
            CanonicalBuiltinType::Int
        ))),
        "Option<Int> must project recursively"
    );
}

#[test]
fn projects_growable_collection() {
    let mut env = TypeEnvironment::new();
    let string_id = env.builtins().string;
    let collection_id = env.intern_collection(string_id, None);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(collection_id, &env, &context)
        .expect("growable collection projection should succeed");

    assert_eq!(
        identity,
        CanonicalTypeIdentity::Collection(CollectionTypeIdentity::new(
            CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String),
            None
        )),
        "growable {{String}} must project with fixed_capacity = None"
    );
}

#[test]
fn projects_fixed_collection_distinct_from_growable() {
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let fixed_id = env.intern_collection(int_id, Some(4));
    let growable_id = env.intern_collection(int_id, None);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let fixed_identity = project_type_id_to_canonical_identity(fixed_id, &env, &context)
        .expect("fixed collection projection should succeed");
    let growable_identity = project_type_id_to_canonical_identity(growable_id, &env, &context)
        .expect("growable collection projection should succeed");

    assert_ne!(
        fixed_identity, growable_identity,
        "fixed and growable collections of the same element must be distinct"
    );

    match &fixed_identity {
        CanonicalTypeIdentity::Collection(collection) => {
            assert_eq!(collection.fixed_capacity(), Some(4));
        }
        other => panic!("expected Collection, got {other:?}"),
    }
}

#[test]
fn projects_ordered_map_preserving_key_value_order() {
    let mut env = TypeEnvironment::new();
    let key_id = env.builtins().string;
    let value_id = env.builtins().int;
    let map_id = env.intern_map(key_id, value_id);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(map_id, &env, &context)
        .expect("map projection should succeed");

    let expected = CanonicalTypeIdentity::OrderedMap(OrderedMapTypeIdentity::new(
        CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String),
        CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
    ));
    assert_eq!(
        identity, expected,
        "{{String = Int}} must preserve key/value order"
    );

    // Swapping key and value must produce a distinct identity.
    let swapped_map_id = env.intern_map(value_id, key_id);
    let swapped_identity = project_type_id_to_canonical_identity(swapped_map_id, &env, &context)
        .expect("swapped map projection should succeed");
    assert_ne!(
        identity, swapped_identity,
        "swapping map key and value must change identity"
    );
}

#[test]
fn projects_fallible_carrier_preserving_success_error_order() {
    let mut env = TypeEnvironment::new();
    let success_id = env.builtins().int;
    let error_id = env.builtins().string;
    let carrier_id = env.intern_fallible_carrier(success_id, error_id);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(carrier_id, &env, &context)
        .expect("fallible carrier projection should succeed");

    let expected = CanonicalTypeIdentity::FallibleCarrier(FallibleCarrierTypeIdentity::new(
        CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
        CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String),
    ));
    assert_eq!(
        identity, expected,
        "Int!String must preserve success/error order"
    );

    // Swapping success and error must produce a distinct identity.
    let swapped_carrier_id = env.intern_fallible_carrier(error_id, success_id);
    let swapped_identity =
        project_type_id_to_canonical_identity(swapped_carrier_id, &env, &context)
            .expect("swapped carrier projection should succeed");
    assert_ne!(
        identity, swapped_identity,
        "swapping success and error channels must change identity"
    );
}

// ---------------------------------------------------------------------------
//  Concrete generic nominal instance projection
// ---------------------------------------------------------------------------

#[test]
fn projects_concrete_generic_nominal_instance() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table);
    let (box_nominal_id, _) =
        register_generic_struct(&mut env, &mut string_table, "Box", param_list_id);

    let int_id = env.builtins().int;
    let instance_id = env.intern_generic_instance(box_nominal_id, Box::new([int_id]));

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(box_nominal_id, struct_origin("Box"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(instance_id, &env, &context)
        .expect("generic instance projection should succeed");

    let expected = CanonicalTypeIdentity::GenericInstance(GenericInstanceTypeIdentity::new(
        struct_origin("Box"),
        Box::new([CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)]),
    ));
    assert_eq!(
        identity, expected,
        "Box<Int> must project to base origin plus canonical concrete arguments"
    );
}

#[test]
fn generic_instance_identity_is_equal_for_equal_base_and_arguments() {
    let base = struct_origin("Box");
    let args = Box::new([CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)]);
    let a = GenericInstanceTypeIdentity::new(base.clone(), args.clone());
    let b = GenericInstanceTypeIdentity::new(base, args);
    assert_eq!(a, b, "equal base and arguments must yield equal identity");

    let mut set = HashSet::new();
    set.insert(a.clone());
    assert!(
        set.contains(&b),
        "equal identity must hash to the same slot"
    );
}

#[test]
fn generic_instance_distinguishes_different_concrete_arguments() {
    let base = struct_origin("Box");
    let int_args = Box::new([CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)]);
    let string_args = Box::new([CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String)]);
    let int_instance = GenericInstanceTypeIdentity::new(base.clone(), int_args);
    let string_instance = GenericInstanceTypeIdentity::new(base, string_args);
    assert_ne!(
        int_instance, string_instance,
        "different concrete arguments must yield distinct identities"
    );
}

// ---------------------------------------------------------------------------
//  CompilerError states
// ---------------------------------------------------------------------------

#[test]
fn absent_nominal_origin_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let (nominal_id, type_id) = register_struct(&mut env, &mut string_table, "Missing");

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect_err("absent nominal origin must return an error");

    assert_eq!(
        error.error_type,
        ErrorType::Compiler,
        "absent nominal origin must use the compiler-error lane"
    );
    assert!(
        error.msg.contains("source-nominal origin") && error.msg.contains("struct"),
        "error should name the struct nominal origin: {}",
        error.msg
    );
    // The NominalTypeId is part of the context but not embedded in the canonical identity.
    let _ = nominal_id;
}

#[test]
fn absent_external_identity_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let unregistered_external_id = ExternalTypeId(999);
    let type_id = env.intern_external(unregistered_external_id);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(type_id, &env, &context)
        .expect_err("absent external identity must return an error");

    assert_eq!(
        error.error_type,
        ErrorType::Compiler,
        "absent external identity must use the compiler-error lane"
    );
    assert!(
        error.msg.contains("ExternalTypeId") && error.msg.contains("999"),
        "error should name the unregistered ExternalTypeId: {}",
        error.msg
    );
}

#[test]
fn unresolved_generic_parameter_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let param_type_id =
        env.intern_generic_parameter(GenericParameterId(0), StringTable::new().intern("T"));

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(param_type_id, &env, &context)
        .expect_err("generic parameter must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("generic parameter"),
        "error should name the generic parameter state: {}",
        error.msg
    );
}

#[test]
fn function_type_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let function_id = env.insert_function_type_for_test(FunctionTypeDefinition {
        parameters: Box::new([FunctionParameterDefinition {
            name: None,
            type_id: int_id,
        }]),
        returns: Box::new([int_id]),
        error_return: None,
    });

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(function_id, &env, &context)
        .expect_err("function type must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("function"),
        "error should name the function type state: {}",
        error.msg
    );
}

#[test]
fn tuple_type_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let tuple_id = env.intern_tuple(vec![int_id, int_id]);

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(tuple_id, &env, &context)
        .expect_err("tuple type must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("tuple"),
        "error should name the tuple state: {}",
        error.msg
    );
}

#[test]
fn malformed_collection_arity_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    // Directly intern a collection with zero arguments, which is a malformed arity.
    let malformed_id = env.intern_constructed(
        TypeConstructor::Builtin(BuiltinTypeConstructor::Collection {
            fixed_capacity: None,
        }),
        Box::new([]),
    );

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(malformed_id, &env, &context)
        .expect_err("malformed collection arity must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("collection") && error.msg.contains("arity"),
        "error should name the collection arity: {}",
        error.msg
    );
}

#[test]
fn malformed_map_arity_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    // Directly intern an ordered map with one argument, which is a malformed arity.
    let malformed_id = env.intern_constructed(
        TypeConstructor::Builtin(BuiltinTypeConstructor::OrderedMap),
        Box::new([int_id]),
    );

    let resolver = MapNominalOriginResolver::new();
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(malformed_id, &env, &context)
        .expect_err("malformed map arity must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("ordered map") && error.msg.contains("arity"),
        "error should name the map arity: {}",
        error.msg
    );
}

#[test]
fn malformed_generic_instance_arity_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table);
    let (box_nominal_id, _) =
        register_generic_struct(&mut env, &mut string_table, "Box", param_list_id);

    // Box declares 1 generic parameter but we intern an instance with 0 arguments.
    let malformed_id = env.intern_generic_instance(box_nominal_id, Box::new([]));

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(box_nominal_id, struct_origin("Box"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(malformed_id, &env, &context)
        .expect_err("malformed generic-instance arity must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("malformed generic-instance arity"),
        "error should name the generic-instance arity: {}",
        error.msg
    );
}

/// Verifies that a zero-argument `GenericInstance` built from a non-generic nominal is rejected.
///
/// The previous silent `0` fallback let `Thing[]` project as a legal concrete instance of a
/// nominal that does not actually declare generic parameters. The projection must reject it.
#[test]
fn generic_instance_of_non_generic_base_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let (plain_nominal_id, _) = register_struct(&mut env, &mut string_table, "Thing");

    // Thing declares no generic parameters; a zero-argument instance is not a legal instance.
    let instance_id = env.intern_generic_instance(plain_nominal_id, Box::new([]));

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(plain_nominal_id, struct_origin("Thing"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(instance_id, &env, &context)
        .expect_err("a generic instance of a non-generic base must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("generic parameter list is absent"),
        "error should name the absent generic parameter list: {}",
        error.msg
    );
}

/// Verifies that a `GenericInstance` whose base nominal is unknown to the `TypeEnvironment` is
/// rejected even when the origin resolver claims an origin for it.
#[test]
fn generic_instance_of_unknown_base_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    // NominalTypeId(999) is never registered in the TypeEnvironment.
    let unknown_base = NominalTypeId(999);
    let instance_id = env.intern_generic_instance(unknown_base, Box::new([]));

    let mut resolver = MapNominalOriginResolver::new();
    // The origin resolver knows an origin for the unknown base, so origin resolution succeeds
    // and the base validation is what catches the missing nominal.
    resolver.register(unknown_base, struct_origin("Phantom"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(instance_id, &env, &context)
        .expect_err("a generic instance of an unknown base must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error
            .msg
            .contains("neither a registered struct nor a choice"),
        "error should name the unknown nominal base: {}",
        error.msg
    );
}

/// Verifies that a `GenericInstance` whose base declares a generic parameter list that is
/// missing from the `TypeEnvironment` is rejected. The base is registered with a dangling
/// `GenericParameterListId` through the normal registration API, so no production escape hatch
/// is needed to construct this inconsistent state.
#[test]
fn generic_instance_with_missing_parameter_list_returns_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    // Register a struct that claims a parameter list that was never registered.
    let (dangling_nominal_id, _) = register_generic_struct(
        &mut env,
        &mut string_table,
        "Dangling",
        GenericParameterListId(777),
    );

    let instance_id = env.intern_generic_instance(dangling_nominal_id, Box::new([]));

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(dangling_nominal_id, struct_origin("Dangling"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let error = project_type_id_to_canonical_identity(instance_id, &env, &context)
        .expect_err("a generic instance whose parameter list is missing must return an error");

    assert_eq!(error.error_type, ErrorType::Compiler);
    assert!(
        error.msg.contains("missing from the TypeEnvironment"),
        "error should name the missing parameter list: {}",
        error.msg
    );
}

// ---------------------------------------------------------------------------
//  Equality and stability across independent construction
// ---------------------------------------------------------------------------

#[test]
fn equal_construction_yields_equal_and_hash_equal_identity() {
    let a = CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int);
    let b = CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int);

    assert_eq!(a, b, "equal construction must yield equal identity");
    let mut set = HashSet::new();
    set.insert(a);
    assert!(
        set.contains(&b),
        "equal identity must hash to the same slot"
    );
}

#[test]
fn distinct_builtins_are_distinct_identities() {
    let int = CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int);
    let string = CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String);
    assert_ne!(int, string, "distinct builtins must be distinct");
}

#[test]
fn canonical_identity_carries_no_local_ids_or_paths() {
    // The Debug output of any canonical identity must not leak donor-local IDs, interned paths,
    // string IDs or external type IDs. This is a structural invariant: the enum and its
    // supporting structs only carry owned stable values.
    let opaque = ExternalOpaqueTypeIdentity::new(
        "@core/io".to_owned(),
        ExternalSymbolPath::from_components(vec!["input".to_owned(), "Input".to_owned()]),
    );
    let identity = CanonicalTypeIdentity::ExternalOpaque(opaque);
    let debug = format!("{identity:?}");

    assert!(
        !debug.contains("ExternalTypeId") && !debug.contains("ExternalPackageId"),
        "canonical identity must not embed build-local external IDs: {debug}"
    );
    assert!(
        !debug.contains("NominalTypeId") && !debug.contains("TypeId("),
        "canonical identity must not embed donor-local TypeId or NominalTypeId: {debug}"
    );
    assert!(
        !debug.contains("InternedPath") && !debug.contains("StringId"),
        "canonical identity must not embed interned paths or string IDs: {debug}"
    );
}

#[test]
fn recursive_generic_instance_arguments_are_canonical() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table);
    let (box_nominal_id, _) =
        register_generic_struct(&mut env, &mut string_table, "Box", param_list_id);

    // Box<Option<Int>>
    let int_id = env.builtins().int;
    let option_id = env.intern_option(int_id);
    let nested_instance_id = env.intern_generic_instance(box_nominal_id, Box::new([option_id]));

    let mut resolver = MapNominalOriginResolver::new();
    resolver.register(box_nominal_id, struct_origin("Box"));
    let registry = ExternalPackageRegistry::new();
    let context = projection_context(&resolver, &registry);

    let identity = project_type_id_to_canonical_identity(nested_instance_id, &env, &context)
        .expect("nested generic instance projection should succeed");

    let expected = CanonicalTypeIdentity::GenericInstance(GenericInstanceTypeIdentity::new(
        struct_origin("Box"),
        Box::new([CanonicalTypeIdentity::Option(Box::new(
            CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
        ))]),
    ));
    assert_eq!(
        identity, expected,
        "Box<Option<Int>> must recursively project inner option to canonical identity"
    );
}
