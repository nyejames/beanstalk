//! Focused unit tests for the public-interface draft aggregate and the corrected direct
//! trait-requirement projection.
//!
//! WHAT: exercises the structural invariants of [`PublicInterfaceDraft`] and the
//! `build_trait_surfaces` projection that integration output cannot inspect: ordered trait
//! requirements, immutable and mutable receivers, trait-local `SelfType` for direct
//! `this_type` occurrences, ordinary builtin and imported canonical nominal projection,
//! `ValueMode` and `ReturnChannel` retention, the trait receiver `this_type` invariant, and
//! totality failures for missing, duplicate, unmatched and wrong-origin inputs. Also proves
//! the draft builder carries exactly one aggregate draft with its four projection steps,
//! including reusable evidence.
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
    ResolvedTraitSourceFact, TraitReceiverAccessKind,
};
use crate::compiler_frontend::builtins::casts::targets::{
    BuiltinCastFallibility, BuiltinCastTarget,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalBuiltinType, CanonicalCoreTraitIdentity, CanonicalEvidenceIdentity,
    CanonicalTraitIdentity, CanonicalTypeIdentity, CanonicalTypeProjectionContext,
    ExportedGenericParameterIdentity, GenericDeclarationOrigin,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::datatypes::datatype::DataType;
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantDefinition, ChoiceVariantPayloadDefinition, FieldDefinition,
    StructTypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterListId, NominalTypeId, TypeId};
use crate::compiler_frontend::defined_public_export_origins::DefinedPublicExportOriginDraft;
use crate::compiler_frontend::defined_public_type_surface::{
    DefinedPublicAliasTypeSurface, DefinedPublicConstantTypeSurface,
    DefinedPublicFunctionTypeSurface, DefinedPublicNominalTypeSurface,
    DefinedPublicReceiverMethodTypeSurface, DefinedPublicTypeSurface, PublicChoiceVariantSurface,
    PublicFieldTypeSlot, PublicGenericParameterSurface, TransientNominalOriginResolver,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::folded_value::{
    FoldedValueGenericParameterResolver, PublicFoldedValue,
};
use crate::compiler_frontend::public_interface_draft::{
    DefinedPublicTraitSurface, EvidenceProjectionContext, FoldedValueJoinContext,
    PublicDeclarationRecord, PublicDeclarationSemantics, PublicEvidenceOwnership,
    PublicEvidenceRecord, PublicInterfaceDraftBuilder, PublicInterfaceDraftBuilderInput,
    PublicReceiverMethodSemantics, PublicStructSemantics, PublicTraitReceiverAccess,
    TraitSurfaceTypeIdentity, build_trait_surfaces, join_declaration_records,
    project_reusable_evidence,
};
use crate::compiler_frontend::semantic_identity::{
    ExportBinding, ModuleRootRole, OriginConstantId, OriginDeclarationId, OriginFunctionId,
    OriginTraitId, OriginTypeCategory, OriginTypeId, StableModuleOriginIdentity,
    StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitRequirement, ResolvedTraitReturn,
    TraitReceiverRequirement, TraitVisibility,
};
use crate::compiler_frontend::traits::environment::{CoreTraitKind, TraitEnvironment};
use crate::compiler_frontend::traits::evidence::TraitEvidenceEnvironment;
use crate::compiler_frontend::traits::evidence::environment::{
    TraitEvidenceDefinition, TraitEvidenceKind, TraitRequirementEvidence,
};
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId, TraitRequirementId};
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
        incompatible_trait_ids: Vec::new(),
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
    build_traits_with_facts(
        trait_roots,
        bindings,
        &FxHashMap::default(),
        nominal_origins,
        trait_origins,
        env,
        string_table,
    )
}

fn build_traits_with_facts(
    trait_roots: &[ResolvedPublicTraitRoot],
    bindings: Vec<ExportBinding>,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    nominal_origins: &FxHashMap<InternedPath, OriginTypeId>,
    trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    env: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<Vec<DefinedPublicTraitSurface>, CompilerError> {
    let registry = ExternalPackageRegistry::new();
    build_trait_surfaces(
        trait_roots,
        &bindings,
        trait_source_facts,
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
    fields: Vec<Declaration>,
    string_table: &mut StringTable,
) -> ResolvedPublicTypeRoot {
    ResolvedPublicTypeRoot {
        path: path(name, string_table),
        kind: ResolvedPublicTypeRootKind::Struct { type_id, fields },
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
    let struct_root = struct_root("Counter", struct_type_id, vec![], &mut string_table);
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
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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

    let root = struct_root("Counter", struct_type_id, vec![], &mut string_table);
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
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
//  Generic-template descriptor classification
// ---------------------------------------------------------------------------

/// Build one `DefinedPublicFunctionTypeSurface` carrying only the given generic parameters so
/// a draft-join test can exercise generic versus non-generic descriptor classification without
/// reconstructing the full AST projection.
fn function_type_surface(
    origin: OriginFunctionId,
    generic_parameters: Vec<PublicGenericParameterSurface>,
) -> DefinedPublicFunctionTypeSurface {
    DefinedPublicFunctionTypeSurface {
        origin,
        generic_parameters,
        parameters: vec![],
        returns: vec![],
        error_return: None,
    }
}

/// Construct one stable exported generic parameter identity for position `position` named
/// `name` on the given free-function origin.
fn exported_generic_parameter(
    origin: &OriginFunctionId,
    position: u32,
    name: &str,
) -> ExportedGenericParameterIdentity {
    let declaration_origin = GenericDeclarationOrigin::free_function(origin.clone())
        .expect("a free function is a valid generic declaration owner");
    ExportedGenericParameterIdentity::new(declaration_origin, position, name.to_owned())
}

#[test]
fn generic_free_function_carries_explicit_template_descriptor() {
    let function_origin = free_function_origin("identity");
    let binding = ExportBinding::new(
        module_origin(),
        "identity".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );

    let parameter_identity = exported_generic_parameter(&function_origin, 0, "T");
    let generic_parameters = vec![PublicGenericParameterSurface {
        identity: parameter_identity.clone(),
        bounds: vec![CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Displayable,
        )],
    }];

    let function = function_type_surface(function_origin, generic_parameters);
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let records = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![])
        .expect("join succeeds for one generic function");

    assert_eq!(records.len(), 1);
    let PublicDeclarationSemantics::Function(semantics) = &records[0].semantics else {
        panic!("expected a function record");
    };

    let descriptor = semantics
        .generic_template
        .as_ref()
        .expect("a generic free function must carry one explicit template descriptor");
    assert_eq!(
        descriptor.generic_parameters.len(),
        1,
        "the descriptor owns the single stable generic parameter"
    );

    let parameter = &descriptor.generic_parameters[0];
    assert_eq!(
        &parameter.identity, &parameter_identity,
        "the descriptor retains the stable exported generic parameter identity"
    );
    assert_eq!(
        &parameter.bounds,
        &[CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Displayable,
        )],
        "the descriptor retains the ordered canonical trait bounds"
    );
}

#[test]
fn non_generic_free_function_carries_no_template_descriptor() {
    let function_origin = free_function_origin("render");
    let binding = ExportBinding::new(
        module_origin(),
        "render".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );

    let function = function_type_surface(function_origin, vec![]);
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let records = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![])
        .expect("join succeeds for one non-generic function");

    assert_eq!(records.len(), 1);
    let PublicDeclarationSemantics::Function(semantics) = &records[0].semantics else {
        panic!("expected a function record");
    };

    assert!(
        semantics.generic_template.is_none(),
        "a non-generic free function must carry no template descriptor"
    );
}

#[test]
fn generic_template_descriptor_owns_stable_identity_without_donor_local_handles() {
    let function_origin = free_function_origin("identity");
    let binding = ExportBinding::new(
        module_origin(),
        "identity".to_owned(),
        OriginDeclarationId::Function(function_origin.clone()),
    );

    let first_identity = exported_generic_parameter(&function_origin, 0, "T");
    let second_identity = exported_generic_parameter(&function_origin, 1, "U");
    let generic_parameters = vec![
        PublicGenericParameterSurface {
            identity: first_identity.clone(),
            bounds: vec![CanonicalTraitIdentity::Core(
                CanonicalCoreTraitIdentity::Displayable,
            )],
        },
        PublicGenericParameterSurface {
            identity: second_identity.clone(),
            bounds: vec![],
        },
    ];

    let function = function_type_surface(function_origin, generic_parameters);
    let type_surface = type_surface(vec![function], vec![], vec![], vec![], vec![]);

    let records = join_with_empty_constants(std::slice::from_ref(&binding), type_surface, vec![])
        .expect("join succeeds for a two-parameter generic function");

    let PublicDeclarationSemantics::Function(semantics) = &records[0].semantics else {
        panic!("expected a function record");
    };
    let descriptor = semantics
        .generic_template
        .as_ref()
        .expect("a generic free function must carry one explicit template descriptor");

    // The descriptor owns only stable exported identities and canonical trait bounds. The
    // enclosing declaration record owns the origin and the enclosing function semantics own
    // the parameter and return contract, so the descriptor carries neither.
    assert_eq!(
        descriptor
            .generic_parameters
            .iter()
            .map(|parameter| &parameter.identity)
            .collect::<Vec<_>>(),
        &[&first_identity, &second_identity],
        "the descriptor preserves authored parameter order through stable identities"
    );
    assert_eq!(
        &descriptor.generic_parameters[0].bounds,
        &[CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Displayable,
        )],
        "the first parameter retains its ordered canonical bound"
    );
    assert!(
        descriptor.generic_parameters[1].bounds.is_empty(),
        "the second parameter retains its empty bound set"
    );
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
            folded_default: None,
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
//  Default retention tests (R2c)
// ---------------------------------------------------------------------------

/// Helper: create a field `Declaration` carrying a compile-time default value.
fn field_declaration_with_default(
    name: &str,
    type_id: TypeId,
    default: Expression,
    string_table: &mut StringTable,
) -> Declaration {
    let mut value = default;
    value.type_id = type_id;
    Declaration {
        id: path(name, string_table),
        value,
    }
}

/// Helper: create a field `Declaration` with no default.
fn field_declaration_no_default(
    name: &str,
    type_id: TypeId,
    string_table: &mut StringTable,
) -> Declaration {
    Declaration {
        id: path(name, string_table),
        value: Expression::no_value_with_type_id(
            default_location(),
            DataType::Inferred,
            type_id,
            immutable(),
        ),
    }
}

/// Helper: create a `FieldDefinition` matching a field name and type.
fn field_def(name: &str, type_id: TypeId, string_table: &mut StringTable) -> FieldDefinition {
    FieldDefinition {
        name: path(name, string_table),
        type_id,
        location: default_location(),
    }
}

#[test]
fn free_function_retains_folded_parameter_defaults_in_authored_order() {
    let mut string_table = StringTable::new();
    let env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let string_id = env.builtins().string;

    // Every default expression carries its declared TypeId so the projection reads the
    // correct canonical type identity from the expression, not from a global builtin
    // constant that may differ from the environment.
    let parameters = vec![
        field_declaration_with_default(
            "prefix",
            string_id,
            Expression::string_slice(
                string_table.intern("default-prefix"),
                default_location(),
                immutable(),
            ),
            &mut string_table,
        ),
        field_declaration_with_default(
            "count",
            int_id,
            Expression::int(42, default_location(), immutable()),
            &mut string_table,
        ),
        field_declaration_no_default("subject", string_id, &mut string_table),
    ];

    let signature = FunctionSignature {
        parameters,
        returns: vec![],
    };

    let root = function_root("render", signature, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = ExportBinding::new(
        module_origin(),
        "render".to_owned(),
        OriginDeclarationId::Function(free_function_origin("render")),
    );

    let export_origin_draft =
        DefinedPublicExportOriginDraft::new(module_origin(), vec![binding], FxHashMap::default());

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
    .expect("draft with function defaults should build");

    assert_eq!(draft.declarations.len(), 1);
    let record = &draft.declarations[0];
    let PublicDeclarationSemantics::Function(semantics) = &record.semantics else {
        panic!("expected a function record");
    };

    assert_eq!(semantics.parameters.len(), 3);

    assert_eq!(semantics.parameters[0].name.as_deref(), Some("prefix"));
    assert_eq!(
        &semantics.parameters[0].folded_default,
        &Some(PublicFoldedValue::String("default-prefix".to_owned()))
    );

    assert_eq!(semantics.parameters[1].name.as_deref(), Some("count"));
    assert_eq!(
        &semantics.parameters[1].folded_default,
        &Some(PublicFoldedValue::Int(42))
    );

    assert_eq!(semantics.parameters[2].name.as_deref(), Some("subject"));
    assert_eq!(&semantics.parameters[2].folded_default, &None);
}

#[test]
fn struct_retains_folded_field_defaults_in_authored_order() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;
    let bool_id = env.builtins().bool;
    let string_id = env.builtins().string;

    let field_defs = Box::new([
        field_def("x", int_id, &mut string_table),
        field_def("flag", bool_id, &mut string_table),
        field_def("label", string_id, &mut string_table),
    ]);

    let (_, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Point", field_defs, None);

    let fields = vec![
        field_declaration_with_default(
            "x",
            int_id,
            Expression::int(10, default_location(), immutable()),
            &mut string_table,
        ),
        field_declaration_with_default(
            "flag",
            bool_id,
            Expression::bool(true, default_location(), immutable()),
            &mut string_table,
        ),
        field_declaration_no_default("label", string_id, &mut string_table),
    ];

    let root = struct_root("Point", struct_type_id, fields, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = ExportBinding::new(
        module_origin(),
        "Point".to_owned(),
        OriginDeclarationId::Type(struct_origin("Point")),
    );

    let nominal_origins =
        nominal_origins_map(vec![("Point", struct_origin("Point"))], &mut string_table);

    let export_origin_draft = DefinedPublicExportOriginDraft::new(
        module_origin(),
        vec![binding],
        nominal_origins.clone(),
    );

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
    .expect("draft with struct field defaults should build");

    assert_eq!(draft.declarations.len(), 1);
    let record = &draft.declarations[0];
    let PublicDeclarationSemantics::Struct(semantics) = &record.semantics else {
        panic!("expected a struct record");
    };

    assert_eq!(semantics.fields.len(), 3);

    assert_eq!(semantics.fields[0].name.as_str(), "x");
    assert_eq!(
        &semantics.fields[0].folded_default,
        &Some(PublicFoldedValue::Int(10))
    );

    assert_eq!(semantics.fields[1].name.as_str(), "flag");
    assert_eq!(
        &semantics.fields[1].folded_default,
        &Some(PublicFoldedValue::Bool(true))
    );

    assert_eq!(semantics.fields[2].name.as_str(), "label");
    assert_eq!(&semantics.fields[2].folded_default, &None);
}

#[test]
fn choice_payload_fields_remain_default_free() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let int_id = env.builtins().int;

    let variant = ChoiceVariantDefinition {
        name: string_table.intern("Some"),
        tag: 0,
        payload: ChoiceVariantPayloadDefinition::Record {
            fields: Box::new([FieldDefinition {
                name: path("value", &mut string_table),
                type_id: int_id,
                location: default_location(),
            }]),
        },
        location: default_location(),
    };

    let choice_path = path("Option", &mut string_table);
    env.register_nominal_choice(ChoiceTypeDefinition {
        id: NominalTypeId(0),
        path: choice_path,
        variants: Box::new([variant]),
        generic_parameters: None,
    });

    let choice_type_id = env
        .type_id_for_nominal_id(NominalTypeId(0))
        .expect("choice must have a TypeId");

    let root = choice_root("Option", choice_type_id, &mut string_table);
    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![root],
        receiver_methods: vec![],
        trait_source_facts: FxHashMap::default(),
    };

    let binding = ExportBinding::new(
        module_origin(),
        "Option".to_owned(),
        OriginDeclarationId::Type(choice_origin("Option")),
    );

    let nominal_origins =
        nominal_origins_map(vec![("Option", choice_origin("Option"))], &mut string_table);

    let export_origin_draft = DefinedPublicExportOriginDraft::new(
        module_origin(),
        vec![binding],
        nominal_origins.clone(),
    );

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
    .expect("draft with choice should build");

    let record = &draft.declarations[0];
    let PublicDeclarationSemantics::Choice(semantics) = &record.semantics else {
        panic!("expected a choice record");
    };

    assert_eq!(semantics.variants.len(), 1);
    let variant = &semantics.variants[0];
    assert_eq!(variant.payload_fields.len(), 1);
    assert_eq!(variant.payload_fields[0].name.as_str(), "value");
    assert_eq!(&variant.payload_fields[0].folded_default, &None);
}

#[test]
fn receiver_method_retains_folded_parameter_defaults() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let string_id = env.builtins().string;

    let (_, struct_type_id) =
        register_struct(&mut env, &mut string_table, "Counter", empty_fields(), None);

    let receiver_path = path("Counter", &mut string_table);
    let method_fn_path = path("render", &mut string_table);

    // The first parameter is the `this` receiver: it carries the struct TypeId and has no
    // default. The second parameter is an ordinary parameter with a folded default.
    let signature = FunctionSignature {
        parameters: vec![
            field_declaration_no_default("this", struct_type_id, &mut string_table),
            field_declaration_with_default(
                "label",
                string_id,
                Expression::string_slice(
                    string_table.intern("fallback"),
                    default_location(),
                    immutable(),
                ),
                &mut string_table,
            ),
        ],
        returns: vec![],
    };

    let entry = receiver_entry(
        method_fn_path.clone(),
        ReceiverKey::Struct(receiver_path.clone()),
        signature,
    );

    let root = struct_root("Counter", struct_type_id, vec![], &mut string_table);
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

    let nominal_origins = nominal_origins_map(
        vec![("Counter", struct_origin("Counter"))],
        &mut string_table,
    );

    let export_origin_draft = DefinedPublicExportOriginDraft::new(
        module_origin(),
        vec![binding],
        nominal_origins.clone(),
    );

    let mut receiver_catalog = ReceiverMethodCatalog::default();
    receiver_catalog
        .by_function_path
        .insert(method_fn_path, entry);

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table,
        trait_roots: vec![],
        receiver_catalog: Some(std::rc::Rc::new(receiver_catalog)),
        trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
        trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
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
    .expect("draft with receiver method defaults should build");

    let record = &draft.declarations[0];
    let PublicDeclarationSemantics::Struct(semantics) = &record.semantics else {
        panic!("expected a struct record");
    };

    assert_eq!(semantics.receiver_methods.len(), 1);
    let method = &semantics.receiver_methods[0];

    // The receiver slot remains default-free and authored order is preserved.
    assert_eq!(method.parameters.len(), 2);

    assert_eq!(method.parameters[0].name.as_deref(), Some("this"));
    assert_eq!(&method.parameters[0].folded_default, &None);

    assert_eq!(method.parameters[1].name.as_deref(), Some("label"));
    assert_eq!(
        &method.parameters[1].folded_default,
        &Some(PublicFoldedValue::String("fallback".to_owned()))
    );
}

// ---------------------------------------------------------------------------
//  Trait incompatibility projection
// ---------------------------------------------------------------------------

/// Build a trait root carrying explicit incompatible trait ids, for incompatibility
/// projection tests that bypass the trait environment.
fn trait_root_with_incompatibilities(
    name: &str,
    this_type: TypeId,
    incompatible_trait_ids: Vec<TraitId>,
    string_table: &mut StringTable,
) -> ResolvedPublicTraitRoot {
    ResolvedPublicTraitRoot {
        canonical_path: path(name, string_table),
        this_type,
        requirements: Vec::new(),
        incompatible_trait_ids,
    }
}

fn core_cast_fact(
    trait_id: u32,
    target: BuiltinCastTarget,
    fallibility: BuiltinCastFallibility,
) -> (TraitId, ResolvedTraitSourceFact) {
    (
        TraitId(trait_id),
        ResolvedTraitSourceFact::Core(CoreTraitKind::Castable {
            target,
            fallibility,
        }),
    )
}

#[test]
fn projects_source_to_source_incompatibility_identity() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // Alpha (the direct public trait) is incompatible with Beta (another source trait). Both
    // resolve to Source(OriginTraitId) through the public source-trait origin index.
    let alpha_path = path("Alpha", &mut string_table);
    let beta_path = path("Beta", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(alpha_path));
    trait_source_facts.insert(TraitId(1), ResolvedTraitSourceFact::Source(beta_path));

    let root =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(1)], &mut string_table);
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));
    trait_origins.insert(path("Beta", &mut string_table), trait_origin("Beta"));

    let surfaces = build_traits_with_facts(
        &[root],
        vec![binding],
        &trait_source_facts,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("a source-to-source public incompatibility projects to a canonical Source identity");

    assert_eq!(surfaces.len(), 1);
    assert_eq!(
        surfaces[0].incompatibilities,
        vec![CanonicalTraitIdentity::Source(trait_origin("Beta"))]
    );
}

#[test]
fn projects_source_to_core_incompatibility_identity() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // Alpha (a direct public source trait) is incompatible with a compiler-owned core cast
    // trait. The core trait side resolves to a stable CanonicalCoreTraitIdentity.
    let alpha_path = path("Alpha", &mut string_table);
    let (core_id, core_fact) = core_cast_fact(
        7,
        BuiltinCastTarget::String,
        BuiltinCastFallibility::Infallible,
    );
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(alpha_path));
    trait_source_facts.insert(core_id, core_fact);

    let root =
        trait_root_with_incompatibilities("Alpha", this_id, vec![core_id], &mut string_table);
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));

    let surfaces = build_traits_with_facts(
        &[root],
        vec![binding],
        &trait_source_facts,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("a source-to-core public incompatibility projects to a canonical Core identity");

    assert_eq!(surfaces.len(), 1);
    assert_eq!(
        surfaces[0].incompatibilities,
        vec![CanonicalTraitIdentity::Core(
            CanonicalCoreTraitIdentity::Castable {
                target: BuiltinCastTarget::String,
                fallibility: BuiltinCastFallibility::Infallible,
            }
        )]
    );
}

#[test]
fn incompatibility_identity_is_stable_across_local_trait_id_allocation() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // Two independent local TraitId allocations (10 and 99) for the same source trait path
    // produce the same canonical incompatibility fact, because identity derives from the
    // stable OriginTraitId, not from the donor-local TraitId.
    let beta_path = path("Beta", &mut string_table);
    let alpha_path = path("Alpha", &mut string_table);

    let mut facts_a = FxHashMap::default();
    facts_a.insert(
        TraitId(0),
        ResolvedTraitSourceFact::Source(alpha_path.clone()),
    );
    facts_a.insert(
        TraitId(10),
        ResolvedTraitSourceFact::Source(beta_path.clone()),
    );

    let mut facts_b = FxHashMap::default();
    facts_b.insert(
        TraitId(0),
        ResolvedTraitSourceFact::Source(alpha_path.clone()),
    );
    facts_b.insert(
        TraitId(99),
        ResolvedTraitSourceFact::Source(beta_path.clone()),
    );

    let root_a =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(10)], &mut string_table);
    let root_b =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(99)], &mut string_table);
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));
    trait_origins.insert(path("Beta", &mut string_table), trait_origin("Beta"));

    let surfaces_a = build_traits_with_facts(
        &[root_a],
        vec![trait_binding("Alpha")],
        &facts_a,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("first allocation projects");
    let surfaces_b = build_traits_with_facts(
        &[root_b],
        vec![trait_binding("Alpha")],
        &facts_b,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("second allocation projects");

    assert_eq!(
        surfaces_a[0].incompatibilities,
        surfaces_b[0].incompatibilities
    );
    assert_eq!(
        surfaces_a[0].incompatibilities,
        vec![CanonicalTraitIdentity::Source(trait_origin("Beta"))]
    );
}

#[test]
fn rejects_duplicate_canonical_incompatibility_identity() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // Two distinct local TraitIds resolve to the same source trait path, so both
    // incompatibilities canonicalize to the same CanonicalTraitIdentity and the projection
    // rejects the duplicate.
    let beta_path = path("Beta", &mut string_table);
    let alpha_path = path("Alpha", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(alpha_path));
    trait_source_facts.insert(
        TraitId(1),
        ResolvedTraitSourceFact::Source(beta_path.clone()),
    );
    trait_source_facts.insert(TraitId(2), ResolvedTraitSourceFact::Source(beta_path));

    let root = trait_root_with_incompatibilities(
        "Alpha",
        this_id,
        vec![TraitId(1), TraitId(2)],
        &mut string_table,
    );
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));
    trait_origins.insert(path("Beta", &mut string_table), trait_origin("Beta"));

    let result = build_traits_with_facts(
        &[root],
        vec![binding],
        &trait_source_facts,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a duplicate canonical incompatibility identity must be rejected")
        .msg;
    assert!(
        message.contains("a duplicate must not enter the public trait surface"),
        "expected a duplicate-identity rejection, got: {message}"
    );
}

#[test]
fn rejects_incompatibility_without_retained_source_fact() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // The incompatible TraitId has no entry in the trait-source-fact table, so the projection
    // cannot classify it and fails through a CompilerError.
    let root =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(5)], &mut string_table);
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));

    let result = build_traits_with_facts(
        &[root],
        vec![binding],
        &FxHashMap::default(),
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a missing trait source fact must be rejected")
        .msg;
    assert!(
        message.contains("has no retained trait source fact"),
        "expected a missing-source-fact rejection, got: {message}"
    );
}

#[test]
fn rejects_incompatibility_source_without_public_source_trait_origin() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // The incompatible trait is a source trait, but its canonical path has no entry in the
    // public source-trait origin index, so it is private/unexported and must not enter the
    // public trait surface.
    let beta_path = path("Beta", &mut string_table);
    let alpha_path = path("Alpha", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(alpha_path));
    trait_source_facts.insert(TraitId(1), ResolvedTraitSourceFact::Source(beta_path));

    let root =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(1)], &mut string_table);
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));

    let result = build_traits_with_facts(
        &[root],
        vec![binding],
        &trait_source_facts,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a missing public source-trait origin must be rejected")
        .msg;
    assert!(
        message.contains("no retained public source-trait origin"),
        "expected a missing-origin rejection, got: {message}"
    );
}

#[test]
fn rejects_internal_self_incompatibility_relation() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    // The direct public trait Alpha carries an incompatibility that resolves to itself. An
    // authored self-relation is rejected earlier by a user-facing diagnostic, so reaching
    // this point means inconsistent resolved metadata and must fail through a CompilerError.
    let alpha_path = path("Alpha", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(alpha_path));

    let root =
        trait_root_with_incompatibilities("Alpha", this_id, vec![TraitId(0)], &mut string_table);
    let binding = trait_binding("Alpha");
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Alpha", &mut string_table), trait_origin("Alpha"));

    let result = build_traits_with_facts(
        &[root],
        vec![binding],
        &trait_source_facts,
        &FxHashMap::default(),
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("an internal self-relation must be rejected")
        .msg;
    assert!(
        message.contains("resolves to itself"),
        "expected a self-relation rejection, got: {message}"
    );
}

#[test]
fn builder_carries_incompatibilities_on_trait_record() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);
    let int_id = env.builtins().int;

    // Build a minimal draft where the only trait (Shape) carries one public incompatibility
    // with another source trait (Mark). The builder must surface the canonical incompatibility
    // on the trait declaration record.
    let shape_path = path("Shape", &mut string_table);
    let mark_path = path("Mark", &mut string_table);
    let mut trait_source_facts = FxHashMap::default();
    trait_source_facts.insert(TraitId(0), ResolvedTraitSourceFact::Source(shape_path));
    trait_source_facts.insert(TraitId(1), ResolvedTraitSourceFact::Source(mark_path));

    let requirement_fact = requirement(
        "read",
        receiver_immutable(this_id),
        vec![param(
            "value",
            ValueMode::MutableOwned,
            int_id,
            &mut string_table,
        )],
        vec![ret(env.builtins().bool, ReturnChannel::Success)],
        &mut string_table,
    );
    let trait_root = ResolvedPublicTraitRoot {
        canonical_path: path("Shape", &mut string_table),
        this_type: this_id,
        requirements: vec![requirement_fact],
        incompatible_trait_ids: vec![TraitId(1)],
    };

    let root_table = ResolvedPublicTypeRootTable {
        roots: vec![],
        receiver_methods: vec![],
        trait_source_facts,
    };

    let nominal_origins: FxHashMap<InternedPath, OriginTypeId> = FxHashMap::default();
    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("Shape", &mut string_table), trait_origin("Shape"));
    trait_origins.insert(path("Mark", &mut string_table), trait_origin("Mark"));

    let export_origin_draft = DefinedPublicExportOriginDraft::new(
        module_origin(),
        vec![trait_binding("Shape")],
        FxHashMap::default(),
    );

    let draft = PublicInterfaceDraftBuilder::new(PublicInterfaceDraftBuilderInput {
        export_origin_draft,
        public_interface_projection_input: AstPublicInterfaceProjectionInput {
            root_table,
            trait_roots: vec![trait_root],
            receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
            trait_environment: Some(std::rc::Rc::new(TraitEnvironment::new())),
            trait_evidence_environment: Some(std::rc::Rc::new(TraitEvidenceEnvironment::new())),
        },
        public_source_nominal_type_origins: &nominal_origins,
        public_source_trait_origins: &trait_origins,
        type_environment: &env,
        external_registry: &ExternalPackageRegistry::new(),
        string_table: &string_table,
        module_constants: &[],
    })
    .build()
    .expect("a trait record with one public incompatibility builds a draft");

    let trait_record = draft
        .declarations
        .iter()
        .find_map(|record| match &record.semantics {
            PublicDeclarationSemantics::Trait(semantics) => Some(semantics),
            _ => None,
        })
        .expect("the draft contains a trait record");

    assert_eq!(
        trait_record.incompatibilities,
        vec![CanonicalTraitIdentity::Source(trait_origin("Mark"))]
    );
}

// ---------------------------------------------------------------------------
//  Reusable evidence projection tests
// ---------------------------------------------------------------------------

/// Build a `ResolvedTraitDefinition` with one or more requirements, each carrying a dense
/// `TraitRequirementId` in authored order. The trait is source-visible and exported.
/// `start_requirement_id` is the dense `TraitRequirementId` the first requirement carries;
/// the remaining requirements increment from there in authored order. Production code lets
/// the `TraitEnvironment` allocate dense IDs; tests that need to vary the allocation use
/// this knob to keep the trait definition in lock-step with the dense requirement IDs the
/// corresponding evidence row supplies.
fn trait_definition(
    trait_id: TraitId,
    name: &str,
    this_type: TypeId,
    start_requirement_id: u32,
    requirement_specs: &[(&str, TypeId)],
    string_table: &mut StringTable,
) -> ResolvedTraitDefinition {
    let requirements = requirement_specs
        .iter()
        .enumerate()
        .map(
            |(index, (req_name, return_type))| ResolvedTraitRequirement {
                id: TraitRequirementId(start_requirement_id + index as u32),
                name: string_table.intern(req_name),
                name_location: default_location(),
                receiver: TraitReceiverRequirement::Immutable { this_type },
                parameters: vec![],
                returns: vec![ResolvedTraitReturn {
                    type_id: *return_type,
                    channel: ReturnChannel::Success,
                    location: default_location(),
                }],
                location: default_location(),
            },
        )
        .collect();

    ResolvedTraitDefinition {
        id: trait_id,
        name: string_table.intern(name),
        canonical_path: path(name, string_table),
        source_file: path(name, string_table),
        this_type,
        requirements,
        declaration_location: default_location(),
        visibility: TraitVisibility::Source { exported: true },
    }
}

/// Build a canonical `TraitEvidenceDefinition` mapping each requirement, in authored order,
/// to a receiver method path.
fn canonical_evidence(
    trait_id: TraitId,
    target_type_id: TypeId,
    requirement_method_paths: &[(TraitRequirementId, &str)],
    string_table: &mut StringTable,
) -> TraitEvidenceDefinition {
    let requirements = requirement_method_paths
        .iter()
        .map(|(requirement_id, method_name)| TraitRequirementEvidence {
            requirement_id: *requirement_id,
            method_path: path(method_name, string_table),
        })
        .collect();

    TraitEvidenceDefinition {
        id: TraitEvidenceId(0),
        kind: TraitEvidenceKind::Canonical,
        target_type_id,
        trait_id,
        source_file: InternedPath::new(),
        declaration_location: default_location(),
        requirements,
    }
}

/// Build a builtin `TraitEvidenceDefinition` for the cast-table ownership vocabulary test.
fn builtin_evidence(trait_id: TraitId, target_type_id: TypeId) -> TraitEvidenceDefinition {
    TraitEvidenceDefinition {
        id: TraitEvidenceId(0),
        kind: TraitEvidenceKind::Builtin,
        target_type_id,
        trait_id,
        source_file: InternedPath::new(),
        declaration_location: default_location(),
        requirements: vec![],
    }
}

/// Build the declared struct/choice declaration records the evidence projection joins
/// against, mirroring the declaration-centric join the builder uses.
///
/// WHAT: each entry becomes one `PublicDeclarationRecord` whose `receiver_methods` carry the
/// already-finalized `PublicReceiverMethodSemantics` values for that receiver. The
/// evidence projection joins its method-path keys against the
/// `(receiver_origin, defining_name)` pair on those records. The helper constructs stable
/// method origins directly as finalized test inputs and never builds a `ReceiverMethodCatalog`;
/// the production projection under test only consumes those origins.
fn declarations_with_receiver_methods(
    entries: &[(InternedPath, ReceiverKey)],
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Vec<PublicDeclarationRecord> {
    let mut by_receiver: FxHashMap<&InternedPath, Vec<PublicReceiverMethodSemantics>> =
        FxHashMap::default();
    let mut by_receiver_name: FxHashMap<&InternedPath, String> = FxHashMap::default();
    for (function_path, receiver) in entries {
        let receiver_path = match receiver {
            ReceiverKey::Struct(path) | ReceiverKey::Choice(path) => path,
            ReceiverKey::External(_) | ReceiverKey::BuiltinScalar(_) => {
                panic!("test helper only supports nominal receivers")
            }
        };
        let method_name = function_path
            .name_str(string_table)
            .expect("test method path must have a defining name")
            .to_owned();
        let receiver_origin = type_environment
            .nominal_id_for_path(receiver_path)
            .map(|_| {
                OriginTypeId::new(
                    module_origin(),
                    receiver_path
                        .name_str(string_table)
                        .expect("receiver path has defining name")
                        .to_owned(),
                    match receiver {
                        ReceiverKey::Struct(_) => OriginTypeCategory::Struct,
                        ReceiverKey::Choice(_) => OriginTypeCategory::Choice,
                        _ => unreachable!("non-nominal receivers rejected above"),
                    },
                )
            })
            .expect("receiver nominal must be registered for the test");
        let method_origin =
            OriginFunctionId::new_receiver(module_origin(), method_name, receiver_origin.clone());
        by_receiver
            .entry(receiver_path)
            .or_default()
            .push(PublicReceiverMethodSemantics {
                method_origin,
                parameters: vec![],
                returns: vec![],
                error_return: None,
            });
        by_receiver_name.insert(
            receiver_path,
            receiver_path.name_str(string_table).unwrap().to_owned(),
        );
    }

    by_receiver
        .into_iter()
        .map(|(receiver_path, receiver_methods)| {
            let name = by_receiver_name
                .get(receiver_path)
                .cloned()
                .unwrap_or_default();
            let category = if entries
                .iter()
                .any(|(_, key)| matches!(key, ReceiverKey::Choice(p) if p == receiver_path))
            {
                OriginTypeCategory::Choice
            } else {
                OriginTypeCategory::Struct
            };
            let origin = OriginTypeId::new(module_origin(), name, category);
            PublicDeclarationRecord {
                origin: OriginDeclarationId::Type(origin),
                semantics: PublicDeclarationSemantics::Struct(PublicStructSemantics {
                    generic_parameters: vec![],
                    fields: vec![],
                    receiver_methods,
                }),
            }
        })
        .collect()
}

/// Build the evidence projection context and call `project_reusable_evidence` directly,
/// bypassing the full draft builder so the evidence projection is the only unit under test.
///
/// The declarations passed in must already expose the public receiver-method origins the
/// evidence projection will join against; the helper no longer accepts a raw
/// `ReceiverMethodCatalog` because the projection must not iterate it.
#[allow(clippy::too_many_arguments)]
fn project_evidence(
    trait_environment: &TraitEnvironment,
    trait_evidence_environment: &TraitEvidenceEnvironment,
    declarations: &[PublicDeclarationRecord],
    nominal_origins: &FxHashMap<InternedPath, OriginTypeId>,
    trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<Vec<PublicEvidenceRecord>, CompilerError> {
    let registry = ExternalPackageRegistry::new();
    let nominal_resolver = TransientNominalOriginResolver::new(type_environment, nominal_origins);
    let generic_resolver = FoldedValueGenericParameterResolver;
    let projection_context =
        CanonicalTypeProjectionContext::new(&nominal_resolver, &generic_resolver, &registry);

    let context = EvidenceProjectionContext {
        trait_environment,
        trait_evidence_environment,
        public_source_nominal_type_origins: nominal_origins,
        public_source_trait_origins: trait_origins,
        type_environment,
        string_table,
        projection_context: &projection_context,
    };

    project_reusable_evidence(declarations, &context)
}

#[test]
fn evidence_identity_stable_across_local_allocations() {
    let mut string_table = StringTable::new();

    // Build two identical semantic configurations with different local allocation offsets.
    // The first uses TypeId, TraitId, TraitRequirementId and TraitEvidenceId starting from
    // their natural values. The second inserts dummy types, a dummy private trait with its
    // own requirement, and a dummy private-trait evidence row first so every dense local ID
    // differs while the retained public evidence is semantically identical.
    #[allow(clippy::type_complexity)]
    fn build_config(
        string_table: &mut StringTable,
        allocate_waste: bool,
    ) -> (
        TraitEnvironment,
        TraitEvidenceEnvironment,
        Vec<PublicDeclarationRecord>,
        FxHashMap<InternedPath, OriginTypeId>,
        FxHashMap<InternedPath, OriginTraitId>,
        TypeEnvironment,
    ) {
        let mut env = TypeEnvironment::new();

        if allocate_waste {
            let _waste_type = this_type(&mut env, string_table);
            let _ = register_struct(&mut env, string_table, "Waste", empty_fields(), None);
        }

        let this_id = this_type(&mut env, string_table);
        let (_, label_type_id) =
            register_struct(&mut env, string_table, "Label", empty_fields(), None);
        let _ = register_struct(&mut env, string_table, "Other", empty_fields(), None);

        let mut trait_env = TraitEnvironment::new();

        // When allocating waste, insert a coherent dummy private trait with one requirement
        // before the real trait. The real trait then receives a higher TraitId and its
        // requirement receives a higher TraitRequirementId, matching the TraitEnvironment
        // vector-index invariant.
        let trait_id = if allocate_waste {
            let dummy_this = this_type(&mut env, string_table);
            let dummy_definition = trait_definition(
                TraitId(0),
                "DummyPrivateTrait",
                dummy_this,
                0,
                &[("dummy_req", env.builtins().string)],
                string_table,
            );
            trait_env.insert(dummy_definition);
            TraitId(1)
        } else {
            TraitId(0)
        };

        // The dummy trait occupies requirement id 0; the real trait's display
        // requirement must start at id 1 so the four transient allocations
        // (TypeId, TraitId, TraitRequirementId, TraitEvidenceId) all
        // differ between the baseline and waste configurations.
        let real_start_requirement_id = if allocate_waste { 1 } else { 0 };
        let definition = trait_definition(
            trait_id,
            "DISPLAY_TEXT",
            this_id,
            real_start_requirement_id,
            &[("display", string_type_id(&env))],
            string_table,
        );
        trait_env.insert(definition);

        let display_path = path("display", string_table);
        let mut evidence_env = TraitEvidenceEnvironment::new();
        // The dummy private trait's evidence uses requirement id 0. The retained
        // evidence entry uses the real trait's authored requirement id, which is
        // real_start_requirement_id, so its TraitEvidenceId differs from the
        // baseline configuration.
        if allocate_waste {
            evidence_env.insert_validated(canonical_evidence(
                TraitId(0),
                label_type_id,
                &[(TraitRequirementId(0), "dummy_req")],
                string_table,
            ));
        }
        evidence_env.insert_validated(canonical_evidence(
            trait_id,
            label_type_id,
            &[(TraitRequirementId(real_start_requirement_id), "display")],
            string_table,
        ));

        let declarations = declarations_with_receiver_methods(
            &[(
                display_path,
                ReceiverKey::Struct(path("Label", string_table)),
            )],
            &env,
            string_table,
        );

        let mut nominal_origins = FxHashMap::default();
        nominal_origins.insert(path("Label", string_table), struct_origin("Label"));
        nominal_origins.insert(path("Other", string_table), struct_origin("Other"));

        let mut trait_origins = FxHashMap::default();
        trait_origins.insert(
            path("DISPLAY_TEXT", string_table),
            trait_origin("DISPLAY_TEXT"),
        );

        (
            trait_env,
            evidence_env,
            declarations,
            nominal_origins,
            trait_origins,
            env,
        )
    }

    let config_a = build_config(&mut string_table, false);
    let config_b = build_config(&mut string_table, true);

    let evidence_a = project_evidence(
        &config_a.0,
        &config_a.1,
        &config_a.2,
        &config_a.3,
        &config_a.4,
        &config_a.5,
        &string_table,
    )
    .expect("evidence projection A should succeed");

    let evidence_b = project_evidence(
        &config_b.0,
        &config_b.1,
        &config_b.2,
        &config_b.3,
        &config_b.4,
        &config_b.5,
        &string_table,
    )
    .expect("evidence projection B should succeed");

    assert_eq!(evidence_a.len(), 1);
    assert_eq!(evidence_b.len(), 1);

    let record_a = &evidence_a[0];
    let record_b = &evidence_b[0];

    assert_eq!(record_a.identity, record_b.identity);
    assert_eq!(record_a.ownership, record_b.ownership);
    assert_eq!(
        record_a.requirement_mappings.len(),
        record_b.requirement_mappings.len()
    );
    assert_eq!(
        record_a.requirement_mappings[0].requirement_identity,
        record_b.requirement_mappings[0].requirement_identity
    );
    assert_eq!(
        record_a.requirement_mappings[0].method_origin,
        record_b.requirement_mappings[0].method_origin
    );

    // Capture a small snapshot of the four dense local identity classes that drive
    // stable identity so the test proves the baseline and waste configurations
    // genuinely differ in every one of them. The four values are looked up from
    // the existing evidence-environment index and the trait/type environments
    // rather than from a new production getter.
    fn snapshot(
        trait_env: &TraitEnvironment,
        evidence_env: &TraitEvidenceEnvironment,
        env: &TypeEnvironment,
        label_path: &InternedPath,
        real_trait_canonical_path: &InternedPath,
    ) -> (TypeId, TraitId, TraitRequirementId, TraitEvidenceId) {
        let label_type_id = env
            .type_id_for_nominal_id(
                env.nominal_id_for_path(label_path)
                    .expect("Label nominal id"),
            )
            .expect("Label type id");
        let real_trait_id = trait_env
            .id_for_path(real_trait_canonical_path)
            .expect("real trait id");
        let definition = trait_env.get(real_trait_id).expect("real trait definition");
        let real_requirement_id = definition.requirements[0].id;
        let evidence_id = evidence_env
            .canonical_for(label_type_id, real_trait_id)
            .expect("real evidence id");
        (
            label_type_id,
            real_trait_id,
            real_requirement_id,
            evidence_id,
        )
    }

    let label_path = path("Label", &mut string_table);
    let trait_path = path("DISPLAY_TEXT", &mut string_table);
    let (type_id_a, trait_id_a, requirement_id_a, evidence_id_a) = snapshot(
        &config_a.0,
        &config_a.1,
        &config_a.5,
        &label_path,
        &trait_path,
    );
    let (type_id_b, trait_id_b, requirement_id_b, evidence_id_b) = snapshot(
        &config_b.0,
        &config_b.1,
        &config_b.5,
        &label_path,
        &trait_path,
    );
    assert_ne!(
        type_id_a, type_id_b,
        "TypeId must differ between configurations"
    );
    assert_ne!(
        trait_id_a, trait_id_b,
        "TraitId must differ between configurations"
    );
    assert_ne!(
        requirement_id_a, requirement_id_b,
        "TraitRequirementId must differ between configurations"
    );
    assert_ne!(
        evidence_id_a, evidence_id_b,
        "TraitEvidenceId must differ between configurations"
    );
}

fn string_type_id(env: &TypeEnvironment) -> TypeId {
    env.builtins().string
}

#[test]
fn evidence_requirement_mappings_preserve_authored_order_and_exact_receiver_origins() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "NAMED",
        this_id,
        0,
        &[("name", env.builtins().string), ("id", env.builtins().int)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        label_type_id,
        &[
            // Deliberately reverse the evidence requirement vector so the output proves
            // trait-definition authored order, not accidental evidence-vector order.
            (TraitRequirementId(1), "id"),
            (TraitRequirementId(0), "name"),
        ],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let name_path = path("name", &mut string_table);
    let id_path = path("id", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[
            (name_path.clone(), ReceiverKey::Struct(label_path.clone())),
            (id_path.clone(), ReceiverKey::Struct(label_path.clone())),
        ],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("NAMED", &mut string_table), trait_origin("NAMED"));

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("draft with two-requirement evidence should build");

    assert_eq!(evidence.len(), 1);
    let record = &evidence[0];

    assert_eq!(record.requirement_mappings.len(), 2);
    assert_eq!(
        record.requirement_mappings[0]
            .requirement_identity
            .requirement_name(),
        "name"
    );
    assert_eq!(
        record.requirement_mappings[1]
            .requirement_identity
            .requirement_name(),
        "id"
    );

    let expected_name_origin =
        OriginFunctionId::new_receiver(module_origin(), "name".to_owned(), struct_origin("Label"));
    let expected_id_origin =
        OriginFunctionId::new_receiver(module_origin(), "id".to_owned(), struct_origin("Label"));
    assert_eq!(
        record.requirement_mappings[0].method_origin,
        expected_name_origin
    );
    assert_eq!(
        record.requirement_mappings[1].method_origin,
        expected_id_origin
    );
}

#[test]
fn evidence_excludes_private_target_and_retains_public_target() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, public_type_id) =
        register_struct(&mut env, &mut string_table, "Public", empty_fields(), None);
    let (_, private_type_id) =
        register_struct(&mut env, &mut string_table, "Private", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "DISPLAY_TEXT",
        this_id,
        0,
        &[("display", env.builtins().string)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        public_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        private_type_id,
        &[(TraitRequirementId(0), "display_private")],
        &mut string_table,
    ));

    let public_path = path("Public", &mut string_table);
    let display_path = path("display", &mut string_table);
    let display_private_path = path("display_private", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[
            (
                display_path.clone(),
                ReceiverKey::Struct(public_path.clone()),
            ),
            (
                display_private_path,
                ReceiverKey::Struct(path("Private", &mut string_table)),
            ),
        ],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(public_path.clone(), struct_origin("Public"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(
        path("DISPLAY_TEXT", &mut string_table),
        trait_origin("DISPLAY_TEXT"),
    );

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("draft with mixed public/private evidence should build");

    assert_eq!(evidence.len(), 1);
    let record = &evidence[0];
    assert_eq!(
        record.identity,
        CanonicalEvidenceIdentity::new(
            CanonicalTypeIdentity::SourceNominal(struct_origin("Public")),
            CanonicalTraitIdentity::Source(trait_origin("DISPLAY_TEXT")),
        )
    );
}

#[test]
fn evidence_excludes_private_source_trait() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, target_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "PrivateTrait",
        this_id,
        0,
        &[("display", env.builtins().string)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        target_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let display_path = path("display", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(
            display_path.clone(),
            ReceiverKey::Struct(label_path.clone()),
        )],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    // The trait is NOT in the public source-trait origin index, so it is private.
    let trait_origins = FxHashMap::default();

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("draft with private-trait evidence should build");

    assert_eq!(evidence.len(), 0);
}

#[test]
fn evidence_excludes_builtin_from_direct_module_draft() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let (_, target_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let mut trait_env = TraitEnvironment::new();

    // Register a core trait so the trait environment has a definition for the builtin evidence.
    let string_type = env.builtins().string;
    trait_env.register_core_trait(
        &mut env,
        &mut string_table,
        "CASTABLE_TO_STRING",
        "to_string",
        string_type,
        None,
    );

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_builtin(builtin_evidence(trait_id, target_type_id));

    let label_path = path("Label", &mut string_table);
    let nominal_origins = FxHashMap::default();
    let mut nominal_origins = nominal_origins;
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &[],
        &nominal_origins,
        &FxHashMap::default(),
        &env,
        &string_table,
    )
    .expect("evidence projection with builtin evidence should succeed");

    assert_eq!(
        evidence.len(),
        0,
        "builtin evidence must not be duplicated into a direct module draft"
    );
}

#[test]
fn evidence_retains_source_canonical_core_trait_evidence() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let mut trait_env = TraitEnvironment::new();

    // Register the compiler-owned DISPLAYABLE core trait. The trait environment records
    // its CoreTraitKind so core_trait_kind resolves it without the partial root-table map.
    let displayable_id = trait_env.register_core_displayable(&mut env, &mut string_table);

    // Source-authored canonical evidence: the public Label target conforms to the core
    // DISPLAYABLE trait. This must be retained, not excluded, even though the core trait
    // has no entry in public_source_trait_origins or trait_source_facts.
    let display_path = path("display", &mut string_table);
    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        displayable_id,
        label_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(
            display_path.clone(),
            ReceiverKey::Struct(label_path.clone()),
        )],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    // The core trait is not in the public source-trait origin index, but core traits are
    // always consumer-visible so the evidence is retained.
    let trait_origins = FxHashMap::default();

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("source canonical evidence for a core trait should be retained");

    assert_eq!(evidence.len(), 1);
    let record = &evidence[0];
    assert_eq!(
        record.identity,
        CanonicalEvidenceIdentity::new(
            CanonicalTypeIdentity::SourceNominal(struct_origin("Label")),
            CanonicalTraitIdentity::Core(CanonicalCoreTraitIdentity::Displayable),
        )
    );
    assert_eq!(record.ownership, PublicEvidenceOwnership::SourceCanonical);
    assert_eq!(record.requirement_mappings.len(), 1);
    assert_eq!(
        record.requirement_mappings[0]
            .requirement_identity
            .requirement_name(),
        "display"
    );
    let expected_origin = OriginFunctionId::new_receiver(
        module_origin(),
        "display".to_owned(),
        struct_origin("Label"),
    );
    assert_eq!(
        record.requirement_mappings[0].method_origin,
        expected_origin
    );
}

#[test]
fn evidence_rejects_duplicate_stable_keys() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "DISPLAY_TEXT",
        this_id,
        0,
        &[("display", env.builtins().string)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    let evidence = canonical_evidence(
        trait_id,
        label_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    );
    evidence_env.insert_validated(evidence.clone());
    evidence_env.insert_validated(evidence);

    let label_path = path("Label", &mut string_table);
    let display_path = path("display", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(
            display_path.clone(),
            ReceiverKey::Struct(label_path.clone()),
        )],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(
        path("DISPLAY_TEXT", &mut string_table),
        trait_origin("DISPLAY_TEXT"),
    );

    let result = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("duplicate evidence keys must be rejected")
        .msg;
    assert!(
        message.contains("target-plus-trait identity"),
        "expected a duplicate-key rejection, got: {message}"
    );
}

#[test]
fn evidence_rejects_core_trait_without_classifier() {
    // A trait definition with `TraitVisibility::Core` but no `CoreTraitKind` is malformed
    // compiler metadata. The projection must reject it as a `CompilerError` rather than
    // silently fall through to source-path visibility.
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let mut trait_env = TraitEnvironment::new();
    // Build a `Core` trait definition without recording any `CoreTraitKind`. The
    // `TraitEnvironment` allows raw inserts, so this test exercises the malformed
    // combination directly without expanding the public surface.
    let definition = ResolvedTraitDefinition {
        id: TraitId(0),
        name: string_table.intern("UnrecordedCore"),
        canonical_path: path("UnrecordedCore", &mut string_table),
        source_file: path("UnrecordedCore", &mut string_table),
        this_type: this_id,
        requirements: vec![ResolvedTraitRequirement {
            id: TraitRequirementId(0),
            name: string_table.intern("display"),
            name_location: default_location(),
            receiver: TraitReceiverRequirement::Immutable { this_type: this_id },
            parameters: vec![],
            returns: vec![ResolvedTraitReturn {
                type_id: env.builtins().string,
                channel: ReturnChannel::Success,
                location: default_location(),
            }],
            location: default_location(),
        }],
        declaration_location: default_location(),
        visibility: TraitVisibility::Core,
    };
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    let display_path = path("display", &mut string_table);
    evidence_env.insert_validated(canonical_evidence(
        TraitId(0),
        label_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(
            display_path.clone(),
            ReceiverKey::Struct(label_path.clone()),
        )],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));
    // No public source-trait origin and no `CoreTraitKind`; the projection must fail.
    let trait_origins = FxHashMap::default();

    let result = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a core trait without a CoreTraitKind must be rejected")
        .msg;
    assert!(
        message.contains("no recorded `CoreTraitKind`"),
        "expected a core-without-classifier rejection, got: {message}"
    );
}

#[test]
fn evidence_rejects_requirement_count_mismatch() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "NAMED",
        this_id,
        0,
        &[("name", env.builtins().string), ("id", env.builtins().int)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    // Evidence has only one requirement but the trait definition has two.
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        label_type_id,
        &[(TraitRequirementId(0), "name")],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let name_path = path("name", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(name_path.clone(), ReceiverKey::Struct(label_path.clone()))],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(path("NAMED", &mut string_table), trait_origin("NAMED"));

    let result = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a requirement count mismatch must be rejected")
        .msg;
    assert!(
        message.contains("count mismatch"),
        "expected a count-mismatch rejection, got: {message}"
    );
}

#[test]
fn evidence_rejects_missing_receiver_origin() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "DISPLAY_TEXT",
        this_id,
        0,
        &[("display", env.builtins().string)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        label_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));

    // The joined declaration records carry no receiver methods, so the method path has
    // no matching public receiver-method origin on the Label receiver.
    let declarations: Vec<PublicDeclarationRecord> = Vec::new();

    let label_path = path("Label", &mut string_table);
    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(
        path("DISPLAY_TEXT", &mut string_table),
        trait_origin("DISPLAY_TEXT"),
    );

    let result = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    );

    let message = result
        .expect_err("a missing receiver origin must be rejected")
        .msg;
    assert!(
        message.contains("no matching public receiver-method origin"),
        "expected a missing-origin rejection, got: {message}"
    );
}

#[test]
fn evidence_ownership_is_source_canonical_for_direct_drafts() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();

    let this_id = this_type(&mut env, &mut string_table);
    let (_, label_type_id) =
        register_struct(&mut env, &mut string_table, "Label", empty_fields(), None);

    let trait_id = TraitId(0);
    let definition = trait_definition(
        trait_id,
        "DISPLAY_TEXT",
        this_id,
        0,
        &[("display", env.builtins().string)],
        &mut string_table,
    );
    let mut trait_env = TraitEnvironment::new();
    trait_env.insert(definition);

    let mut evidence_env = TraitEvidenceEnvironment::new();
    evidence_env.insert_validated(canonical_evidence(
        trait_id,
        label_type_id,
        &[(TraitRequirementId(0), "display")],
        &mut string_table,
    ));

    let label_path = path("Label", &mut string_table);
    let display_path = path("display", &mut string_table);
    let declarations = declarations_with_receiver_methods(
        &[(
            display_path.clone(),
            ReceiverKey::Struct(label_path.clone()),
        )],
        &env,
        &string_table,
    );

    let mut nominal_origins = FxHashMap::default();
    nominal_origins.insert(label_path.clone(), struct_origin("Label"));

    let mut trait_origins = FxHashMap::default();
    trait_origins.insert(
        path("DISPLAY_TEXT", &mut string_table),
        trait_origin("DISPLAY_TEXT"),
    );

    let evidence = project_evidence(
        &trait_env,
        &evidence_env,
        &declarations,
        &nominal_origins,
        &trait_origins,
        &env,
        &string_table,
    )
    .expect("draft with source-canonical evidence should build");

    assert_eq!(evidence.len(), 1);
    assert_eq!(
        evidence[0].ownership,
        PublicEvidenceOwnership::SourceCanonical
    );
}

// ---------------------------------------------------------------------------
//  Folded-value projection tests live in a focused sibling module
// ---------------------------------------------------------------------------

#[cfg(test)]
mod folded_value_tests;
