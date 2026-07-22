//! Focused unit tests for the defined public type-only surface projection.
//!
//! WHAT: exercises the structural invariants of `DefinedPublicTypeSurface` values and the
//! `build_defined_public_type_surface` projection that integration output cannot inspect:
//! canonical type projection for every root category, deterministic ordering, receiver-method
//! attachment, generic-parameter origin resolution, and `CompilerError` for missing or
//! ambiguous ownership.
//! WHY: these are pure projection invariants owned by
//! `compiler_frontend::defined_public_type_surface`, so they own a focused test beside the
//! module rather than an end-to-end case.

use crate::compiler_frontend::ast::ResolvedTraitSourceFact;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::{
    ReceiverMethodEntry, ResolvedPublicTypeRoot, ResolvedPublicTypeRootKind,
    ResolvedPublicTypeRootTable,
};
use crate::compiler_frontend::builtins::casts::targets::{
    BuiltinCastFallibility, BuiltinCastTarget,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalBuiltinType, CanonicalCoreTraitIdentity, CanonicalTraitIdentity, CanonicalTypeIdentity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantDefinition, ChoiceVariantPayloadDefinition, FieldDefinition,
    StructTypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterListId, NominalTypeId, TypeId};
use crate::compiler_frontend::defined_public_type_surface::{
    DefinedPublicReceiverMethodTypeSurface, DefinedPublicTypeSurface,
    build_defined_public_type_surface,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::semantic_identity::{
    DefinedPublicExportOrigins, ExportBinding, ModuleRootRole, OriginConstantId,
    OriginDeclarationId, OriginFunctionId, OriginTraitId, OriginTypeCategory, OriginTypeId,
    ReceiverSurfaceOrigins, StableModuleOriginIdentity, StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::environment::CoreTraitKind;
use crate::compiler_frontend::traits::ids::TraitId;
use crate::compiler_frontend::value_mode::ValueMode;

use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
//  Test fixtures
// ---------------------------------------------------------------------------

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

fn alias_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(
        module_origin("aliases"),
        name.to_owned(),
        OriginTypeCategory::TransparentAlias,
    )
}

fn free_function_origin(name: &str) -> OriginFunctionId {
    OriginFunctionId::new_free(module_origin("functions"), name.to_owned())
}

fn constant_origin(name: &str) -> OriginConstantId {
    OriginConstantId::new(module_origin("constants"), name.to_owned())
}

fn empty_fields() -> Box<[FieldDefinition]> {
    Box::new([])
}

fn location() -> SourceLocation {
    SourceLocation::default()
}

fn path(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

fn param_declaration(name: &str, type_id: TypeId, string_table: &mut StringTable) -> Declaration {
    Declaration {
        id: path(name, string_table),
        value: Expression::no_value_with_type_id(
            location(),
            DataType::Inferred,
            type_id,
            ValueMode::default(),
        ),
    }
}

fn return_slot(type_id: TypeId, channel: ReturnChannel) -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::Value(DataType::Inferred),
        type_id: Some(type_id),
        reactive_template: None,
        channel,
    }
}

fn unresolved_return_slot(channel: ReturnChannel) -> ReturnSlot {
    ReturnSlot {
        value: FunctionReturn::Value(DataType::Inferred),
        type_id: None,
        reactive_template: None,
        channel,
    }
}

fn free_function_signature(
    parameters: Vec<Declaration>,
    return_type_ids: Vec<TypeId>,
) -> FunctionSignature {
    let returns = return_type_ids
        .into_iter()
        .map(|type_id| return_slot(type_id, ReturnChannel::Success))
        .collect();
    FunctionSignature {
        parameters,
        returns,
    }
}

fn function_root(
    name: &str,
    signature: FunctionSignature,
    generic_parameter_list_id: Option<GenericParameterListId>,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Function {
            signature,
            generic_parameter_list_id,
        },
    }
}

fn struct_root(
    name: &str,
    type_id: TypeId,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Struct { type_id },
    }
}

fn choice_root(
    name: &str,
    type_id: TypeId,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Choice { type_id },
    }
}

fn alias_root(
    name: &str,
    target_type_id: TypeId,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::TransparentAlias { target_type_id },
    }
}

fn constant_root(
    name: &str,
    type_id: TypeId,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Constant { type_id },
    }
}

fn receiver_entry(
    function_path: InternedPath,
    receiver: ReceiverKey,
    signature: FunctionSignature,
) -> ReceiverMethodEntry {
    ReceiverMethodEntry {
        function_path,
        receiver,
        source_file: InternedPath::new(),
        receiver_mutable: false,
        signature,
    }
}

fn register_struct(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    name: &str,
    fields: Box<[FieldDefinition]>,
    generic_parameters: Option<GenericParameterListId>,
) -> (NominalTypeId, TypeId) {
    let path = InternedPath::from_single_str(name, string_table);
    env.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path,
        fields,
        generic_parameters,
        const_record: false,
    })
}

fn register_choice(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    name: &str,
    variants: Box<[ChoiceVariantDefinition]>,
    generic_parameters: Option<GenericParameterListId>,
) -> (NominalTypeId, TypeId) {
    let path = InternedPath::from_single_str(name, string_table);
    env.register_nominal_choice(ChoiceTypeDefinition {
        id: NominalTypeId(0),
        path,
        variants,
        generic_parameters,
    })
}

fn field_def(name: &str, type_id: TypeId, string_table: &mut StringTable) -> FieldDefinition {
    FieldDefinition {
        name: path(name, string_table),
        type_id,
        location: location(),
    }
}

fn unit_variant(name: &str, string_table: &mut StringTable) -> ChoiceVariantDefinition {
    ChoiceVariantDefinition {
        name: string_table.intern(name),
        tag: 0,
        payload: ChoiceVariantPayloadDefinition::Unit,
        location: location(),
    }
}

fn record_variant(
    name: &str,
    fields: Box<[FieldDefinition]>,
    string_table: &mut StringTable,
) -> ChoiceVariantDefinition {
    ChoiceVariantDefinition {
        name: string_table.intern(name),
        tag: 0,
        payload: ChoiceVariantPayloadDefinition::Record { fields },
        location: location(),
    }
}

fn register_single_param_list(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    param_name: &str,
) -> GenericParameterListId {
    register_param_list(env, string_table, &[param_name])
}

/// Register a generic parameter list with one or more parameters in the given order.
///
/// Each parameter name becomes one declaration-local entry, so tests can assert
/// declaration-local ordering on the projected surface.
fn register_param_list(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    param_names: &[&str],
) -> GenericParameterListId {
    use crate::compiler_frontend::datatypes::generic_parameters::{
        GenericParameter, GenericParameterList, TypeParameterId,
    };
    let parameters = param_names
        .iter()
        .enumerate()
        .map(|(position, name)| GenericParameter {
            id: TypeParameterId(position as u32),
            name: string_table.intern(name),
            location: location(),
            trait_bounds: Vec::new(),
        })
        .collect();
    let list = GenericParameterList { parameters };
    env.register_generic_parameter_list(&list, &FxHashMap::default())
        .list_id
}

fn export_binding(name: &str, origin: OriginDeclarationId) -> ExportBinding {
    ExportBinding::new(module_origin("functions"), name.to_owned(), origin)
}

fn build_origins(
    export_bindings: Vec<ExportBinding>,
    receiver_surfaces: Vec<ReceiverSurfaceOrigins>,
) -> DefinedPublicExportOrigins {
    DefinedPublicExportOrigins::new(
        module_origin("functions"),
        export_bindings,
        receiver_surfaces,
    )
}

fn build_surface(
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
    public_nominal_type_origins: &FxHashMap<InternedPath, OriginTypeId>,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<DefinedPublicTypeSurface, CompilerError> {
    let registry = ExternalPackageRegistry::new();
    build_defined_public_type_surface(
        root_table,
        export_origins,
        public_nominal_type_origins,
        &FxHashMap::default(),
        type_environment,
        &registry,
        string_table,
    )
}

fn build_surface_with_traits(
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
    public_nominal_type_origins: &FxHashMap<InternedPath, OriginTypeId>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<DefinedPublicTypeSurface, CompilerError> {
    let registry = ExternalPackageRegistry::new();
    build_defined_public_type_surface(
        root_table,
        export_origins,
        public_nominal_type_origins,
        public_source_trait_origins,
        type_environment,
        &registry,
        string_table,
    )
}

fn nominal_origins_map(
    entries: Vec<(&str, OriginTypeId)>,
    string_table: &mut StringTable,
) -> FxHashMap<InternedPath, OriginTypeId> {
    let mut map = FxHashMap::default();
    for (name, origin) in entries {
        map.insert(path(name, string_table), origin);
    }
    map
}

/// Register a single generic parameter with the given resolved trait bound TraitIds.
///
/// The parser-level `GenericParameter.trait_bounds` is left empty; the resolved `TraitId`
/// bounds are supplied through `resolved_bounds_by_local`, matching how the real AST
/// environment builder registers generic parameter lists.
fn register_param_list_with_bounds(
    env: &mut TypeEnvironment,
    string_table: &mut StringTable,
    param_name: &str,
    bound_trait_ids: Vec<TraitId>,
) -> GenericParameterListId {
    use crate::compiler_frontend::datatypes::generic_parameters::{
        GenericParameter, GenericParameterList, TypeParameterId,
    };
    let parameters = vec![GenericParameter {
        id: TypeParameterId(0),
        name: string_table.intern(param_name),
        location: location(),
        trait_bounds: Vec::new(),
    }];
    let list = GenericParameterList { parameters };
    let mut bounds_by_local: FxHashMap<TypeParameterId, Vec<TraitId>> = FxHashMap::default();
    bounds_by_local.insert(TypeParameterId(0), bound_trait_ids);
    env.register_generic_parameter_list(&list, &bounds_by_local)
        .list_id
}

// ---------------------------------------------------------------------------
//  Free function projection
// ---------------------------------------------------------------------------

#[test]
fn projects_free_function_parameter_and_return_types() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let param = param_declaration("value", int_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![bool_id]);
    let root = function_root("render", signature, None, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "render",
        OriginDeclarationId::Function(free_function_origin("render")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("free function projection should succeed");

    assert_eq!(surface.free_functions.len(), 1);
    let function = &surface.free_functions[0];
    assert_eq!(&function.origin, &free_function_origin("render"));
    assert_eq!(function.parameters.len(), 1);
    assert_eq!(
        function.parameters[0].name.as_deref(),
        Some("value"),
        "parameter name must be preserved as an owned string"
    );
    assert_eq!(
        &function.parameters[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
    assert_eq!(function.returns.len(), 1);
    assert_eq!(
        &function.returns[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Bool)
    );
    assert!(function.error_return.as_ref().is_none());
}

#[test]
fn projects_free_function_with_error_return() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let string_id = env.builtins().string;

    let param = param_declaration("value", int_id, &mut string_table);
    let mut signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![
            return_slot(string_id, ReturnChannel::Success),
            return_slot(int_id, ReturnChannel::Error),
        ],
    };
    // Remove the default returns and add custom ones
    signature.returns.clear();
    signature
        .returns
        .push(return_slot(string_id, ReturnChannel::Success));
    signature
        .returns
        .push(return_slot(int_id, ReturnChannel::Error));

    let root = function_root("parse", signature, None, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "parse",
        OriginDeclarationId::Function(free_function_origin("parse")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("error-return projection should succeed");

    let function = &surface.free_functions[0];
    assert_eq!(function.returns.len(), 1);
    assert_eq!(
        &function.returns[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::String)
    );
    assert_eq!(
        function.error_return.as_ref(),
        Some(&CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)),
        "error return type must be projected separately"
    );
}

// ---------------------------------------------------------------------------
//  Struct projection
// ---------------------------------------------------------------------------

#[test]
fn projects_struct_fields_to_canonical_field_types() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let fields = Box::new([
        field_def("x", int_id, &mut string_table),
        field_def("flag", bool_id, &mut string_table),
    ]);
    let (_nominal_id, type_id) =
        register_struct(&mut env, &mut string_table, "Point", fields, None);

    let root = struct_root("Point", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Point", OriginDeclarationId::Type(struct_origin("Point")));
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map =
        nominal_origins_map(vec![("Point", struct_origin("Point"))], &mut string_table);

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("struct projection should succeed");

    assert_eq!(surface.nominal_types.len(), 1);
    let nominal = &surface.nominal_types[0];
    assert_eq!(&nominal.origin, &struct_origin("Point"));
    assert_eq!(nominal.fields.len(), 2);
    assert_eq!(nominal.fields[0].name.as_str(), "x");
    assert_eq!(
        &nominal.fields[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
    assert_eq!(nominal.fields[1].name.as_str(), "flag");
    assert_eq!(
        &nominal.fields[1].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Bool)
    );
    assert!(
        nominal.variants.is_empty(),
        "struct surface has no variants"
    );
}

// ---------------------------------------------------------------------------
//  Choice projection
// ---------------------------------------------------------------------------

#[test]
fn projects_choice_variants_and_payload_fields() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let unit = unit_variant("none", &mut string_table);
    let record_fields = Box::new([field_def("count", int_id, &mut string_table)]);
    let some = record_variant("some", record_fields, &mut string_table);

    let (_nominal_id, type_id) = register_choice(
        &mut env,
        &mut string_table,
        "Counter",
        Box::new([unit, some]),
        None,
    );

    let root = choice_root("Counter", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "Counter",
        OriginDeclarationId::Type(choice_origin("Counter")),
    );
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map = nominal_origins_map(
        vec![("Counter", choice_origin("Counter"))],
        &mut string_table,
    );

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("choice projection should succeed");

    assert_eq!(surface.nominal_types.len(), 1);
    let nominal = &surface.nominal_types[0];
    assert_eq!(&nominal.origin, &choice_origin("Counter"));
    assert!(
        nominal.fields.is_empty(),
        "choice surface has no struct fields"
    );
    assert_eq!(nominal.variants.len(), 2);
    assert_eq!(nominal.variants[0].name.as_str(), "none");
    assert!(
        nominal.variants[0].payload_fields.is_empty(),
        "unit variant has no payload"
    );
    assert_eq!(nominal.variants[1].name.as_str(), "some");
    assert_eq!(nominal.variants[1].payload_fields.len(), 1);
    assert_eq!(nominal.variants[1].payload_fields[0].name.as_str(), "count");
    assert_eq!(
        &nominal.variants[1].payload_fields[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
}

// ---------------------------------------------------------------------------
//  Transparent alias projection
// ---------------------------------------------------------------------------

#[test]
fn projects_transparent_alias_target_once() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let root = alias_root("Count", int_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Count", OriginDeclarationId::Type(alias_origin("Count")));
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("alias projection should succeed");

    assert_eq!(surface.transparent_aliases.len(), 1);
    let alias = &surface.transparent_aliases[0];
    assert_eq!(&alias.origin, &alias_origin("Count"));
    assert_eq!(
        &alias.target_type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
}

// ---------------------------------------------------------------------------
//  Constant projection
// ---------------------------------------------------------------------------

#[test]
fn projects_constant_type_only() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let root = constant_root("MaxRetries", int_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "MaxRetries",
        OriginDeclarationId::Constant(constant_origin("MaxRetries")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("constant projection should succeed");

    assert_eq!(surface.constants.len(), 1);
    let constant = &surface.constants[0];
    assert_eq!(&constant.origin, &constant_origin("MaxRetries"));
    assert_eq!(
        &constant.type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
}

// ---------------------------------------------------------------------------
//  Nested constructed types
// ---------------------------------------------------------------------------

#[test]
fn projects_nested_collection_and_option_types() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let collection_id = env.intern_collection(int_id, None);
    let option_id = env.intern_option(collection_id);

    let param = param_declaration("items", option_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![collection_id]);
    let root = function_root("collect", signature, None, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "collect",
        OriginDeclarationId::Function(free_function_origin("collect")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("nested constructed type projection should succeed");

    let function = &surface.free_functions[0];
    assert_eq!(
        &function.parameters[0].type_identity,
        &CanonicalTypeIdentity::Option(Box::new(CanonicalTypeIdentity::Collection(
            crate::compiler_frontend::canonical_type_identity::CollectionTypeIdentity::new(
                CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
                None,
            )
        ))),
        "nested option(collection(int)) must project recursively"
    );
    assert_eq!(
        &function.returns[0].type_identity,
        &CanonicalTypeIdentity::Collection(
            crate::compiler_frontend::canonical_type_identity::CollectionTypeIdentity::new(
                CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
                None,
            )
        )
    );
}

// ---------------------------------------------------------------------------
//  Generic function projection
// ---------------------------------------------------------------------------

#[test]
fn projects_open_exported_generic_function_parameter() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("generic parameter must have a TypeId");

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root(
        "identity",
        signature,
        Some(param_list_id),
        &mut string_table,
    );

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "identity",
        OriginDeclarationId::Function(free_function_origin("identity")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("generic function projection should succeed");

    let function = &surface.free_functions[0];
    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::free_function(
            free_function_origin("identity"),
        )
        .expect("free function must be a valid generic declaration owner");
    let expected_identity =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            0,
            "T".to_owned(),
        );
    assert_eq!(
        &function.parameters[0].type_identity,
        &CanonicalTypeIdentity::GenericParameter(expected_identity.clone()),
        "open exported generic parameter must project to its stable identity"
    );
    assert_eq!(
        function
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        &[&expected_identity],
        "the generic free function must expose its single ordered exported generic parameter identity"
    );
    assert_eq!(
        function.generic_parameters[0].identity.declaration_origin(),
        &expected_origin,
        "the exported generic parameter must name the function's stable origin"
    );
}

// ---------------------------------------------------------------------------
//  Generic nominal field projection
// ---------------------------------------------------------------------------

#[test]
fn projects_generic_struct_field_with_open_generic_parameter() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("generic parameter must have a TypeId");

    let fields = Box::new([field_def("value", generic_type_id, &mut string_table)]);
    let (_nominal_id, type_id) = register_struct(
        &mut env,
        &mut string_table,
        "Box",
        fields,
        Some(param_list_id),
    );

    let root = struct_root("Box", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Box", OriginDeclarationId::Type(struct_origin("Box")));
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map = nominal_origins_map(vec![("Box", struct_origin("Box"))], &mut string_table);

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("generic struct projection should succeed");

    let nominal = &surface.nominal_types[0];
    assert_eq!(nominal.fields.len(), 1);
    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::nominal_type(
            struct_origin("Box"),
        )
        .expect("struct origin must be a valid generic declaration owner");
    let expected_identity =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            0,
            "T".to_owned(),
        );
    assert_eq!(
        &nominal.fields[0].type_identity,
        &CanonicalTypeIdentity::GenericParameter(expected_identity.clone()),
        "generic struct field must project the open parameter to its nominal-owned identity"
    );
    assert_eq!(
        nominal
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        &[&expected_identity],
        "the generic struct must expose its single ordered exported generic parameter identity"
    );
    assert_eq!(
        nominal.generic_parameters[0].identity.declaration_origin(),
        &expected_origin,
        "the exported generic parameter must name the struct's stable origin"
    );
}

// ---------------------------------------------------------------------------
//  Ordered exported generic parameter identities
// ---------------------------------------------------------------------------

#[test]
fn non_generic_free_function_exposes_empty_generic_parameter_list() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let param = param_declaration("value", int_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![int_id]);
    let root = function_root("plain", signature, None, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "plain",
        OriginDeclarationId::Function(free_function_origin("plain")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("non-generic free function projection should succeed");

    assert!(
        surface.free_functions[0].generic_parameters.is_empty(),
        "a non-generic free function must expose an empty generic parameter list"
    );
}

#[test]
fn non_generic_struct_exposes_empty_generic_parameter_list() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let fields = Box::new([field_def("count", int_id, &mut string_table)]);
    let (_nominal_id, type_id) =
        register_struct(&mut env, &mut string_table, "Counter", fields, None);

    let root = struct_root("Counter", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "Counter",
        OriginDeclarationId::Type(struct_origin("Counter")),
    );
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("non-generic struct projection should succeed");

    assert!(
        surface.nominal_types[0].generic_parameters.is_empty(),
        "a non-generic struct must expose an empty generic parameter list"
    );
}

#[test]
fn generic_free_function_exposes_ordered_generic_parameter_identities() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_param_list(&mut env, &mut string_table, &["Key", "Value"]);

    let key_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("first generic parameter must have a TypeId");
    let value_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[1].id,
        )
        .expect("second generic parameter must have a TypeId");

    let key_param = param_declaration("key", key_type_id, &mut string_table);
    let value_param = param_declaration("value", value_type_id, &mut string_table);
    let signature = free_function_signature(vec![key_param, value_param], vec![value_type_id]);
    let root = function_root("pair", signature, Some(param_list_id), &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "pair",
        OriginDeclarationId::Function(free_function_origin("pair")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("generic function projection should succeed");

    let function = &surface.free_functions[0];
    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::free_function(
            free_function_origin("pair"),
        )
        .expect("free function must be a valid generic declaration owner");
    let expected_first =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            0,
            "Key".to_owned(),
        );
    let expected_second =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            1,
            "Value".to_owned(),
        );

    assert_eq!(
        function
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        &[&expected_first, &expected_second],
        "the generic free function must expose its parameters in declaration-local order"
    );
}

#[test]
fn generic_choice_exposes_ordered_generic_parameter_identities() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_param_list(&mut env, &mut string_table, &["T", "U"]);

    let first_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("first generic parameter must have a TypeId");
    let second_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[1].id,
        )
        .expect("second generic parameter must have a TypeId");

    let variant_fields = Box::new([
        field_def("first", first_type_id, &mut string_table),
        field_def("second", second_type_id, &mut string_table),
    ]);
    let variant = record_variant("Pair", variant_fields, &mut string_table);
    let (_nominal_id, type_id) = register_choice(
        &mut env,
        &mut string_table,
        "Result",
        Box::new([variant]),
        Some(param_list_id),
    );

    let root = choice_root("Result", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Result", OriginDeclarationId::Type(choice_origin("Result")));
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map =
        nominal_origins_map(vec![("Result", choice_origin("Result"))], &mut string_table);

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("generic choice projection should succeed");

    let nominal = &surface.nominal_types[0];
    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::nominal_type(
            choice_origin("Result"),
        )
        .expect("choice origin must be a valid generic declaration owner");
    let expected_first =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            0,
            "T".to_owned(),
        );
    let expected_second =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            1,
            "U".to_owned(),
        );

    assert_eq!(
        nominal
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        &[&expected_first, &expected_second],
        "the generic choice must expose its parameters in declaration-local order"
    );
}

#[test]
fn generic_parameter_identities_are_stable_across_donor_local_allocation() {
    // Two independent TypeEnvironments register the same single-parameter generic list, but one
    // environment first registers a throwaway generic parameter list through the ordinary
    // registration owner so its target parameter is allocated from a higher donor-local counter.
    // The donor-local GenericParameterId allocations must differ, yet the projected
    // ExportedGenericParameterIdentity must be identical because it derives from the stable
    // declaration origin and declaration-local position, not the donor-local id.
    let function_name = "identity";
    let make_surface = |perturb: bool| {
        let mut env = TypeEnvironment::new();
        let mut string_table = StringTable::new();
        if perturb {
            // Push the donor-local allocation counter ahead through the ordinary owner before
            // registering the target list, so the target parameter gets a different
            // GenericParameterId without constructing or mutating private IDs directly.
            let _ = register_single_param_list(&mut env, &mut string_table, "Perturb");
        }
        let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
        let target_local_id = env
            .generic_parameters(param_list_id)
            .expect("target generic parameter list must resolve")
            .parameters[0]
            .id;
        let generic_type_id = env
            .type_id_for_generic_parameter(target_local_id)
            .expect("generic parameter must have a TypeId");

        let param = param_declaration("value", generic_type_id, &mut string_table);
        let signature = free_function_signature(vec![param], vec![generic_type_id]);
        let root = function_root(
            function_name,
            signature,
            Some(param_list_id),
            &mut string_table,
        );

        let root_table = ResolvedPublicTypeRootTable {
            roots: vec![root],
            receiver_methods: vec![],
            trait_source_facts: FxHashMap::default(),
        };

        let binding = export_binding(
            function_name,
            OriginDeclarationId::Function(free_function_origin(function_name)),
        );
        let origins = build_origins(vec![binding], vec![]);

        let surface = build_surface(
            &root_table,
            &origins,
            &FxHashMap::default(),
            &env,
            &string_table,
        )
        .expect("projection should succeed");
        (surface, target_local_id)
    };

    let (surface_a, local_id_a) = make_surface(false);
    let (surface_b, local_id_b) = make_surface(true);

    assert_ne!(
        local_id_a, local_id_b,
        "the two environments must allocate different donor-local GenericParameterIds for the target parameter so the stability premise is real"
    );
    assert_eq!(
        surface_a.free_functions[0]
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        surface_b.free_functions[0]
            .generic_parameters
            .iter()
            .map(|s| &s.identity)
            .collect::<Vec<_>>(),
        "exported generic parameter identities must be stable across donor-local GenericParameterId allocation"
    );
}

// ---------------------------------------------------------------------------
//  Wrong-owner and duplicate exported generic parameter identities
// ---------------------------------------------------------------------------

/// A test-only `GenericParameterOriginResolver` that returns a fixed identity for every
/// `GenericParameterId`, so the projection helper's owner and duplicate invariants can be
/// exercised without constructing a full root table.
struct FixedGenericParameterOriginResolver {
    identity: crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity,
}

impl crate::compiler_frontend::canonical_type_identity::GenericParameterOriginResolver
    for FixedGenericParameterOriginResolver
{
    fn resolve_generic_parameter_origin(
        &self,
        _parameter_id: crate::compiler_frontend::datatypes::ids::GenericParameterId,
    ) -> Result<
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity,
        crate::compiler_frontend::compiler_errors::CompilerError,
    > {
        Ok(self.identity.clone())
    }
}

#[test]
fn wrong_owner_generic_parameter_identity_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");

    let wrong_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::free_function(
            free_function_origin("other"),
        )
        .expect("wrong-owner origin must be constructible");
    let wrong_identity =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            wrong_origin,
            0,
            "T".to_owned(),
        );

    let resolver = FixedGenericParameterOriginResolver {
        identity: wrong_identity,
    };

    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::free_function(
            free_function_origin("identity"),
        )
        .expect("expected origin must be constructible");

    let result = super::project_exported_generic_parameter_surfaces(
        Some(param_list_id),
        &env,
        &resolver,
        &expected_origin,
        &FxHashMap::default(),
        &FxHashMap::default(),
    );

    assert!(
        result.is_err(),
        "an exported generic parameter whose declaration origin does not match the root owner must be a CompilerError, not silently admitted"
    );
}

#[test]
fn duplicate_resolved_generic_parameter_identity_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_param_list(&mut env, &mut string_table, &["T", "T"]);

    let expected_origin =
        crate::compiler_frontend::canonical_type_identity::GenericDeclarationOrigin::free_function(
            free_function_origin("identity"),
        )
        .expect("expected origin must be constructible");

    // Both parameters resolve to the same identity (same owner, position 0, name "T"), so the
    // second entry is a duplicate.
    let duplicate_identity =
        crate::compiler_frontend::canonical_type_identity::ExportedGenericParameterIdentity::new(
            expected_origin.clone(),
            0,
            "T".to_owned(),
        );

    let resolver = FixedGenericParameterOriginResolver {
        identity: duplicate_identity,
    };

    let result = super::project_exported_generic_parameter_surfaces(
        Some(param_list_id),
        &env,
        &resolver,
        &expected_origin,
        &FxHashMap::default(),
        &FxHashMap::default(),
    );

    assert!(
        result.is_err(),
        "two exported generic parameters resolving to the same identity must be a CompilerError"
    );
}

// ---------------------------------------------------------------------------
//  Receiver method projection
// ---------------------------------------------------------------------------

#[test]
fn projects_receiver_method_attached_to_public_receiver() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let (_nominal_id, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);

    let receiver_path = path("Counter", &mut string_table);
    let method_fn_path = path("render", &mut string_table);

    let param = param_declaration("delta", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![return_slot(int_id, ReturnChannel::Success)],
    };

    let entry = receiver_entry(
        method_fn_path,
        ReceiverKey::Struct(receiver_path.clone()),
        signature,
    );

    let root = struct_root("Counter", struct_type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![entry],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "Counter",
        OriginDeclarationId::Type(struct_origin("Counter")),
    );

    let method_origin = OriginFunctionId::new_receiver(
        module_origin("functions"),
        "render".to_owned(),
        struct_origin("Counter"),
    );
    let receiver_surface =
        ReceiverSurfaceOrigins::new(struct_origin("Counter"), vec![method_origin]);

    let origins = build_origins(vec![binding], vec![receiver_surface]);
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("receiver method projection should succeed");

    // The struct is in the nominal types, and the receiver method is in receiver_methods.
    assert_eq!(surface.nominal_types.len(), 1);
    assert_eq!(surface.receiver_methods.len(), 1);
    let method = &surface.receiver_methods[0];
    assert_eq!(&method.receiver_origin, &struct_origin("Counter"));
    assert_eq!(method.parameters.len(), 1);
    assert_eq!(method.parameters[0].name.as_deref(), Some("delta"));
    assert_eq!(
        &method.returns[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
}

// ---------------------------------------------------------------------------
//  Error cases
// ---------------------------------------------------------------------------

#[test]
fn missing_nominal_origin_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let fields = Box::new([field_def("x", int_id, &mut string_table)]);
    let (_nominal_id, type_id) =
        register_struct(&mut env, &mut string_table, "Point", fields, None);

    let root = struct_root("Point", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Point", OriginDeclarationId::Type(struct_origin("Point")));
    let origins = build_origins(vec![binding], vec![]);

    // Empty nominal map: the struct is not a registered public nominal origin.
    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "a struct whose nominal path is not in the public nominal-type origin index must fail"
    );
}

#[test]
fn missing_signature_slot_type_id_is_compiler_error() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let param = param_declaration("value", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![unresolved_return_slot(ReturnChannel::Success)],
    };
    let root = function_root("unresolved_return", signature, None, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "unresolved_return",
        OriginDeclarationId::Function(free_function_origin("unresolved_return")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "a return slot with no TypeId must be a CompilerError, not silently omitted"
    );
}

#[test]
fn category_mismatch_between_root_and_binding_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let _int_id = env.builtins().int;

    let (_nominal_id, type_id) =
        register_struct(&mut env, &mut string_table, "Widget", empty_fields(), None);

    let root = struct_root("Widget", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    // The root is a struct but the binding origin says it is a constant.
    let binding = export_binding(
        "Widget",
        OriginDeclarationId::Constant(constant_origin("Widget")),
    );
    let origins = build_origins(vec![binding], vec![]);
    let nominal_map =
        nominal_origins_map(vec![("Widget", struct_origin("Widget"))], &mut string_table);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a struct root matched to a constant binding must fail"
    );
}

#[test]
fn unregistered_generic_parameter_in_signature_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("generic parameter must have a TypeId");

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);

    // Create the root WITHOUT the generic_parameter_list_id, so the resolver won't register it.
    let root = function_root("missing_generic", signature, None, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "missing_generic",
        OriginDeclarationId::Function(free_function_origin("missing_generic")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "a generic parameter whose owner was not registered must be a CompilerError"
    );
}

// ---------------------------------------------------------------------------
//  Deterministic ordering
// ---------------------------------------------------------------------------

#[test]
fn top_level_entries_are_ordered_by_binding_order() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    // Create two functions. Insert roots in reverse order, but bindings in sorted order.
    let param1 = param_declaration("a", int_id, &mut string_table);
    let sig1 = free_function_signature(vec![param1], vec![bool_id]);
    let root1 = function_root("alpha", sig1, None, &mut string_table);

    let param2 = param_declaration("b", int_id, &mut string_table);
    let sig2 = free_function_signature(vec![param2], vec![int_id]);
    let root2 = function_root("beta", sig2, None, &mut string_table);

    // Roots in reverse order (beta first, alpha second).
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root2, root1],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    // Bindings in sorted order (alpha first, beta second).
    let bindings = vec![
        export_binding(
            "alpha",
            OriginDeclarationId::Function(free_function_origin("alpha")),
        ),
        export_binding(
            "beta",
            OriginDeclarationId::Function(free_function_origin("beta")),
        ),
    ];
    let origins = build_origins(bindings, vec![]);

    let surface = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("ordering test should succeed");

    // The output must follow the binding order (alpha, beta), not the root insertion order.
    assert_eq!(
        &surface.free_functions[0].origin,
        &free_function_origin("alpha")
    );
    assert_eq!(
        &surface.free_functions[1].origin,
        &free_function_origin("beta")
    );
}

// ---------------------------------------------------------------------------
//  Output contains only canonical identities and owned names
// ---------------------------------------------------------------------------

#[test]
fn output_types_recursively_contain_only_canonical_identities() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    // Register a public struct with an int field.
    let fields = Box::new([field_def("count", int_id, &mut string_table)]);
    let (_nominal_id, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Widget", fields, None);

    // Register a public function that takes the struct as a parameter.
    let param = param_declaration("widget", struct_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![int_id]);
    let root_fn = function_root("use_widget", signature, None, &mut string_table);
    let root_struct = struct_root("Widget", struct_type_id, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root_fn, root_struct],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    // Bindings sorted by name: "Widget" (type) then "use_widget" (function).
    let bindings = vec![
        export_binding("Widget", OriginDeclarationId::Type(struct_origin("Widget"))),
        export_binding(
            "use_widget",
            OriginDeclarationId::Function(free_function_origin("use_widget")),
        ),
    ];
    let origins = build_origins(bindings, vec![]);
    let nominal_map =
        nominal_origins_map(vec![("Widget", struct_origin("Widget"))], &mut string_table);

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("canonical identity test should succeed");

    // The struct field type is a canonical builtin.
    let nominal = &surface.nominal_types[0];
    assert_eq!(nominal.fields[0].name.as_str(), "count");
    assert_eq!(
        &nominal.fields[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );

    // The function parameter type is a canonical source nominal.
    let function = &surface.free_functions[0];
    assert_eq!(
        &function.parameters[0].type_identity,
        &CanonicalTypeIdentity::SourceNominal(struct_origin("Widget")),
        "function parameter referencing the public struct must project to a canonical nominal"
    );
    assert_eq!(
        &function.returns[0].type_identity,
        &CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
    );
}

// ---------------------------------------------------------------------------
//  Imported public nominal resolution (graph-derived provider origin)
// ---------------------------------------------------------------------------

/// A public nominal type origin owned by a different (imported) module, proving that an
/// imported nominal projects to its provider origin rather than the active module origin.
fn imported_struct_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(
        module_origin("imports"),
        name.to_owned(),
        OriginTypeCategory::Struct,
    )
}

/// A multi-component declaration path `module::name`, so two same-named nominals from different
/// modules carry distinct canonical paths that resolve to distinct stable origins.
fn module_path(module: &str, name: &str, string_table: &mut StringTable) -> InternedPath {
    let mut path = InternedPath::from_single_str(module, string_table);
    path.push_str(name, string_table);
    path
}

/// Register a public struct at an explicit canonical path (not a single-component name).
fn register_struct_at_path(
    env: &mut TypeEnvironment,
    path: InternedPath,
    fields: Box<[FieldDefinition]>,
    generic_parameters: Option<GenericParameterListId>,
) -> (NominalTypeId, TypeId) {
    env.register_nominal_struct(StructTypeDefinition {
        id: NominalTypeId(0),
        path,
        fields,
        generic_parameters,
        const_record: false,
    })
}

#[test]
fn projects_imported_public_nominal_reference_to_provider_origin() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    // An imported public struct "Imported" owned by a different module origin.
    let (_imported_nominal_id, imported_type_id) = register_struct(
        &mut env,
        &mut string_table,
        "Imported",
        empty_fields(),
        None,
    );

    // A directly-defined public struct "Widget" with a field of the imported type.
    let fields = Box::new([field_def("value", imported_type_id, &mut string_table)]);
    let (_widget_nominal_id, widget_type_id) =
        register_struct(&mut env, &mut string_table, "Widget", fields, None);

    let root = struct_root("Widget", widget_type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Widget", OriginDeclarationId::Type(struct_origin("Widget")));
    let origins = build_origins(vec![binding], vec![]);

    // The expanded nominal origin index carries both the active-root nominal (Widget) and the
    // imported project-graph nominal (Imported) with its provider module origin.
    let nominal_map = nominal_origins_map(
        vec![
            ("Widget", struct_origin("Widget")),
            ("Imported", imported_struct_origin("Imported")),
        ],
        &mut string_table,
    );

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("imported nominal projection should succeed");

    let nominal = &surface.nominal_types[0];
    assert_eq!(nominal.fields[0].name.as_str(), "value");
    assert_eq!(
        &nominal.fields[0].type_identity,
        &CanonicalTypeIdentity::SourceNominal(imported_struct_origin("Imported")),
        "a directly-defined public field referencing an imported public nominal must project \
         to SourceNominal(provider_origin), not the active module origin"
    );
}

#[test]
fn imported_nominal_required_but_absent_from_index_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    // An imported nominal referenced by a public field, deliberately absent from the index
    // (the None / source-package case: no project-module owner).
    let (_imported_nominal_id, imported_type_id) = register_struct(
        &mut env,
        &mut string_table,
        "Imported",
        empty_fields(),
        None,
    );

    let fields = Box::new([field_def("value", imported_type_id, &mut string_table)]);
    let (_widget_nominal_id, widget_type_id) =
        register_struct(&mut env, &mut string_table, "Widget", fields, None);

    let root = struct_root("Widget", widget_type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Widget", OriginDeclarationId::Type(struct_origin("Widget")));
    let origins = build_origins(vec![binding], vec![]);

    // The index carries only the active-root nominal; "Imported" is absent, so its required
    // nominal reference cannot resolve and must fail with a precise CompilerError rather than a
    // path/display identity fallback.
    let nominal_map =
        nominal_origins_map(vec![("Widget", struct_origin("Widget"))], &mut string_table);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a public field referencing an imported nominal absent from the source-nominal origin \
         index (None owner) must be a CompilerError"
    );
}

// ---------------------------------------------------------------------------
//  Total root-to-binding join (finding 2)
// ---------------------------------------------------------------------------

#[test]
fn non_trait_binding_without_matching_root_is_compiler_error() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let param = param_declaration("a", int_id, &mut string_table);
    let sig = free_function_signature(vec![param], vec![bool_id]);
    let root = function_root("alpha", sig, None, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    // A function binding "ghost" with no matching root must be a CompilerError, not a skip.
    let binding = export_binding(
        "ghost",
        OriginDeclarationId::Function(free_function_origin("ghost")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "a non-trait binding with no matching type root must be a CompilerError"
    );
}

#[test]
fn duplicate_root_public_name_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let (_nominal_id, type_id) =
        register_struct(&mut env, &mut string_table, "Foo", empty_fields(), None);

    let root_a = struct_root("Foo", type_id, &mut string_table);
    let root_b = struct_root("Foo", type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root_a, root_b],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding("Foo", OriginDeclarationId::Type(struct_origin("Foo")));
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "two roots sharing a public name must be a CompilerError, not a silent overwrite"
    );
}

#[test]
fn unmatched_extra_root_is_compiler_error() {
    let env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let param = param_declaration("a", int_id, &mut string_table);
    let sig = free_function_signature(vec![param], vec![bool_id]);
    let root_alpha = function_root("alpha", sig, None, &mut string_table);
    // An extra root with no matching binding.
    let root_extra = function_root(
        "extra",
        free_function_signature(vec![], vec![]),
        None,
        &mut string_table,
    );
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root_alpha, root_extra],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "alpha",
        OriginDeclarationId::Function(free_function_origin("alpha")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "a root with no matching binding must be a CompilerError, not silently dropped"
    );
}

// ---------------------------------------------------------------------------
//  Exact receiver-method origin join (finding 3)
// ---------------------------------------------------------------------------

#[test]
fn receiver_methods_join_by_exact_origin_not_rendered_name() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    // Two same-named nominals "Counter" in different modules, with distinct canonical paths.
    let shapes_path = module_path("shapes", "Counter", &mut string_table);
    let imports_path = module_path("imports", "Counter", &mut string_table);
    let (_shapes_nominal_id, _) =
        register_struct_at_path(&mut env, shapes_path.clone(), empty_fields(), None);
    let (_imports_nominal_id, _) =
        register_struct_at_path(&mut env, imports_path.clone(), empty_fields(), None);

    let shapes_origin = struct_origin("Counter");
    let imports_origin = imported_struct_origin("Counter");

    let nominal_map = FxHashMap::from_iter([
        (shapes_path.clone(), shapes_origin.clone()),
        (imports_path.clone(), imports_origin.clone()),
    ]);

    // A "tick" method on each receiver. Rendered names collide ("Counter::tick"), so only the
    // exact stable origin can join the right entry.
    let make_entry = |receiver_path: InternedPath, string_table: &mut StringTable| {
        let method_path = path("tick", string_table);
        let param = param_declaration("delta", int_id, string_table);
        let signature = FunctionSignature {
            parameters: vec![param],
            returns: vec![return_slot(int_id, ReturnChannel::Success)],
        };
        receiver_entry(method_path, ReceiverKey::Struct(receiver_path), signature)
    };
    let entry_shapes = make_entry(shapes_path.clone(), &mut string_table);
    let entry_imports = make_entry(imports_path.clone(), &mut string_table);

    let method_origin_shapes = OriginFunctionId::new_receiver(
        module_origin("shapes"),
        "tick".to_owned(),
        shapes_origin.clone(),
    );
    let method_origin_imports = OriginFunctionId::new_receiver(
        module_origin("imports"),
        "tick".to_owned(),
        imports_origin.clone(),
    );

    let surface_shapes =
        ReceiverSurfaceOrigins::new(shapes_origin.clone(), vec![method_origin_shapes]);
    let surface_imports =
        ReceiverSurfaceOrigins::new(imports_origin.clone(), vec![method_origin_imports]);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![entry_shapes, entry_imports],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![surface_shapes, surface_imports]);

    let surface = build_surface(&root_table, &origins, &nominal_map, &env, &string_table)
        .expect("exact-origin receiver join should succeed");

    assert_eq!(surface.receiver_methods.len(), 2);
    let by_receiver: FxHashMap<&OriginTypeId, &DefinedPublicReceiverMethodTypeSurface> = surface
        .receiver_methods
        .iter()
        .map(|method| (&method.receiver_origin, method))
        .collect();
    assert_eq!(
        by_receiver
            .get(&shapes_origin)
            .unwrap()
            .method_origin
            .defining_name(),
        "tick",
        "the shapes receiver method must join its own origin"
    );
    assert_eq!(
        by_receiver
            .get(&imports_origin)
            .unwrap()
            .method_origin
            .defining_name(),
        "tick",
        "the imports receiver method must join its own origin"
    );
}

#[test]
fn duplicate_receiver_method_entry_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let receiver_path = path("Counter", &mut string_table);
    let (_nominal_id, _) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let method_path = path("tick", &mut string_table);
    let param = param_declaration("delta", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![return_slot(int_id, ReturnChannel::Success)],
    };

    // Two entries with the same exact stable receiver origin and method name.
    let entry_a = receiver_entry(
        method_path.clone(),
        ReceiverKey::Struct(receiver_path.clone()),
        signature.clone(),
    );
    let entry_b = receiver_entry(method_path, ReceiverKey::Struct(receiver_path), signature);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![entry_a, entry_b],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![]);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "two receiver-method entries sharing the exact stable receiver origin and method name \
         must be a CompilerError, not a silent overwrite"
    );
}

#[test]
fn missing_receiver_method_entry_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let (_nominal_id, _) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    // A surface method with no matching resolved entry.
    let method_origin = OriginFunctionId::new_receiver(
        module_origin("functions"),
        "tick".to_owned(),
        struct_origin("Counter"),
    );
    let receiver_surface =
        ReceiverSurfaceOrigins::new(struct_origin("Counter"), vec![method_origin]);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![receiver_surface]);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a surface method with no resolved receiver entry must be a CompilerError"
    );
}

#[test]
fn extra_receiver_method_entry_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let receiver_path = path("Counter", &mut string_table);
    let (_nominal_id, _) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let method_path = path("tick", &mut string_table);
    let param = param_declaration("delta", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![return_slot(int_id, ReturnChannel::Success)],
    };
    let entry = receiver_entry(method_path, ReceiverKey::Struct(receiver_path), signature);

    // A resolved entry with no matching surface must be a CompilerError.
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![entry],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![]);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a resolved receiver entry with no matching surface must be a CompilerError"
    );
}

// ---------------------------------------------------------------------------
//  Receiver key / origin category mismatch
// ---------------------------------------------------------------------------

#[test]
fn struct_receiver_key_with_choice_origin_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let receiver_path = path("Counter", &mut string_table);
    // The nominal is registered as a struct so the TypeEnvironment can resolve its path, but the
    // public nominal origin index names it as a choice: a re-categorised declaration must not
    // silently join a struct receiver surface.
    let (_nominal_id, _) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);
    let nominal_map = nominal_origins_map(
        vec![("Counter", choice_origin("Counter"))],
        &mut string_table,
    );

    let method_path = path("tick", &mut string_table);
    let param = param_declaration("delta", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![return_slot(int_id, ReturnChannel::Success)],
    };
    let entry = receiver_entry(method_path, ReceiverKey::Struct(receiver_path), signature);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![entry],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![]);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a struct receiver key whose resolved nominal origin is a choice must be a \
         CompilerError rather than a silent coercion"
    );
}

#[test]
fn choice_receiver_key_with_struct_origin_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let int_id = env.builtins().int;

    let receiver_path = path("Counter", &mut string_table);
    // The nominal is registered as a choice so the TypeEnvironment can resolve its path, but the
    // public nominal origin index names it as a struct: the choice receiver key must disagree.
    let zero_variant = unit_variant("Zero", &mut string_table);
    let (_nominal_id, _) = register_choice(
        &mut env,
        &mut string_table,
        "Counter",
        Box::new([zero_variant]),
        None,
    );
    let nominal_map = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let method_path = path("tick", &mut string_table);
    let param = param_declaration("delta", int_id, &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![param],
        returns: vec![return_slot(int_id, ReturnChannel::Success)],
    };
    let entry = receiver_entry(method_path, ReceiverKey::Choice(receiver_path), signature);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![entry],
        trait_source_facts: FxHashMap::default(),
    };
    let origins = build_origins(vec![], vec![]);

    let result = build_surface(&root_table, &origins, &nominal_map, &env, &string_table);
    assert!(
        result.is_err(),
        "a choice receiver key whose resolved nominal origin is a struct must be a \
         CompilerError rather than a silent coercion"
    );
}

// ---------------------------------------------------------------------------
//  Ambiguous generic-parameter owner (finding 4)
// ---------------------------------------------------------------------------

#[test]
fn ambiguous_generic_parameter_owner_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .expect("generic parameter must have a TypeId");

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);

    // Two function roots share the SAME generic parameter list id, so the same
    // GenericParameterId would be registered under two distinct declaration origins.
    let root_alpha = function_root(
        "alpha",
        signature.clone(),
        Some(param_list_id),
        &mut string_table,
    );
    let root_beta = function_root("beta", signature, Some(param_list_id), &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root_alpha, root_beta],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let bindings = vec![
        export_binding(
            "alpha",
            OriginDeclarationId::Function(free_function_origin("alpha")),
        ),
        export_binding(
            "beta",
            OriginDeclarationId::Function(free_function_origin("beta")),
        ),
    ];
    let origins = build_origins(bindings, vec![]);

    let result = build_surface(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    );
    assert!(
        result.is_err(),
        "registering the same GenericParameterId under two distinct declaration origins must \
         be a CompilerError without overwriting the first identity"
    );
}

// ---------------------------------------------------------------------------
//  Generic parameter bound projection
// ---------------------------------------------------------------------------

fn trait_origin(name: &str) -> OriginTraitId {
    OriginTraitId::new(module_origin("traits"), name.to_owned())
}

#[test]
fn generic_parameter_with_no_bounds_projects_empty_bound_list() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();
    let param_list_id = register_single_param_list(&mut env, &mut string_table, "T");
    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root(
        "identity",
        signature,
        Some(param_list_id),
        &mut string_table,
    );

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "identity",
        OriginDeclarationId::Function(free_function_origin("identity")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("parameter with no bounds should project");

    assert!(
        surface.free_functions[0].generic_parameters[0]
            .bounds
            .is_empty(),
        "a parameter with no bounds must project an empty bound list"
    );
}

#[test]
fn generic_parameter_with_source_trait_bound_projects_canonical_source_identity() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let source_trait_id = TraitId(0);
    let param_list_id =
        register_param_list_with_bounds(&mut env, &mut string_table, "T", vec![source_trait_id]);

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("render", signature, Some(param_list_id), &mut string_table);

    let trait_path = path("RENDERABLE", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        source_trait_id,
        ResolvedTraitSourceFact::Source(trait_path.clone()),
    );

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(trait_path, trait_origin("RENDERABLE"));

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "render",
        OriginDeclarationId::Function(free_function_origin("render")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("source trait bound should project");

    assert_eq!(
        &surface.free_functions[0].generic_parameters[0].bounds,
        &[CanonicalTraitIdentity::Source(trait_origin("RENDERABLE"))],
        "a source trait bound must project to its canonical source identity"
    );
}

#[test]
fn generic_parameter_with_displayable_core_bound_projects_canonical_core_identity() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let displayable_trait_id = TraitId(0);
    let param_list_id = register_param_list_with_bounds(
        &mut env,
        &mut string_table,
        "T",
        vec![displayable_trait_id],
    );

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("display", signature, Some(param_list_id), &mut string_table);

    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        displayable_trait_id,
        ResolvedTraitSourceFact::Core(CoreTraitKind::Displayable),
    );

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "display",
        OriginDeclarationId::Function(free_function_origin("display")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("displayable core bound should project");

    assert_eq!(
        &surface.free_functions[0].generic_parameters[0].bounds,
        &[CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Displayable
        )],
        "a Displayable core bound must project to its canonical core identity"
    );
}

#[test]
fn generic_parameter_with_cast_core_bound_projects_canonical_cast_identity() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let cast_trait_id = TraitId(0);
    let param_list_id =
        register_param_list_with_bounds(&mut env, &mut string_table, "T", vec![cast_trait_id]);

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("convert", signature, Some(param_list_id), &mut string_table);

    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        cast_trait_id,
        ResolvedTraitSourceFact::Core(CoreTraitKind::Castable {
            target: BuiltinCastTarget::Int,
            fallibility: BuiltinCastFallibility::Fallible,
        }),
    );

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "convert",
        OriginDeclarationId::Function(free_function_origin("convert")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("cast core bound should project");

    assert_eq!(
        &surface.free_functions[0].generic_parameters[0].bounds,
        &[CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Castable {
                target: BuiltinCastTarget::Int,
                fallibility: BuiltinCastFallibility::Fallible,
            }
        )],
        "a fallible cast core bound must project to its canonical cast identity with target and fallibility"
    );
}

#[test]
fn multiple_bounds_preserve_declaration_order() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let source_trait_id = TraitId(0);
    let displayable_trait_id = TraitId(1);
    let cast_trait_id = TraitId(2);

    let param_list_id = register_param_list_with_bounds(
        &mut env,
        &mut string_table,
        "T",
        vec![source_trait_id, displayable_trait_id, cast_trait_id],
    );

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("multi", signature, Some(param_list_id), &mut string_table);

    let source_trait_path = path("RENDERABLE", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        source_trait_id,
        ResolvedTraitSourceFact::Source(source_trait_path.clone()),
    );
    trait_source_facts.insert(
        displayable_trait_id,
        ResolvedTraitSourceFact::Core(CoreTraitKind::Displayable),
    );
    trait_source_facts.insert(
        cast_trait_id,
        ResolvedTraitSourceFact::Core(CoreTraitKind::Castable {
            target: BuiltinCastTarget::String,
            fallibility: BuiltinCastFallibility::Infallible,
        }),
    );

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(source_trait_path, trait_origin("RENDERABLE"));

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "multi",
        OriginDeclarationId::Function(free_function_origin("multi")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("multiple bounds should project in order");

    assert_eq!(
        &surface.free_functions[0].generic_parameters[0].bounds,
        &[
            CanonicalTraitIdentity::Source(trait_origin("RENDERABLE")),
            CanonicalTraitIdentity::Core(CanonicalCoreTraitIdentity::Displayable),
            CanonicalTraitIdentity::Core(CanonicalCoreTraitIdentity::Castable {
                target: BuiltinCastTarget::String,
                fallibility: BuiltinCastFallibility::Infallible,
            }),
        ],
        "multiple bounds must be projected in declaration-site order"
    );
}

#[test]
fn missing_trait_source_fact_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let unknown_trait_id = TraitId(99);
    let param_list_id =
        register_param_list_with_bounds(&mut env, &mut string_table, "T", vec![unknown_trait_id]);

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("missing", signature, Some(param_list_id), &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = export_binding(
        "missing",
        OriginDeclarationId::Function(free_function_origin("missing")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    );

    assert!(
        result.is_err(),
        "a bound TraitId with no retained trait source fact must be a CompilerError"
    );
}

#[test]
fn missing_source_trait_origin_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let source_trait_id = TraitId(0);
    let param_list_id =
        register_param_list_with_bounds(&mut env, &mut string_table, "T", vec![source_trait_id]);

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("private", signature, Some(param_list_id), &mut string_table);

    let trait_path = path("PrivateTrait", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(source_trait_id, ResolvedTraitSourceFact::Source(trait_path));

    // No trait origin index entry for the path -> private/unexported trait

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "private",
        OriginDeclarationId::Function(free_function_origin("private")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    );

    assert!(
        result.is_err(),
        "a source trait bound whose path has no retained public source-trait origin must be a CompilerError"
    );
}

#[test]
fn duplicate_canonical_bound_identity_is_compiler_error() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let trait_a_id = TraitId(0);
    let trait_b_id = TraitId(1);
    let param_list_id = register_param_list_with_bounds(
        &mut env,
        &mut string_table,
        "T",
        vec![trait_a_id, trait_b_id],
    );

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("dup", signature, Some(param_list_id), &mut string_table);

    let trait_path = path("RENDERABLE", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        trait_a_id,
        ResolvedTraitSourceFact::Source(trait_path.clone()),
    );
    trait_source_facts.insert(
        trait_b_id,
        ResolvedTraitSourceFact::Source(trait_path.clone()),
    );

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(trait_path, trait_origin("RENDERABLE"));

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "dup",
        OriginDeclarationId::Function(free_function_origin("dup")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let result = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(
        result.is_err(),
        "two bounds resolving to the same canonical trait identity must be a CompilerError"
    );
}

#[test]
fn source_trait_bound_resolves_to_provider_module_origin_not_active_origin() {
    let mut env = TypeEnvironment::new();
    let mut string_table = StringTable::new();

    let source_trait_id = TraitId(0);
    let param_list_id =
        register_param_list_with_bounds(&mut env, &mut string_table, "T", vec![source_trait_id]);

    let generic_type_id = env
        .type_id_for_generic_parameter(
            env.generic_parameters(param_list_id).unwrap().parameters[0].id,
        )
        .unwrap();

    let param = param_declaration("value", generic_type_id, &mut string_table);
    let signature = free_function_signature(vec![param], vec![generic_type_id]);
    let root = function_root("render", signature, Some(param_list_id), &mut string_table);

    let trait_path = path("RENDERABLE", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(
        source_trait_id,
        ResolvedTraitSourceFact::Source(trait_path.clone()),
    );

    // The trait is defined by an imported provider module whose origin differs from the
    // active module that owns the generic function. The projection must resolve the bound
    // to the trait's provider module origin, never the active function module origin.
    let provider_origin = module_origin("provider");
    let active_origin = module_origin("functions");
    assert_ne!(
        provider_origin, active_origin,
        "the provider and active module origins must be distinct for this test to prove provider ownership"
    );
    let provider_trait_origin = OriginTraitId::new(provider_origin, "RENDERABLE".to_owned());

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(trait_path, provider_trait_origin.clone());

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let binding = export_binding(
        "render",
        OriginDeclarationId::Function(free_function_origin("render")),
    );
    let origins = build_origins(vec![binding], vec![]);

    let surface = build_surface_with_traits(
        &root_table,
        &origins,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("a source trait bound from a provider module should project");

    assert_eq!(
        &surface.free_functions[0].generic_parameters[0].bounds,
        &[CanonicalTraitIdentity::Source(provider_trait_origin)],
        "a source-bound trait must resolve to its provider module origin, not the active module origin"
    );
}
