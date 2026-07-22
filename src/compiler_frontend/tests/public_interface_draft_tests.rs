//! Focused unit tests for the public-interface draft aggregate and the corrected direct
//! trait-requirement projection.
//!
//! WHAT: exercises the structural invariants of [`PublicInterfaceDraft`] and the
//! `build_trait_surfaces` projection that integration output cannot inspect: ordered trait
//! requirements, immutable and mutable receivers, trait-local `SelfType` for direct
//! `this_type` occurrences, ordinary builtin and imported canonical nominal projection,
//! `ValueMode` and `ReturnChannel` retention, the trait receiver `this_type` invariant, and
//! totality failures for missing, duplicate, unmatched and wrong-origin inputs. Also proves
//! the draft builder carries exactly one aggregate draft with the three projection components.
//! WHY: these are pure projection and aggregate-boundary invariants owned by
//! `compiler_frontend::public_interface_draft`, so they own a focused test beside the module
//! rather than an end-to-end case.

use crate::compiler_frontend::ast::AstPublicInterfaceProjectionInput;
use crate::compiler_frontend::ast::ReceiverMethodCatalog;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::{FunctionSignature, ReturnChannel};
use crate::compiler_frontend::ast::{
    ReceiverMethodEntry, ResolvedPublicTraitRoot, ResolvedPublicTypeRoot,
    ResolvedPublicTypeRootKind, ResolvedPublicTypeRootTable, ResolvedTraitParameterFact,
    ResolvedTraitReceiverFact, ResolvedTraitRequirementFact, ResolvedTraitReturnFact,
    TraitReceiverAccessKind,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalBuiltinType, CanonicalTypeIdentity, CanonicalTypeProjectionContext,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantDefinition, FieldDefinition, StructTypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterListId, NominalTypeId, TypeId};
use crate::compiler_frontend::defined_public_export_origins::DefinedPublicExportOriginDraft;
use crate::compiler_frontend::defined_public_type_surface::{
    DefinedPublicAliasTypeSurface, DefinedPublicConstantTypeSurface,
    DefinedPublicFunctionTypeSurface, DefinedPublicNominalTypeSurface,
    DefinedPublicReceiverMethodTypeSurface, DefinedPublicTypeSurface, PublicChoiceVariantSurface,
    PublicFieldTypeSlot, TransientNominalOriginResolver,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::public_interface_draft::{
    DefinedPublicTraitSurface, FoldedValueGenericParameterResolver, FoldedValueJoinContext,
    PublicDeclarationRecord, PublicDeclarationSemantics, PublicInterfaceDraftBuilder,
    PublicInterfaceDraftBuilderInput, PublicTraitReceiverAccess, TraitSurfaceTypeIdentity,
    build_trait_surfaces, join_declaration_records,
};
use crate::compiler_frontend::semantic_identity::{
    ExportBinding, ModuleRootRole, OriginConstantId, OriginDeclarationId, OriginFunctionId,
    OriginTraitId, OriginTypeCategory, OriginTypeId, StableModuleOriginIdentity,
    StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
//  Fixtures
// ---------------------------------------------------------------------------

fn module_origin() -> StableModuleOriginIdentity {
    StableModuleOriginIdentity::from_portable_path(
        StablePackageIdentity::project_local("test-project"),
        "shapes".to_owned(),
        ModuleRootRole::Normal,
    )
}

fn trait_origin(name: &str) -> OriginTraitId {
    OriginTraitId::new(module_origin(), name.to_owned())
}

fn struct_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(module_origin(), name.to_owned(), OriginTypeCategory::Struct)
}

fn trait_binding(name: &str) -> ExportBinding {
    ExportBinding::new(
        module_origin(),
        name.to_owned(),
        OriginDeclarationId::Trait(trait_origin(name)),
    )
}

fn path(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

fn this_type(env: &mut TypeEnvironment, string_table: &mut StringTable) -> TypeId {
    env.register_synthetic_generic_parameter(string_table.intern("This"))
}

fn requirement(
    name: &str,
    receiver: ResolvedTraitReceiverFact,
    parameters: Vec<ResolvedTraitParameterFact>,
    returns: Vec<ResolvedTraitReturnFact>,
    string_table: &mut StringTable,
) -> ResolvedTraitRequirementFact {
    ResolvedTraitRequirementFact {
        name: string_table.intern(name),
        receiver,
        parameters,
        returns,
    }
}

fn receiver_immutable(this_type: TypeId) -> ResolvedTraitReceiverFact {
    ResolvedTraitReceiverFact {
        access: TraitReceiverAccessKind::Immutable,
        this_type,
    }
}

fn receiver_mutable(this_type: TypeId) -> ResolvedTraitReceiverFact {
    ResolvedTraitReceiverFact {
        access: TraitReceiverAccessKind::Mutable,
        this_type,
    }
}

fn param(
    name: &str,
    value_mode: ValueMode,
    type_id: TypeId,
    string_table: &mut StringTable,
) -> ResolvedTraitParameterFact {
    ResolvedTraitParameterFact {
        name: path(name, string_table),
        value_mode,
        type_id,
    }
}

fn ret(type_id: TypeId, channel: ReturnChannel) -> ResolvedTraitReturnFact {
    ResolvedTraitReturnFact { type_id, channel }
}

fn trait_root(
    name: &str,
    this_type: TypeId,
    requirements: Vec<ResolvedTraitRequirementFact>,
    string_table: &mut StringTable,
) -> ResolvedPublicTraitRoot {
    ResolvedPublicTraitRoot {
        canonical_path: path(name, string_table),
        this_type,
        requirements,
    }
}

fn empty_fields() -> Box<[FieldDefinition]> {
    Box::new([])
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

fn build_traits(
    trait_roots: &[ResolvedPublicTraitRoot],
    bindings: Vec<ExportBinding>,
    nominal_origins: &FxHashMap<InternedPath, OriginTypeId>,
    trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    env: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<Vec<DefinedPublicTraitSurface>, CompilerError> {
    let registry = ExternalPackageRegistry::new();
    build_trait_surfaces(
        trait_roots,
        &bindings,
        nominal_origins,
        trait_origins,
        env,
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

fn trait_origins_map(
    entries: Vec<(&str, OriginTraitId)>,
    string_table: &mut StringTable,
) -> FxHashMap<InternedPath, OriginTraitId> {
    let mut map = FxHashMap::default();
    for (name, origin) in entries {
        map.insert(path(name, string_table), origin);
    }
    map
}

fn constant_origin(name: &str) -> OriginConstantId {
    OriginConstantId::new(module_origin(), name.to_owned())
}

fn free_function_origin(name: &str) -> OriginFunctionId {
    OriginFunctionId::new_free(module_origin(), name.to_owned())
}

fn choice_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(module_origin(), name.to_owned(), OriginTypeCategory::Choice)
}

fn alias_origin(name: &str) -> OriginTypeId {
    OriginTypeId::new(
        module_origin(),
        name.to_owned(),
        OriginTypeCategory::TransparentAlias,
    )
}

fn empty_variant_box() -> Box<[ChoiceVariantDefinition]> {
    Box::new([])
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
        variants: empty_variant_box(),
        generic_parameters: None,
    })
}

fn function_root(
    name: &str,
    signature: FunctionSignature,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Function {
            signature,
            generic_parameter_list_id: None,
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

fn empty_signature() -> FunctionSignature {
    FunctionSignature {
        parameters: vec![],
        returns: vec![],
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

fn type_surface(
    free_functions: Vec<DefinedPublicFunctionTypeSurface>,
    nominal_types: Vec<DefinedPublicNominalTypeSurface>,
    transparent_aliases: Vec<DefinedPublicAliasTypeSurface>,
    constants: Vec<DefinedPublicConstantTypeSurface>,
    receiver_methods: Vec<DefinedPublicReceiverMethodTypeSurface>,
) -> DefinedPublicTypeSurface {
    DefinedPublicTypeSurface {
        free_functions,
        nominal_types,
        transparent_aliases,
        constants,
        receiver_methods,
    }
}

/// Wraps `join_declaration_records` with an empty folded-value context for tests that do not
/// exercise constant folded-value projection.
fn join_with_empty_constants(
    export_bindings: &[ExportBinding],
    type_surface: DefinedPublicTypeSurface,
    trait_surfaces: Vec<DefinedPublicTraitSurface>,
) -> Result<Vec<PublicDeclarationRecord>, CompilerError> {
    let env = TypeEnvironment::new();
    let string_table = StringTable::new();
    let nominal_origins: FxHashMap<InternedPath, OriginTypeId> = FxHashMap::default();
    let nominal_resolver = TransientNominalOriginResolver::new(&env, &nominal_origins);
    let generic_resolver = FoldedValueGenericParameterResolver;
    let registry = ExternalPackageRegistry::new();
    let projection_context =
        CanonicalTypeProjectionContext::new(&nominal_resolver, &generic_resolver, &registry);
    let folded_value_context = FoldedValueJoinContext {
        module_constants: &[],
        type_environment: &env,
        string_table: &string_table,
        projection_context: &projection_context,
    };
    join_declaration_records(
        export_bindings,
        type_surface,
        trait_surfaces,
        &folded_value_context,
    )
}

fn default_location() -> SourceLocation {
    SourceLocation::default()
}

fn immutable() -> ValueMode {
    ValueMode::ImmutableOwned
}

// ---------------------------------------------------------------------------
//  Trait projection: ordered requirements, receivers, SelfType, concrete types
// ---------------------------------------------------------------------------

#[test]
fn projects_trait_with_ordered_requirements_immutable_and_mutable_receivers() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let requirements = vec![
        requirement(
            "read",
            receiver_immutable(this_id),
            vec![param(
                "value",
                ValueMode::MutableOwned,
                int_id,
                &mut string_table,
            )],
            vec![ret(bool_id, ReturnChannel::Success)],
            &mut string_table,
        ),
        requirement(
            "write",
            receiver_mutable(this_id),
            vec![],
            vec![ret(bool_id, ReturnChannel::Success)],
            &mut string_table,
        ),
    ];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let surfaces = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("trait projection succeeds");

    assert_eq!(surfaces.len(), 1);
    let surface = &surfaces[0];
    assert_eq!(surface.origin, trait_origin("Shape"));
    assert_eq!(surface.requirements.len(), 2);
    assert_eq!(&surface.requirements[0].name, "read");
    assert_eq!(
        surface.requirements[0].receiver_access,
        PublicTraitReceiverAccess::Immutable
    );
    assert_eq!(&surface.requirements[1].name, "write");
    assert_eq!(
        surface.requirements[1].receiver_access,
        PublicTraitReceiverAccess::Mutable
    );
}

#[test]
fn projects_self_type_for_direct_this_type_parameter_and_return_occurrences() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let requirements = vec![requirement(
        "transform",
        receiver_immutable(this_id),
        vec![param(
            "other",
            ValueMode::default(),
            this_id,
            &mut string_table,
        )],
        vec![ret(this_id, ReturnChannel::Success)],
        &mut string_table,
    )];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let surfaces = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("trait projection succeeds");

    let requirement = &surfaces[0].requirements[0];
    assert_eq!(
        requirement.parameters[0].type_identity,
        TraitSurfaceTypeIdentity::SelfType
    );
    assert_eq!(
        requirement.returns[0].type_identity,
        TraitSurfaceTypeIdentity::SelfType
    );
}

#[test]
fn projects_ordinary_builtin_and_source_nominal_types_as_concrete() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let int_id = env.builtins().int;
    let (_, widget_id) =
        register_struct(&mut env, &mut string_table, "Widget", empty_fields(), None);

    let requirements = vec![requirement(
        "build",
        receiver_immutable(this_id),
        vec![param(
            "count",
            ValueMode::default(),
            int_id,
            &mut string_table,
        )],
        vec![ret(widget_id, ReturnChannel::Success)],
        &mut string_table,
    )];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let nominal_origins =
        nominal_origins_map(vec![("Widget", struct_origin("Widget"))], &mut string_table);
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let surfaces = build_traits(
        &[root],
        vec![binding],
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("trait projection succeeds");

    let requirement = &surfaces[0].requirements[0];
    assert_eq!(
        requirement.parameters[0].type_identity,
        TraitSurfaceTypeIdentity::Concrete(Box::new(CanonicalTypeIdentity::Builtin(
            CanonicalBuiltinType::Int
        )))
    );
    assert!(matches!(
        &requirement.returns[0].type_identity,
        TraitSurfaceTypeIdentity::Concrete(canonical) if matches!(canonical.as_ref(), CanonicalTypeIdentity::SourceNominal(_))
    ));
}

#[test]
fn retains_value_mode_and_return_channel_facts() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;

    let requirements = vec![requirement(
        "parse",
        receiver_immutable(this_id),
        vec![param(
            "input",
            ValueMode::MutableOwned,
            int_id,
            &mut string_table,
        )],
        vec![
            ret(bool_id, ReturnChannel::Success),
            ret(int_id, ReturnChannel::Error),
        ],
        &mut string_table,
    )];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let surfaces = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("trait projection succeeds");

    let requirement = &surfaces[0].requirements[0];
    assert_eq!(
        requirement.parameters[0].value_mode,
        ValueMode::MutableOwned
    );
    assert_eq!(requirement.returns.len(), 2);
    assert_eq!(requirement.returns[0].channel, ReturnChannel::Success);
    assert_eq!(requirement.returns[1].channel, ReturnChannel::Error);
}

// ---------------------------------------------------------------------------
//  Trait receiver this_type invariant
// ---------------------------------------------------------------------------

#[test]
fn rejects_requirement_receiver_this_type_mismatch() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let other_id = this_type(&mut env, &mut string_table);

    let requirements = vec![requirement(
        "read",
        receiver_immutable(other_id),
        vec![],
        vec![],
        &mut string_table,
    )];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("does not equal the owning trait this_type"),
        "expected a receiver this_type mismatch diagnostic, got: {message}"
    );
}

#[test]
fn rejects_mutable_receiver_this_type_mismatch() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let other_id = this_type(&mut env, &mut string_table);

    let requirements = vec![requirement(
        "write",
        receiver_mutable(other_id),
        vec![],
        vec![],
        &mut string_table,
    )];

    let root = trait_root("Shape", this_id, requirements, &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
//  Inclusion boundaries: only direct, matched traits are projected
// ---------------------------------------------------------------------------

#[test]
fn ignores_non_trait_export_bindings() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Shape", this_id, vec![], &mut string_table);
    // A free-function binding must not produce a trait surface and must not block the trait.
    let function_binding = ExportBinding::new(
        module_origin(),
        "helper".to_owned(),
        OriginDeclarationId::Function(
            crate::compiler_frontend::semantic_identity::OriginFunctionId::new_free(
                module_origin(),
                "helper".to_owned(),
            ),
        ),
    );
    let trait_binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let surfaces = build_traits(
        &[root],
        vec![function_binding, trait_binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("non-trait bindings are skipped");

    assert_eq!(surfaces.len(), 1);
    assert_eq!(&surfaces[0].origin, &trait_origin("Shape"));
}

// ---------------------------------------------------------------------------
//  Totality failures
// ---------------------------------------------------------------------------

#[test]
fn rejects_trait_binding_without_matching_root() {
    let mut string_table = StringTable::new();
    let env = TypeEnvironment::new();

    let binding = trait_binding("Missing");
    let trait_origins = trait_origins_map(vec![], &mut string_table);

    let result = build_traits(
        &[],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("no matching trait root"));
}

#[test]
fn rejects_duplicate_trait_roots_sharing_a_name() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Shape", this_id, vec![], &mut string_table);
    let duplicate = trait_root("Shape", this_id, vec![], &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root, duplicate],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("two trait roots share the public name"));
}

#[test]
fn rejects_trait_root_without_matching_binding() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Orphan", this_id, vec![], &mut string_table);
    let trait_origins =
        trait_origins_map(vec![("Orphan", trait_origin("Orphan"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("has no matching export binding"));
}

#[test]
fn rejects_trait_binding_origin_mismatching_root_resolved_origin() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Shape", this_id, vec![], &mut string_table);
    // The binding names a different trait origin than the root resolves to.
    let wrong_binding = ExportBinding::new(
        module_origin(),
        "Shape".to_owned(),
        OriginDeclarationId::Trait(OriginTraitId::new(module_origin(), "OtherShape".to_owned())),
    );
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![wrong_binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("disagrees with its root resolved origin"));
}

#[test]
fn rejects_trait_root_without_retained_source_trait_origin() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Shape", this_id, vec![], &mut string_table);
    let binding = trait_binding("Shape");
    // The public source-trait origin index is empty, so the root canonical path has no origin.

    let result = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &FxHashMap::default(),
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("no retained public source-trait origin"));
}

// ---------------------------------------------------------------------------
//  Orchestration: declaration-centric draft shape
// ---------------------------------------------------------------------------

#[test]
fn builder_produces_declaration_centric_draft_covering_every_category() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;

    // Register a struct and a choice so the type-surface projection can resolve them.
    let (_, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);
    let (_, choice_type_id) = register_choice(&mut env, &mut string_table, "Status");

    // Build roots for every non-trait category.
    let function_root = function_root("render", empty_signature(), &mut string_table);
    let struct_root = struct_root("Counter", struct_type_id, &mut string_table);
    let choice_root = choice_root("Status", choice_type_id, &mut string_table);
    let alias_root = alias_root("IntAlias", int_id, &mut string_table);
    let constant_root = constant_root("MaxSize", int_id, &mut string_table);

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![
            function_root,
            struct_root,
            choice_root,
            alias_root,
            constant_root,
        ],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    // Build the trait root.
    let this_id = this_type(&mut env, &mut string_table);
    let trait_root = trait_root("Shape", this_id, vec![], &mut string_table);

    // Build export bindings for all six categories, in deterministic sorted order by name.
    let bindings = vec![
        ExportBinding::new(
            module_origin(),
            "Counter".to_owned(),
            OriginDeclarationId::Type(struct_origin("Counter")),
        ),
        ExportBinding::new(
            module_origin(),
            "IntAlias".to_owned(),
            OriginDeclarationId::Type(alias_origin("IntAlias")),
        ),
        ExportBinding::new(
            module_origin(),
            "MaxSize".to_owned(),
            OriginDeclarationId::Constant(constant_origin("MaxSize")),
        ),
        ExportBinding::new(
            module_origin(),
            "Status".to_owned(),
            OriginDeclarationId::Type(choice_origin("Status")),
        ),
        ExportBinding::new(
            module_origin(),
            "render".to_owned(),
            OriginDeclarationId::Function(free_function_origin("render")),
        ),
        trait_binding("Shape"),
    ];

    let nominal_origins = nominal_origins_map(
        vec![
            ("Counter", struct_origin("Counter")),
            ("Status", choice_origin("Status")),
        ],
        &mut string_table,
    );
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let export_origin_draft =
        DefinedPublicExportOriginDraft::new(module_origin(), bindings, nominal_origins.clone());

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![trait_root],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
    };

    let max_size_constant = Declaration {
        id: InternedPath::from_single_str("MaxSize", &mut string_table),
        value: Expression::int(256, default_location(), immutable()),
    };
    let module_constants = vec![max_size_constant];

    let registry = ExternalPackageRegistry::new();
    let draft = PublicInterfaceDraftBuilder::new(PublicInterfaceDraftBuilderInput {
        export_origin_draft,
        public_interface_projection_input: projection_input,
        public_source_nominal_type_origins: &nominal_origins,
        public_source_trait_origins: &trait_origins,
        type_environment: &env,
        external_registry: &registry,
        string_table: &string_table,
        module_constants: &module_constants,
    })
    .build()
    .expect("declaration-centric draft builds for all categories");

    // The draft owns its module origin.
    assert_eq!(draft.module_origin, module_origin());

    // The draft carries exactly six export bindings and six declaration records.
    assert_eq!(draft.export_bindings.len(), 6);
    assert_eq!(draft.declarations.len(), 6);

    // Every semantics category is present as a distinct variant. Collect them by origin name.
    let categories: Vec<&str> = draft
        .declarations
        .iter()
        .map(|record| match &record.semantics {
            PublicDeclarationSemantics::Function(_) => "function",
            PublicDeclarationSemantics::Struct(_) => "struct",
            PublicDeclarationSemantics::Choice(_) => "choice",
            PublicDeclarationSemantics::TransparentAlias(_) => "alias",
            PublicDeclarationSemantics::Constant(_) => "constant",
            PublicDeclarationSemantics::Trait(_) => "trait",
        })
        .collect();
    assert!(categories.contains(&"function"));
    assert!(categories.contains(&"struct"));
    assert!(categories.contains(&"choice"));
    assert!(categories.contains(&"alias"));
    assert!(categories.contains(&"constant"));
    assert!(categories.contains(&"trait"));

    // The constant record carries the canonical builtin int type.
    let constant_record = draft
        .declarations
        .iter()
        .find(|record| matches!(record.semantics, PublicDeclarationSemantics::Constant(_)))
        .expect("constant record exists");
    if let PublicDeclarationSemantics::Constant(semantics) = &constant_record.semantics {
        assert_eq!(
            semantics.type_identity,
            CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
        );
    }

    // The alias record carries the canonical builtin int target.
    let alias_record = draft
        .declarations
        .iter()
        .find(|record| {
            matches!(
                record.semantics,
                PublicDeclarationSemantics::TransparentAlias(_)
            )
        })
        .expect("alias record exists");
    if let PublicDeclarationSemantics::TransparentAlias(semantics) = &alias_record.semantics {
        assert_eq!(
            semantics.target_type_identity,
            CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int)
        );
    }

    // The trait record carries zero requirements and the correct origin.
    let trait_record = draft
        .declarations
        .iter()
        .find(|record| matches!(record.semantics, PublicDeclarationSemantics::Trait(_)))
        .expect("trait record exists");
    if let PublicDeclarationSemantics::Trait(semantics) = &trait_record.semantics {
        assert!(semantics.requirements.is_empty());
    }
    assert_eq!(
        trait_record.origin,
        OriginDeclarationId::Trait(trait_origin("Shape"))
    );
}

#[test]
fn builder_attaches_receiver_methods_to_struct_record() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let (_, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);

    let receiver_path = path("Counter", &mut string_table);
    let method_fn_path = path("render", &mut string_table);
    let signature = FunctionSignature {
        parameters: vec![],
        returns: vec![],
    };
    let entry = receiver_entry(
        method_fn_path.clone(),
        ReceiverKey::Struct(receiver_path),
        signature,
    );

    let root = struct_root("Counter", struct_type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![entry.clone()],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = ExportBinding::new(
        module_origin(),
        "Counter".to_owned(),
        OriginDeclarationId::Type(struct_origin("Counter")),
    );

    let method_origin = OriginFunctionId::new_receiver(
        module_origin(),
        "render".to_owned(),
        struct_origin("Counter"),
    );

    let nominal_origins = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let export_origin_draft = DefinedPublicExportOriginDraft::new(
        module_origin(),
        vec![binding],
        nominal_origins.clone(),
    );

    let mut catalog = ReceiverMethodCatalog::default();
    catalog.by_function_path.insert(method_fn_path, entry);

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(catalog)),
    };

    let registry = ExternalPackageRegistry::new();
    let draft = PublicInterfaceDraftBuilder::new(PublicInterfaceDraftBuilderInput {
        export_origin_draft,
        public_interface_projection_input: projection_input,
        public_source_nominal_type_origins: &nominal_origins,
        public_source_trait_origins: &FxHashMap::default(),
        type_environment: &env,
        external_registry: &registry,
        string_table: &string_table,
        module_constants: &[],
    })
    .build()
    .expect("draft with receiver method builds");

    assert_eq!(draft.declarations.len(), 1);
    let record = &draft.declarations[0];
    assert!(matches!(
        record.semantics,
        PublicDeclarationSemantics::Struct(_)
    ));
    if let PublicDeclarationSemantics::Struct(semantics) = &record.semantics {
        assert_eq!(semantics.receiver_methods.len(), 1);
        assert_eq!(semantics.receiver_methods[0].method_origin, method_origin);
    }
}

#[test]
fn module_origin_survives_empty_public_surface() {
    let string_table = StringTable::new();
    let env = TypeEnvironment::new();

    let export_origin_draft =
        DefinedPublicExportOriginDraft::new(module_origin(), vec![], FxHashMap::default());

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table: ResolvedPublicTypeRootTable::default(),
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
    };

    let registry = ExternalPackageRegistry::new();
    let draft = PublicInterfaceDraftBuilder::new(PublicInterfaceDraftBuilderInput {
        export_origin_draft,
        public_interface_projection_input: projection_input,
        public_source_nominal_type_origins: &FxHashMap::default(),
        public_source_trait_origins: &FxHashMap::default(),
        type_environment: &env,
        external_registry: &registry,
        string_table: &string_table,
        module_constants: &[],
    })
    .build()
    .expect("empty-surface draft builds");

    assert_eq!(draft.module_origin, module_origin());
    assert!(draft.export_bindings.is_empty());
    assert!(draft.declarations.is_empty());
}

// ---------------------------------------------------------------------------
//  Declaration-centric join totality
// ---------------------------------------------------------------------------

#[test]
fn join_rejects_binding_without_matching_type_surface_entry() {
    let binding = ExportBinding::new(
        module_origin(),
        "missing".to_owned(),
        OriginDeclarationId::Function(free_function_origin("missing")),
    );
    let type_surface = type_surface(vec![], vec![], vec![], vec![], vec![]);

    let result = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("no matching free-function type-surface entry"));
}

#[test]
fn join_rejects_extra_type_surface_entry_without_binding() {
    let function = DefinedPublicFunctionTypeSurface {
        origin: free_function_origin("orphan"),
        generic_parameters: vec![],
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let result = join_with_empty_constants(&[], type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(message.contains("no matching export binding"));
}

#[test]
fn join_produces_one_record_per_origin() {
    let function_origin = free_function_origin("render");
    let binding = ExportBinding::new(
        module_origin(),
        "render".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );
    let function = DefinedPublicFunctionTypeSurface {
        origin: function_origin,
        generic_parameters: vec![],
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let records = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![])
        .expect("join succeeds for one function");

    assert_eq!(records.len(), 1);
    assert!(matches!(
        records[0].semantics,
        PublicDeclarationSemantics::Function(_)
    ));
}

// ---------------------------------------------------------------------------
//  Trait root this_type validation
// ---------------------------------------------------------------------------

#[test]
fn rejects_trait_root_with_builtin_this_type() {
    let mut string_table = StringTable::new();
    let env = TypeEnvironment::new();
    let int_id = env.builtins().int;

    // A builtin int TypeId is not a GenericParameter named "This".
    let root = trait_root("Shape", int_id, vec![], &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("not a GenericParameter"),
        "expected a malformed this_type diagnostic, got: {message}"
    );
}

#[test]
fn rejects_trait_root_with_wrong_name_generic_parameter() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    // Register a synthetic generic parameter with the wrong name.
    let wrong_id = env.register_synthetic_generic_parameter(string_table.intern("Other"));

    let root = trait_root("Shape", wrong_id, vec![], &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let result = build_traits(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("not \"This\""),
        "expected a wrong-name this_type diagnostic, got: {message}"
    );
}

// ---------------------------------------------------------------------------
//  Declaration-centric join: unique-origin and total-join invariants
// ---------------------------------------------------------------------------

#[test]
fn join_emits_one_record_when_two_bindings_share_one_origin() {
    let function_origin = free_function_origin("render");

    // Two export bindings name the same function origin under different public names. The
    // data-model invariant is one declaration record per unique origin; both bindings are
    // preserved separately by the draft (not by the join).
    let binding_a = ExportBinding::new(
        module_origin(),
        "render".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );
    let binding_b = ExportBinding::new(
        module_origin(),
        "export_render".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );

    let function = DefinedPublicFunctionTypeSurface {
        origin: function_origin,
        generic_parameters: vec![],
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let records = join_with_empty_constants(&[binding_a, binding_b], type_surface, vec![])
        .expect("join succeeds with one record for a shared origin");

    assert_eq!(records.len(), 1, "one record per unique origin");
    assert!(matches!(
        records[0].semantics,
        PublicDeclarationSemantics::Function(_)
    ));
}

#[test]
fn join_rejects_duplicate_type_surface_origin() {
    let function_origin = free_function_origin("render");
    let function = DefinedPublicFunctionTypeSurface {
        origin: function_origin.clone(),
        generic_parameters: vec![],
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let duplicate = DefinedPublicFunctionTypeSurface {
        origin: function_origin,
        generic_parameters: vec![],
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let type_surface = type_surface(vec![function, duplicate], vec![], vec![], vec![], vec![]);

    let result = join_with_empty_constants(&[], type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("two free-function type-surface entries share origin"),
        "expected a duplicate-origin diagnostic, got: {message}"
    );
}

#[test]
fn join_rejects_struct_fact_containing_choice_variants() {
    let origin = struct_origin("Counter");
    let binding = ExportBinding::new(
        module_origin(),
        "Counter".to_owned(),
        OriginDeclarationId::Type(origin.clone()),
    );

    let nominal = DefinedPublicNominalTypeSurface {
        origin,
        generic_parameters: vec![],
        fields: vec![],
        variants: vec![PublicChoiceVariantSurface {
            name: "unexpected".to_owned(),
            payload_fields: vec![],
        }],
    };
    let type_surface = type_surface(vec![], vec![nominal], vec![], vec![], vec![]);

    let result = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("a struct must not contain choice variants"),
        "expected a struct-with-variants diagnostic, got: {message}"
    );
}

#[test]
fn join_rejects_choice_fact_containing_struct_fields() {
    let origin = choice_origin("Status");
    let binding = ExportBinding::new(
        module_origin(),
        "Status".to_owned(),
        OriginDeclarationId::Type(origin.clone()),
    );

    let nominal = DefinedPublicNominalTypeSurface {
        origin,
        generic_parameters: vec![],
        fields: vec![PublicFieldTypeSlot {
            name: "unexpected".to_owned(),
            type_identity: CanonicalTypeIdentity::Builtin(CanonicalBuiltinType::Int),
        }],
        variants: vec![],
    };
    let type_surface = type_surface(vec![], vec![nominal], vec![], vec![], vec![]);

    let result = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("a choice must not contain struct fields"),
        "expected a choice-with-fields diagnostic, got: {message}"
    );
}

#[test]
fn join_rejects_duplicate_receiver_method_origin() {
    let origin = struct_origin("Counter");
    let method_origin =
        OriginFunctionId::new_receiver(module_origin(), "render".to_owned(), origin.clone());

    let method_a = DefinedPublicReceiverMethodTypeSurface {
        receiver_origin: origin.clone(),
        method_origin: method_origin.clone(),
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let method_b = DefinedPublicReceiverMethodTypeSurface {
        receiver_origin: origin.clone(),
        method_origin,
        parameters: vec![],
        returns: vec![],
        error_return: None,
    };
    let type_surface = type_surface(vec![], vec![], vec![], vec![], vec![method_a, method_b]);

    // A binding is not needed: the duplicate is caught while indexing receiver methods, before
    // the binding loop runs.
    let result = join_with_empty_constants(&[], type_surface, vec![]);

    assert!(result.is_err());
    let message = result.unwrap_err().msg.clone();
    assert!(
        message.contains("two receiver-method type-surface entries share method origin"),
        "expected a duplicate method-origin diagnostic, got: {message}"
    );
}

// ---------------------------------------------------------------------------
//  Folded-value projection tests live in a focused sibling module
// ---------------------------------------------------------------------------

#[cfg(test)]
mod folded_value_tests;
