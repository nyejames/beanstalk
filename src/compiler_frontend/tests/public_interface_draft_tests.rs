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
use crate::compiler_frontend::ast::statements::functions::ReturnChannel;
use crate::compiler_frontend::ast::{
    ResolvedPublicTraitRoot, ResolvedPublicTypeRootTable, ResolvedTraitParameterFact,
    ResolvedTraitReceiverFact, ResolvedTraitRequirementFact, ResolvedTraitReturnFact,
    TraitReceiverAccessKind,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalBuiltinType, CanonicalTypeIdentity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::{FieldDefinition, StructTypeDefinition};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, TypeId};
use crate::compiler_frontend::defined_public_export_origins::DefinedPublicExportOriginDraft;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::public_interface_draft::{
    DefinedPublicTraitReceiverAccess, DefinedPublicTraitSurface, PublicInterfaceDraftBuilder,
    PublicInterfaceDraftBuilderInput, TraitSurfaceTypeIdentity, build_trait_surfaces,
};
use crate::compiler_frontend::semantic_identity::{
    ExportBinding, ModuleRootRole, OriginDeclarationId, OriginTraitId, OriginTypeCategory,
    OriginTypeId, StableModuleOriginIdentity, StablePackageIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
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
        DefinedPublicTraitReceiverAccess::Immutable
    );
    assert_eq!(&surface.requirements[1].name, "write");
    assert_eq!(
        surface.requirements[1].receiver_access,
        DefinedPublicTraitReceiverAccess::Mutable
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
    let (_, widget_id) = register_struct(&mut env, &mut string_table, "Widget");

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
    assert!(message.contains("no matching export binding"));
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
//  Orchestration: the builder carries exactly one aggregate draft
// ---------------------------------------------------------------------------

#[test]
fn builder_produces_one_aggregate_draft_with_three_components() {
    let mut string_table = StringTable::new();
    let mut env = TypeEnvironment::new();
    let this_id = this_type(&mut env, &mut string_table);

    let root = trait_root("Shape", this_id, vec![], &mut string_table);
    let binding = trait_binding("Shape");
    let trait_origins =
        trait_origins_map(vec![("Shape", trait_origin("Shape"))], &mut string_table);

    let export_origin_draft =
        DefinedPublicExportOriginDraft::new(module_origin(), vec![binding], FxHashMap::default());

    let projection_input = AstPublicInterfaceProjectionInput {
        root_table: ResolvedPublicTypeRootTable {
            roots: vec![],
            receiver_methods: vec![],
            trait_source_facts: FxHashMap::default(),
        },
        trait_roots: vec![root],
        receiver_catalog: Some(std::rc::Rc::new(ReceiverMethodCatalog::default())),
    };

    let registry = ExternalPackageRegistry::new();
    let draft = PublicInterfaceDraftBuilder::new(PublicInterfaceDraftBuilderInput {
        export_origin_draft,
        public_interface_projection_input: projection_input,
        public_source_nominal_type_origins: &FxHashMap::default(),
        public_source_trait_origins: &trait_origins,
        type_environment: &env,
        external_registry: &registry,
        string_table: &string_table,
    })
    .build()
    .expect("aggregate draft builds");

    // The draft carries exactly one of each projection component.
    assert_eq!(draft.export_origins.export_bindings().len(), 1);
    assert_eq!(draft.trait_surfaces.len(), 1);
    assert_eq!(draft.trait_surfaces[0].origin, trait_origin("Shape"));
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
