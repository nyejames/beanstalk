//! The one aggregate pre-HIR public-semantic handoff for a compiled module.
//!
//! WHAT: owns the [`PublicInterfaceDraftBuilder`] and the [`PublicInterfaceDraft`] it produces.
//! The draft is the sole pre-HIR public-semantic handoff that crosses the semantic compilation
//! boundary. It is declaration-centric: one [`PublicDeclarationRecord`] per stable
//! [`OriginDeclarationId`], carrying a closed [`PublicDeclarationSemantics`] enum that
//! distinguishes free functions, structs, choices, transparent aliases, constants and traits.
//! Receiver methods are attached to their owning struct or choice record, not stored as a
//! top-level parallel vector. Direct [`ExportBinding`] values remain distinct from declaration
//! records so future re-exports can add bindings without changing donor origins.
//!
//! The builder internalizes three projection components as private builder steps:
//! - the accepted direct export-origin projection ([`DefinedPublicExportOrigins`]),
//! - the accepted canonical type-surface projection ([`DefinedPublicTypeSurface`]),
//! - the corrected direct trait-contract projection ([`DefinedPublicTraitSurface`]).
//!
//! These intermediates are consumed before the draft boundary: the draft stores only `Public*`
//! semantic leaf types. `DefinedPublic*` aggregate projection containers are transient and are
//! consumed by the join. The join validates every category/origin pairing and rejects missing,
//! duplicate, extra or mismatched facts through [`CompilerError`].
//!
//! WHY: the compiler design overview and the recovery plan require one aggregate producer
//! boundary with a declaration-centric shape instead of parallel `DefinedPublic*` fields that
//! every later phase would have to rejoin. Keeping the three projections behind one builder
//! preserves their proven, total projection logic while ensuring only one draft crosses
//! orchestration.
//!
//! ## Trait-contract projection
//!
//! Direct trait surfaces are keyed by the exact matching direct [`OriginTraitId`] export
//! binding, preserve authored requirement order, and reuse existing [`ValueMode`] and
//! [`ReturnChannel`] facts. Requirement receiver `this_type` is validated against the owning
//! trait `this_type` before mapping immutable or mutable receiver access; a mismatch is a
//! `CompilerError`. Direct parameter or return occurrences of that exact `this_type` become a
//! trait-local [`TraitSurfaceTypeIdentity::SelfType`]; every other `TypeId` projects through
//! the existing canonical type projection as a [`TraitSurfaceTypeIdentity::Concrete`]. Publicly
//! authored incompatibilities project through the same source/core trait-identity owner used by
//! generic bounds. No unscoped self type is added to [`CanonicalTypeIdentity`].
//!
//! Boundary: the draft is private to compiler/build orchestration and never reaches backends.
//! It is not the final `PublicSemanticInterface`.

use crate::compiler_frontend::ast::AstPublicInterfaceProjectionInput;
use crate::compiler_frontend::ast::ResolvedTraitSourceFact;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::ReturnChannel;
use crate::compiler_frontend::ast::{
    ResolvedPublicTraitRoot, ResolvedTraitRequirementFact, TraitReceiverAccessKind,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalTraitIdentity, CanonicalTypeIdentity, CanonicalTypeProjectionContext,
    ExportedGenericParameterIdentity, GenericParameterOriginResolver,
    project_type_id_to_canonical_identity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterId, TypeId};
use crate::compiler_frontend::defined_public_export_origins::DefinedPublicExportOriginDraft;
use crate::compiler_frontend::defined_public_type_surface::{
    DefinedPublicAliasTypeSurface, DefinedPublicConstantTypeSurface,
    DefinedPublicFunctionTypeSurface, DefinedPublicNominalTypeSurface,
    DefinedPublicReceiverMethodTypeSurface, DefinedPublicTypeSurface, PublicChoiceVariantSurface,
    PublicFieldTypeSlot, PublicGenericParameterSurface, PublicParameterTypeSlot,
    PublicReturnTypeSlot, TransientNominalOriginResolver, build_defined_public_type_surface,
    project_trait_source_fact_to_canonical_identity,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::folded_value::{
    FoldedValueGenericParameterResolver, PublicFoldedValue, convert_expression_to_folded_value,
};
use crate::compiler_frontend::semantic_identity::{
    ExportBinding, OriginDeclarationId, OriginFunctionId, OriginTraitId, OriginTypeCategory,
    OriginTypeId, StableModuleOriginIdentity,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::traits::ids::TraitId;
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::{FxHashMap, FxHashSet};

// ===========================================================================
//  Trait surface value types
// ===========================================================================

/// Trait-local vocabulary for one type identity in a trait requirement signature.
///
/// WHAT: a trait requirement parameter or return type is either the trait self type
/// (`SelfType`) or an ordinary projected canonical type (`Concrete`). The self marker is
/// trait-local: it never enters the unscoped [`CanonicalTypeIdentity`] vocabulary, so an
/// unrelated local `TypeId` cannot be misclassified as trait self.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TraitSurfaceTypeIdentity {
    SelfType,
    Concrete(Box<CanonicalTypeIdentity>),
}

/// Required receiver access for one trait requirement, stored separately from the self type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PublicTraitReceiverAccess {
    Immutable,
    Mutable,
}

/// One non-receiver parameter in a trait requirement surface.
///
/// `name` is the owned authored parameter name, or `None` when the source signature omits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicTraitRequirementParameter {
    name: Option<String>,
    value_mode: ValueMode,
    type_identity: TraitSurfaceTypeIdentity,
}

/// One return slot in a trait requirement surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicTraitRequirementReturn {
    channel: ReturnChannel,
    type_identity: TraitSurfaceTypeIdentity,
}

/// One method requirement in a trait surface, in authored order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicTraitRequirementSurface {
    name: String,
    receiver_access: PublicTraitReceiverAccess,
    parameters: Vec<PublicTraitRequirementParameter>,
    returns: Vec<PublicTraitRequirementReturn>,
}

/// The trait surface for one exported trait authored directly in the active module root.
///
/// Keyed by the exact matching direct [`OriginTraitId`] export binding. Requirements preserve
/// authored order, and `incompatibilities` carries the publicly-authored `must not` relations
/// involving this trait as stable, ordered, duplicate-free [`CanonicalTraitIdentity`] values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTraitSurface {
    origin: OriginTraitId,
    requirements: Vec<PublicTraitRequirementSurface>,
    incompatibilities: Vec<CanonicalTraitIdentity>,
}

// ===========================================================================
//  Declaration-centric record value types
// ===========================================================================

/// The closed semantic category for one public declaration record.
///
/// WHAT: a distinct variant per directly-defined public declaration category. Struct and choice
/// are separate variants so nominal meaning is never implicit in empty field/variant vectors.
/// Each variant carries only the semantic facts already produced at R1; folded constant
/// values are owned by the constant variant. Evidence, provenance, borrow/effect summaries,
/// generic template bodies and re-exports remain outside this enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PublicDeclarationSemantics {
    Function(PublicFunctionSemantics),
    Struct(PublicStructSemantics),
    Choice(PublicChoiceSemantics),
    TransparentAlias(PublicAliasSemantics),
    Constant(PublicConstantSemantics),
    Trait(PublicTraitSemantics),
}

/// The semantic facts for one exported free function: generic parameters/bounds, parameter
/// types, success returns and error return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicFunctionSemantics {
    pub(crate) generic_parameters: Vec<PublicGenericParameterSurface>,
    pub(crate) parameters: Vec<PublicParameterTypeSlot>,
    pub(crate) returns: Vec<PublicReturnTypeSlot>,
    pub(crate) error_return: Option<CanonicalTypeIdentity>,
}

/// The semantic facts for one exported receiver method, attached to its owning struct or choice
/// declaration record. The receiver origin is the parent record's origin and is not repeated here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicReceiverMethodSemantics {
    pub(crate) method_origin: OriginFunctionId,
    pub(crate) parameters: Vec<PublicParameterTypeSlot>,
    pub(crate) returns: Vec<PublicReturnTypeSlot>,
    pub(crate) error_return: Option<CanonicalTypeIdentity>,
}

/// The semantic facts for one exported nominal struct: generic parameters/bounds, fields and
/// receiver methods attached to this struct's surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicStructSemantics {
    pub(crate) generic_parameters: Vec<PublicGenericParameterSurface>,
    pub(crate) fields: Vec<PublicFieldTypeSlot>,
    pub(crate) receiver_methods: Vec<PublicReceiverMethodSemantics>,
}

/// The semantic facts for one exported nominal choice: generic parameters/bounds, variants and
/// receiver methods attached to this choice's surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicChoiceSemantics {
    pub(crate) generic_parameters: Vec<PublicGenericParameterSurface>,
    pub(crate) variants: Vec<PublicChoiceVariantSurface>,
    pub(crate) receiver_methods: Vec<PublicReceiverMethodSemantics>,
}

/// The semantic facts for one exported transparent alias: the resolved target type identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicAliasSemantics {
    pub(crate) target_type_identity: CanonicalTypeIdentity,
}

/// The semantic facts for one exported constant: the canonical type identity and the owned
/// fully folded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicConstantSemantics {
    pub(crate) type_identity: CanonicalTypeIdentity,
    pub(crate) folded_value: PublicFoldedValue,
}

/// The semantic facts for one exported trait: its ordered requirements with receiver access,
/// parameter modes/types and return channels/types, plus the ordered, duplicate-free
/// canonical identities of the publicly-authored traits this trait must not be claimed
/// alongside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicTraitSemantics {
    pub(crate) requirements: Vec<PublicTraitRequirementSurface>,
    pub(crate) incompatibilities: Vec<CanonicalTraitIdentity>,
}

/// One declaration-centric record in the public interface draft.
///
/// WHAT: carries exactly one stable [`OriginDeclarationId`] and its closed
/// [`PublicDeclarationSemantics`]. The builder produces one record per stable origin in the
/// deterministic export-binding order, with receiver methods deterministically attached to
/// their owning struct or choice record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicDeclarationRecord {
    pub(crate) origin: OriginDeclarationId,
    pub(crate) semantics: PublicDeclarationSemantics,
}

/// The one aggregate pre-HIR public-semantic handoff for one compiled module.
///
/// WHAT: owns the owning [`StableModuleOriginIdentity`] (even when the module exports nothing),
/// the deterministic [`ExportBinding`] values distinct from declaration records, and one
/// [`PublicDeclarationRecord`] per stable [`OriginDeclarationId`]. It carries only owned stable
/// values: no donor-local `TypeId`, `NominalTypeId`, `GenericParameterId`, `TraitId`,
/// `InternedPath` or `StringId` crosses this boundary.
///
/// It is deliberately not the final `PublicSemanticInterface`. Generic template bodies,
/// evidence, access/effect summaries, provenance, re-export interfaces and cross-module call
/// lowering remain for later phases. Folded constant values are owned by each constant
/// declaration record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicInterfaceDraft {
    pub(crate) module_origin: StableModuleOriginIdentity,
    pub(crate) export_bindings: Vec<ExportBinding>,
    pub(crate) declarations: Vec<PublicDeclarationRecord>,
}

// ===========================================================================
//  Builder
// ===========================================================================

/// Named inputs for [`PublicInterfaceDraftBuilder`].
///
/// WHAT: groups the pre-AST export-origin draft, the post-AST public-interface projection
/// input and the shared projection side tables into one construction value so the builder
/// does not take a long positional parameter list.
/// WHY: keeping the inputs named makes the construction boundary easier to audit than seven
/// positional arguments.
pub(crate) struct PublicInterfaceDraftBuilderInput<'a> {
    pub export_origin_draft: DefinedPublicExportOriginDraft,
    pub public_interface_projection_input: AstPublicInterfaceProjectionInput,
    pub public_source_nominal_type_origins: &'a FxHashMap<InternedPath, OriginTypeId>,
    pub public_source_trait_origins: &'a FxHashMap<InternedPath, OriginTraitId>,
    pub type_environment: &'a TypeEnvironment,
    pub external_registry: &'a ExternalPackageRegistry,
    pub string_table: &'a StringTable,
    /// The finalized and normalized module constant declarations from the AST.
    ///
    /// WHAT: the already-folded `Ast::module_constants` consumed before HIR lowering. Each
    /// entry carries the constant's exact defining `InternedPath` and its fully folded
    /// expression. The draft builder joins each public constant surface to a finalized module
    /// constant by that exact defining path and converts the expression to an owned
    /// [`PublicFoldedValue`].
    pub module_constants: &'a [Declaration],
}

/// Builds the one aggregate [`PublicInterfaceDraft`] from already-resolved pre-HIR facts.
///
/// WHAT: the sole construction path for the draft. It internalizes the export-origin
/// finalization, the canonical type-surface projection and the corrected trait-requirement
/// projection as private builder steps, so no parallel `DefinedPublic*` producer result
/// crosses orchestration. It consumes the pre-AST export-origin draft, the consolidated AST
/// public-interface projection input and the transient expanded public source-nominal and source-trait
/// origin indexes, while both the `TypeEnvironment` and `ExternalPackageRegistry` are still
/// available. The output is retained only on overall semantic success.
pub(crate) struct PublicInterfaceDraftBuilder<'a> {
    input: PublicInterfaceDraftBuilderInput<'a>,
}

impl<'a> PublicInterfaceDraftBuilder<'a> {
    /// Construct the builder from one named input value.
    ///
    /// Compiler-internal: the frontend orchestration constructs this once per module
    /// compilation, after AST construction succeeds and before HIR lowering consumes the AST.
    pub(crate) fn new(input: PublicInterfaceDraftBuilderInput<'a>) -> Self {
        Self { input }
    }

    /// Build the aggregate draft.
    ///
    /// WHAT: runs the three internal projection steps in order, then joins their intermediates
    /// into one declaration-centric record per stable origin:
    /// 1. finalize the export-origin component from the receiver catalog,
    /// 2. build the canonical type surface from the resolved root table,
    /// 3. build the corrected trait surfaces from the resolved trait roots,
    /// 4. join all three into [`PublicDeclarationRecord`] values, attaching receiver methods to
    ///    their owning struct or choice record and joining each constant export binding to its
    ///    finalized module constant folded value.
    ///
    /// Each step is total: a missing, duplicate, unmatched or malformed fact is a
    /// `CompilerError` rather than a silent omission. The intermediates are consumed before the
    /// draft boundary: the draft never stores a `DefinedPublic*` component.
    pub(crate) fn build(self) -> Result<PublicInterfaceDraft, CompilerError> {
        let PublicInterfaceDraftBuilderInput {
            export_origin_draft,
            public_interface_projection_input,
            public_source_nominal_type_origins,
            public_source_trait_origins,
            type_environment,
            external_registry,
            string_table,
            module_constants,
        } = self.input;

        let AstPublicInterfaceProjectionInput {
            root_table,
            trait_roots,
            receiver_catalog,
        } = public_interface_projection_input;

        let receiver_catalog = receiver_catalog.ok_or_else(|| {
            CompilerError::compiler_error(
                "public-interface draft construction: AST finalization did not retain its \
                 resolved receiver-method catalog; the export-origin component cannot finalize \
                 receiver surface origins without it",
            )
        })?;

        let export_origins = export_origin_draft.finalize(&receiver_catalog, string_table)?;

        let type_surface = build_defined_public_type_surface(
            &root_table,
            &export_origins,
            public_source_nominal_type_origins,
            public_source_trait_origins,
            type_environment,
            external_registry,
            string_table,
        )?;

        let trait_surfaces = build_trait_surfaces(
            &trait_roots,
            export_origins.export_bindings(),
            &root_table.trait_source_facts,
            public_source_nominal_type_origins,
            public_source_trait_origins,
            type_environment,
            external_registry,
            string_table,
        )?;

        // Build the folded-value projection context from the same nominal origin resolver
        // and external registry already used by the type-surface and trait-surface
        // projections. Folded constant values are concrete, so a generic parameter reaching
        // this projection is an internal invariant violation rather than a legitimate
        // exported shape.
        let nominal_resolver = TransientNominalOriginResolver::new(
            type_environment,
            public_source_nominal_type_origins,
        );
        let generic_resolver = FoldedValueGenericParameterResolver;
        let projection_context = CanonicalTypeProjectionContext::new(
            &nominal_resolver,
            &generic_resolver,
            external_registry,
        );
        let folded_value_context = FoldedValueJoinContext {
            module_constants,
            type_environment,
            string_table,
            projection_context: &projection_context,
        };

        // Consume the export-origin component after the borrowing projections finish.
        // The module origin and export bindings move into the draft; the receiver surfaces
        // were already projected into the type surface and are dropped here.
        let (module_origin, export_bindings) =
            export_origins.into_module_origin_and_export_bindings();
        let declarations = join_declaration_records(
            &export_bindings,
            type_surface,
            trait_surfaces,
            &folded_value_context,
        )?;

        Ok(PublicInterfaceDraft {
            module_origin,
            export_bindings,
            declarations,
        })
    }
}

// ===========================================================================
//  Folded-value projection context
// ===========================================================================

/// Context for projecting folded constant values during the declaration-centric join.
///
/// WHAT: bundles the finalized module constant declarations, the shared type environment,
/// the string table and the canonical type projection context so the join does not take a
/// long positional parameter list. The projection context is built once in the builder from
/// the same nominal origin resolver and external registry already used by the type-surface
/// and trait-surface projections.
struct FoldedValueJoinContext<'a> {
    module_constants: &'a [Declaration],
    type_environment: &'a TypeEnvironment,
    string_table: &'a StringTable,
    projection_context: &'a CanonicalTypeProjectionContext<'a>,
}

// ===========================================================================
//  Declaration-centric join
// ===========================================================================

/// Join the projection intermediates into one declaration-centric record per stable origin.
///
/// WHAT: indexes the type-surface and trait-surface intermediates by origin, then iterates the
/// export bindings in their deterministic order. For each unique origin, the matching
/// type-surface or trait-surface entry is consumed and its semantic facts are moved into a
/// [`PublicDeclarationRecord`]. When multiple bindings name the same origin, one record is
/// produced at the first binding's deterministic position and every binding is preserved
/// separately in the draft. Receiver methods are grouped by receiver origin and attached to
/// their owning struct or choice record. A struct fact carrying choice variants or a choice
/// fact carrying struct fields is rejected rather than silently dropped. After every binding is
/// processed, any unconsumed type-surface entry, trait surface or receiver method is an extra
/// fact that must not leak: it is reported as a `CompilerError`.
///
/// WHY: the existing projections already validate binding-to-root joins, so the intermediates
/// are consistent. This join reshapes them into the declaration-centric model the draft owns, and
/// adds a final totality check so a mismatch between the three intermediates can never silently
/// omit or duplicate a public fact. Error selection is deterministic: leftover counts and
/// categories are reported without relying on unordered hash-map iteration.
fn join_declaration_records(
    export_bindings: &[ExportBinding],
    type_surface: DefinedPublicTypeSurface,
    trait_surfaces: Vec<DefinedPublicTraitSurface>,
    folded_value_context: &FoldedValueJoinContext,
) -> Result<Vec<PublicDeclarationRecord>, CompilerError> {
    let DefinedPublicTypeSurface {
        free_functions,
        nominal_types,
        transparent_aliases,
        constants,
        receiver_methods,
    } = type_surface;

    let mut functions_by_origin: FxHashMap<OriginDeclarationId, DefinedPublicFunctionTypeSurface> =
        FxHashMap::default();
    for function in free_functions {
        let origin = OriginDeclarationId::Function(function.origin.clone());
        if functions_by_origin
            .insert(origin.clone(), function)
            .is_some()
        {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two free-function type-surface entries share origin {:?}; a duplicate must not silently overwrite the first",
                origin
            )));
        }
    }

    let mut nominals_by_origin: FxHashMap<OriginDeclarationId, DefinedPublicNominalTypeSurface> =
        FxHashMap::default();
    for nominal in nominal_types {
        let origin = OriginDeclarationId::Type(nominal.origin.clone());
        if nominals_by_origin.insert(origin.clone(), nominal).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two nominal type-surface entries share origin {:?}; a duplicate must not silently overwrite the first",
                origin
            )));
        }
    }

    let mut aliases_by_origin: FxHashMap<OriginDeclarationId, DefinedPublicAliasTypeSurface> =
        FxHashMap::default();
    for alias in transparent_aliases {
        let origin = OriginDeclarationId::Type(alias.origin.clone());
        if aliases_by_origin.insert(origin.clone(), alias).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two transparent-alias type-surface entries share origin {:?}; a duplicate must not silently overwrite the first",
                origin
            )));
        }
    }

    let mut constants_by_origin: FxHashMap<OriginDeclarationId, DefinedPublicConstantTypeSurface> =
        FxHashMap::default();
    for constant in constants {
        let origin = OriginDeclarationId::Constant(constant.origin.clone());
        if constants_by_origin
            .insert(origin.clone(), constant)
            .is_some()
        {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two constant type-surface entries share origin {:?}; a duplicate must not silently overwrite the first",
                origin
            )));
        }
    }

    // Index the finalized module constant declarations by their exact defining
    // `InternedPath` so each constant export binding joins the one module constant declared
    // at the same path. Joining by exact path, not by public-name spelling, keeps unrelated
    // private constants with a shared leaf name from clashing and keeps aliased public
    // bindings on one origin. A duplicate exact declaration path is an internal invariant
    // violation, not a silent overwrite. Private constants that have no export binding
    // remain in the index and are expected extras: they are not rejected after the join.
    let mut module_constants_by_path: FxHashMap<&InternedPath, &Declaration> = FxHashMap::default();
    for declaration in folded_value_context.module_constants {
        if module_constants_by_path
            .insert(&declaration.id, declaration)
            .is_some()
        {
            return Err(CompilerError::compiler_error(
                "public-interface draft join: two module constant declarations share the exact defining path; a duplicate must not silently overwrite the first",
            ));
        }
    }

    let mut traits_by_origin: FxHashMap<OriginDeclarationId, DefinedPublicTraitSurface> =
        FxHashMap::default();
    for surface in trait_surfaces {
        let origin = OriginDeclarationId::Trait(surface.origin.clone());
        if traits_by_origin.insert(origin.clone(), surface).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two trait surfaces share origin {:?}; a duplicate must not silently overwrite the first",
                origin
            )));
        }
    }

    let mut receiver_methods_by_receiver: FxHashMap<
        OriginTypeId,
        Vec<DefinedPublicReceiverMethodTypeSurface>,
    > = FxHashMap::default();
    let mut seen_method_origins: FxHashSet<OriginFunctionId> = FxHashSet::default();
    for method in receiver_methods {
        if !seen_method_origins.insert(method.method_origin.clone()) {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft join: two receiver-method type-surface entries share method origin {:?}; a duplicate must not silently overwrite the first",
                method.method_origin
            )));
        }
        receiver_methods_by_receiver
            .entry(method.receiver_origin.clone())
            .or_default()
            .push(method);
    }

    let mut declarations = Vec::new();
    let mut seen_origins: FxHashSet<OriginDeclarationId> = FxHashSet::default();

    for binding in export_bindings {
        // One declaration record per unique origin. A second binding for the same origin is
        // preserved in the export-bindings list but does not produce a second record.
        if !seen_origins.insert(binding.origin().clone()) {
            continue;
        }

        match binding.origin() {
            OriginDeclarationId::Function(function_origin) => {
                let function = functions_by_origin
                    .remove(&OriginDeclarationId::Function(function_origin.clone()))
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "public-interface draft join: the function export binding '{}' has no matching free-function type-surface entry",
                            binding.public_name()
                        ))
                    })?;
                declarations.push(PublicDeclarationRecord {
                    origin: binding.origin().clone(),
                    semantics: PublicDeclarationSemantics::Function(PublicFunctionSemantics {
                        generic_parameters: function.generic_parameters,
                        parameters: function.parameters,
                        returns: function.returns,
                        error_return: function.error_return,
                    }),
                });
            }
            OriginDeclarationId::Type(type_origin) => match type_origin.category() {
                OriginTypeCategory::Struct => {
                    let nominal = nominals_by_origin
                        .remove(&OriginDeclarationId::Type(type_origin.clone()))
                        .ok_or_else(|| {
                            CompilerError::compiler_error(format!(
                                "public-interface draft join: the struct export binding '{}' has no matching nominal type-surface entry",
                                binding.public_name()
                            ))
                        })?;

                    // A struct fact must not carry choice variants; rejecting the wrong vector
                    // prevents silently discarding an input fact.
                    if !nominal.variants.is_empty() {
                        return Err(CompilerError::compiler_error(format!(
                            "public-interface draft join: the struct export binding '{}' carries {} choice variant(s); a struct must not contain choice variants",
                            binding.public_name(),
                            nominal.variants.len()
                        )));
                    }

                    let receiver_methods = receiver_methods_by_receiver
                        .remove(type_origin)
                        .unwrap_or_default();
                    declarations.push(PublicDeclarationRecord {
                        origin: binding.origin().clone(),
                        semantics: PublicDeclarationSemantics::Struct(PublicStructSemantics {
                            generic_parameters: nominal.generic_parameters,
                            fields: nominal.fields,
                            receiver_methods: receiver_methods
                                .into_iter()
                                .map(|method| PublicReceiverMethodSemantics {
                                    method_origin: method.method_origin,
                                    parameters: method.parameters,
                                    returns: method.returns,
                                    error_return: method.error_return,
                                })
                                .collect(),
                        }),
                    });
                }
                OriginTypeCategory::Choice => {
                    let nominal = nominals_by_origin
                        .remove(&OriginDeclarationId::Type(type_origin.clone()))
                        .ok_or_else(|| {
                            CompilerError::compiler_error(format!(
                                "public-interface draft join: the choice export binding '{}' has no matching nominal type-surface entry",
                                binding.public_name()
                            ))
                        })?;

                    // A choice fact must not carry struct fields; rejecting the wrong vector
                    // prevents silently discarding an input fact.
                    if !nominal.fields.is_empty() {
                        return Err(CompilerError::compiler_error(format!(
                            "public-interface draft join: the choice export binding '{}' carries {} struct field(s); a choice must not contain struct fields",
                            binding.public_name(),
                            nominal.fields.len()
                        )));
                    }

                    let receiver_methods = receiver_methods_by_receiver
                        .remove(type_origin)
                        .unwrap_or_default();
                    declarations.push(PublicDeclarationRecord {
                        origin: binding.origin().clone(),
                        semantics: PublicDeclarationSemantics::Choice(PublicChoiceSemantics {
                            generic_parameters: nominal.generic_parameters,
                            variants: nominal.variants,
                            receiver_methods: receiver_methods
                                .into_iter()
                                .map(|method| PublicReceiverMethodSemantics {
                                    method_origin: method.method_origin,
                                    parameters: method.parameters,
                                    returns: method.returns,
                                    error_return: method.error_return,
                                })
                                .collect(),
                        }),
                    });
                }
                OriginTypeCategory::TransparentAlias => {
                    let alias = aliases_by_origin
                        .remove(&OriginDeclarationId::Type(type_origin.clone()))
                        .ok_or_else(|| {
                            CompilerError::compiler_error(format!(
                                "public-interface draft join: the transparent-alias export binding '{}' has no matching alias type-surface entry",
                                binding.public_name()
                            ))
                        })?;
                    declarations.push(PublicDeclarationRecord {
                        origin: binding.origin().clone(),
                        semantics: PublicDeclarationSemantics::TransparentAlias(
                            PublicAliasSemantics {
                                target_type_identity: alias.target_type_identity,
                            },
                        ),
                    });
                }
            },
            OriginDeclarationId::Constant(constant_origin) => {
                let constant = constants_by_origin
                    .remove(&OriginDeclarationId::Constant(constant_origin.clone()))
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "public-interface draft join: the constant export binding '{}' has no matching constant type-surface entry",
                            binding.public_name()
                        ))
                    })?;

                let folded_value = join_constant_folded_value(
                    &constant.defining_path,
                    &mut module_constants_by_path,
                    folded_value_context,
                )?;

                declarations.push(PublicDeclarationRecord {
                    origin: binding.origin().clone(),
                    semantics: PublicDeclarationSemantics::Constant(PublicConstantSemantics {
                        type_identity: constant.type_identity,
                        folded_value,
                    }),
                });
            }
            OriginDeclarationId::Trait(trait_origin) => {
                let surface = traits_by_origin
                    .remove(&OriginDeclarationId::Trait(trait_origin.clone()))
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "public-interface draft join: the trait export binding '{}' has no matching trait surface",
                            binding.public_name()
                        ))
                    })?;
                declarations.push(PublicDeclarationRecord {
                    origin: binding.origin().clone(),
                    semantics: PublicDeclarationSemantics::Trait(PublicTraitSemantics {
                        requirements: surface.requirements,
                        incompatibilities: surface.incompatibilities,
                    }),
                });
            }
        }
    }

    // Every type-surface and trait-surface entry must have joined a binding. Deterministic
    // count/category reporting avoids unordered hash-map iteration when selecting an error.
    let leftover_functions = functions_by_origin.len();
    if leftover_functions > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} free-function type-surface entries have no matching export binding",
            leftover_functions
        )));
    }

    let leftover_nominals = nominals_by_origin.len();
    if leftover_nominals > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} nominal type-surface entries have no matching export binding",
            leftover_nominals
        )));
    }

    let leftover_aliases = aliases_by_origin.len();
    if leftover_aliases > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} transparent-alias type-surface entries have no matching export binding",
            leftover_aliases
        )));
    }

    let leftover_constants = constants_by_origin.len();
    if leftover_constants > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} constant type-surface entries have no matching export binding",
            leftover_constants
        )));
    }

    let leftover_traits = traits_by_origin.len();
    if leftover_traits > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} trait surfaces have no matching export binding",
            leftover_traits
        )));
    }

    let leftover_receiver_methods: usize =
        receiver_methods_by_receiver.values().map(Vec::len).sum();
    if leftover_receiver_methods > 0 {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft join: {} receiver method(s) have no matching struct or choice export binding",
            leftover_receiver_methods
        )));
    }

    Ok(declarations)
}

// ===========================================================================
//  Trait-requirement projection
// ===========================================================================

/// A generic-parameter resolver that rejects every request.
///
/// WHAT: trait requirement types never legitimately reference an exported generic parameter:
/// the only generic parameter in a trait signature is the trait `this_type`, which the
/// projection special-cases as [`TraitSurfaceTypeIdentity::SelfType`] before canonical
/// projection. Any other `GenericParameterId` reaching the canonical projection is an
/// internal invariant violation, so this resolver returns a precise `CompilerError` instead
/// of inventing an identity.
struct TraitRequirementGenericParameterResolver;

impl GenericParameterOriginResolver for TraitRequirementGenericParameterResolver {
    fn resolve_generic_parameter_origin(
        &self,
        parameter_id: GenericParameterId,
    ) -> Result<ExportedGenericParameterIdentity, CompilerError> {
        Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: GenericParameterId({}) reached canonical \
             projection inside a trait requirement; only the trait self type may appear and it \
             is special-cased as SelfType, so a nested or unrelated generic parameter is an \
             internal invariant violation",
            parameter_id.0
        )))
    }
}

/// Build the corrected direct trait surfaces from the resolved trait roots and the direct
/// trait export bindings.
///
/// WHAT: keys each surface by the exact matching direct [`OriginTraitId`] export binding,
/// preserves authored requirement order, and validates every requirement receiver
/// `this_type` against the owning trait `this_type`. Direct parameter or return occurrences
/// of the owning `this_type` become [`TraitSurfaceTypeIdentity::SelfType`]; every other
/// `TypeId` projects through the existing canonical type projection as
/// [`TraitSurfaceTypeIdentity::Concrete`]. Each surface also carries the
/// publicly-authored `must not` incompatibilities for the trait, canonicalized through the
/// shared `trait_source_facts` source/core mapping owner, preserving authored source order.
/// A missing, duplicate, self, unmatched, wrong-origin or malformed-self fact is a
/// `CompilerError`.
// The projection genuinely needs every resolved side table (roots, bindings, the
// shared trait-source-fact mapping and both public source origin indexes) plus the type
// environment, external registry and string table. Grouping them into one more struct would
// not improve readability, so the argument count is allowed here as in the sibling
// type-surface projection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_trait_surfaces(
    trait_roots: &[ResolvedPublicTraitRoot],
    export_bindings: &[ExportBinding],
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_nominal_type_origins: &FxHashMap<InternedPath, OriginTypeId>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    type_environment: &TypeEnvironment,
    external_registry: &ExternalPackageRegistry,
    string_table: &StringTable,
) -> Result<Vec<DefinedPublicTraitSurface>, CompilerError> {
    // Index trait roots by public name so each trait export binding joins exactly one root.
    let mut roots_by_name: FxHashMap<&str, &ResolvedPublicTraitRoot> = FxHashMap::default();
    for root in trait_roots {
        let name = root.canonical_path.name_str(string_table).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "public-interface draft trait projection: a trait root has no resolvable \
                 defining name (canonical path: {:?})",
                root.canonical_path
            ))
        })?;
        if roots_by_name.insert(name, root).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: two trait roots share the public \
                 name '{}'; a duplicate trait root must not silently overwrite the first",
                name
            )));
        }
    }

    let nominal_resolver =
        TransientNominalOriginResolver::new(type_environment, public_source_nominal_type_origins);
    let generic_resolver = TraitRequirementGenericParameterResolver;
    let projection_context = CanonicalTypeProjectionContext::new(
        &nominal_resolver,
        &generic_resolver,
        external_registry,
    );

    let mut surfaces = Vec::new();
    let mut consumed_roots: FxHashMap<&str, ()> = FxHashMap::default();

    for binding in export_bindings {
        let OriginDeclarationId::Trait(trait_origin) = binding.origin() else {
            continue;
        };

        let public_name = binding.public_name();
        let root = roots_by_name.get(public_name).copied().ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "public-interface draft trait projection: the trait export binding '{}' has no \
                 matching trait root; every direct trait binding must join exactly one root",
                public_name
            ))
        })?;

        // The trait root canonical path must resolve through the public source-trait origin
        // index to the exact binding origin. A missing or mismatched origin is a
        // `CompilerError` rather than a silent coercion, so a renamed or re-scoped trait can
        // never join the wrong surface.
        let resolved_origin = public_source_trait_origins
            .get(&root.canonical_path)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "public-interface draft trait projection: the trait root '{}' canonical path \
                     has no retained public source-trait origin; a private, unexported or unowned \
                     trait must not enter the public interface",
                    public_name
                ))
            })?;
        if resolved_origin != trait_origin {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: the trait export binding '{}' origin \
                 {:?} disagrees with its root resolved origin {:?}; the binding and root must \
                 name the same trait",
                public_name, trait_origin, resolved_origin
            )));
        }

        if consumed_roots.insert(public_name, ()).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: two trait export bindings share the \
                 public name '{}'; a duplicate trait binding must not join twice",
                public_name
            )));
        }

        // Validate the trait root this_type before projecting requirements. The root type
        // must be a GenericParameter whose name resolves to exactly "This"; anything else is
        // malformed transient AST data.
        validate_trait_root_this_type(root, type_environment, string_table)?;

        let requirements = root
            .requirements
            .iter()
            .map(|requirement| {
                project_trait_requirement(
                    requirement,
                    root.this_type,
                    type_environment,
                    &projection_context,
                    string_table,
                )
            })
            .collect::<Result<Vec<_>, CompilerError>>()?;

        let incompatibilities = project_trait_incompatibilities(
            &root.incompatible_trait_ids,
            trait_origin,
            trait_source_facts,
            public_source_trait_origins,
        )?;

        surfaces.push(DefinedPublicTraitSurface {
            origin: trait_origin.clone(),
            requirements,
            incompatibilities,
        });
    }

    // Every trait root must have joined a binding. A leftover root is extra and must not leak.
    let mut leftover: Vec<&str> = roots_by_name
        .keys()
        .filter(|name| !consumed_roots.contains_key(**name))
        .copied()
        .collect();
    leftover.sort();
    if let Some(name) = leftover.first() {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: the trait root '{}' has no matching \
             export binding; every direct trait root must join exactly one binding",
            name
        )));
    }

    Ok(surfaces)
}

/// Validate that a trait root's `this_type` is the trait-local synthetic generic
/// parameter named exactly `This`.
///
/// WHAT: uses the `TypeEnvironment` authority to require the root type to be a
/// `TypeDefinition::GenericParameter` whose name resolves to exactly `This`. Anything else
/// is malformed transient AST data and returns `CompilerError`. This check runs before the
/// per-requirement receiver equality validation so a malformed root fails early.
fn validate_trait_root_this_type(
    root: &ResolvedPublicTraitRoot,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let Some(definition) = type_environment.get(root.this_type) else {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: the trait root '{}' this_type TypeId({}) \
             is not registered in the TypeEnvironment; the trait self type must be a synthetic \
             generic parameter",
            root.canonical_path.to_string(string_table),
            root.this_type.0
        )));
    };

    let TypeDefinition::GenericParameter(parameter) = definition else {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: the trait root '{}' this_type TypeId({}) \
             resolved to {:?}, not a GenericParameter; the trait self type must be the synthetic \
             generic parameter named exactly \"This\"",
            root.canonical_path.to_string(string_table),
            root.this_type.0,
            definition
        )));
    };

    let name = string_table.resolve(parameter.name);
    if name != "This" {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: the trait root '{}' this_type is a \
             GenericParameter named '{}', not \"This\"; the trait self type must be named \
             exactly \"This\"",
            root.canonical_path.to_string(string_table),
            name
        )));
    }

    Ok(())
}

/// Project one resolved trait requirement into a stable surface requirement.
///
/// WHAT: validates the receiver `this_type` against the owning trait `this_type` before
/// mapping immutable or mutable access, then projects each non-receiver parameter and each
/// return slot. A direct occurrence of the owning `this_type` becomes
/// [`TraitSurfaceTypeIdentity::SelfType`]; every other `TypeId` projects through the existing
/// canonical type projection. Receiver mutability is stored separately from the self type.
fn project_trait_requirement(
    requirement: &ResolvedTraitRequirementFact,
    owning_this_type: TypeId,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<PublicTraitRequirementSurface, CompilerError> {
    // Validate the receiver this_type against the owning trait this_type before mapping access.
    // A mismatch is malformed transient AST data and must not be silently discarded.
    if requirement.receiver.this_type != owning_this_type {
        return Err(CompilerError::compiler_error(format!(
            "public-interface draft trait projection: a requirement receiver this_type TypeId({}) \
             does not equal the owning trait this_type TypeId({}); receiver self must match the \
             owning trait before mapping immutable or mutable access",
            requirement.receiver.this_type.0, owning_this_type.0
        )));
    }

    let receiver_access = match requirement.receiver.access {
        TraitReceiverAccessKind::Immutable => PublicTraitReceiverAccess::Immutable,
        TraitReceiverAccessKind::Mutable => PublicTraitReceiverAccess::Mutable,
    };

    let name = string_table.resolve(requirement.name).to_owned();

    let parameters = requirement
        .parameters
        .iter()
        .map(|parameter| {
            let name = parameter
                .name
                .name_str(string_table)
                .map(|name| name.to_owned());
            let type_identity = project_trait_surface_type_identity(
                parameter.type_id,
                owning_this_type,
                type_environment,
                context,
            )?;
            Ok(PublicTraitRequirementParameter {
                name,
                value_mode: parameter.value_mode.clone(),
                type_identity,
            })
        })
        .collect::<Result<Vec<_>, CompilerError>>()?;

    let returns = requirement
        .returns
        .iter()
        .map(|return_slot| {
            let type_identity = project_trait_surface_type_identity(
                return_slot.type_id,
                owning_this_type,
                type_environment,
                context,
            )?;
            Ok(PublicTraitRequirementReturn {
                channel: return_slot.channel,
                type_identity,
            })
        })
        .collect::<Result<Vec<_>, CompilerError>>()?;

    Ok(PublicTraitRequirementSurface {
        name,
        receiver_access,
        parameters,
        returns,
    })
}

/// Project one trait requirement type identity.
///
/// WHAT: a direct occurrence of the owning `this_type` becomes
/// [`TraitSurfaceTypeIdentity::SelfType`]. Every other `TypeId` projects through the existing
/// canonical type projection as [`TraitSurfaceTypeIdentity::Concrete`]. Only the exact owning
/// `this_type` is classified as self; an unrelated local `TypeId` remains an ordinary
/// projected type or fails through the canonical projection with a `CompilerError`.
fn project_trait_surface_type_identity(
    type_id: TypeId,
    owning_this_type: TypeId,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
) -> Result<TraitSurfaceTypeIdentity, CompilerError> {
    if type_id == owning_this_type {
        return Ok(TraitSurfaceTypeIdentity::SelfType);
    }

    let canonical = project_type_id_to_canonical_identity(type_id, type_environment, context)?;
    Ok(TraitSurfaceTypeIdentity::Concrete(Box::new(canonical)))
}

/// Project the publicly-authored incompatibilities for one direct public trait into ordered,
/// duplicate-free [`CanonicalTraitIdentity`] values.
///
/// WHAT: resolves each retained incompatible `TraitId` through the shared
/// `trait_source_facts` source/core mapping owner (the same owner used by generic-bound
/// projection), so a source trait becomes `CanonicalTraitIdentity::Source` through the public
/// source-trait origin index and a core trait becomes `CanonicalTraitIdentity::Core`. The
/// owning trait is always a source trait (core traits are never direct public trait roots), so
/// its canonical identity is `CanonicalTraitIdentity::Source(owning_trait_origin)`. An
/// incompatibility that resolves to the owning trait itself is an internal self-relation and is
/// a `CompilerError`. A missing source fact, a missing public source origin or a duplicate
/// canonical identity is a `CompilerError`.
/// WHY: the public-interface draft carries only stable canonical identities: no `TraitId`,
/// `InternedPath`, `StringId`, source location or rendered trait name crosses the boundary.
/// The output order is the deterministic authored source order recorded by the trait
/// environment, independent of hash-map iteration.
fn project_trait_incompatibilities(
    incompatible_trait_ids: &[TraitId],
    owning_trait_origin: &OriginTraitId,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
) -> Result<Vec<CanonicalTraitIdentity>, CompilerError> {
    let owning_canonical = CanonicalTraitIdentity::Source(owning_trait_origin.clone());

    let mut incompatibilities = Vec::with_capacity(incompatible_trait_ids.len());
    for trait_id in incompatible_trait_ids {
        let Some(source_fact) = trait_source_facts.get(trait_id) else {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: an incompatibility TraitId({}) for trait origin {:?} has no retained trait source fact; a missing local mapping is an internal invariant violation",
                trait_id.0, owning_trait_origin
            )));
        };

        let canonical_identity = project_trait_source_fact_to_canonical_identity(
            source_fact,
            public_source_trait_origins,
        )?;

        if canonical_identity == owning_canonical {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: the trait origin {:?} carries an incompatibility that resolves to itself; an internal self-relation must not enter the public trait surface",
                owning_trait_origin
            )));
        }

        if incompatibilities.contains(&canonical_identity) {
            return Err(CompilerError::compiler_error(format!(
                "public-interface draft trait projection: two incompatibility trait ids for trait origin {:?} resolved to the same canonical trait identity {:?}; a duplicate must not enter the public trait surface",
                owning_trait_origin, canonical_identity
            )));
        }

        incompatibilities.push(canonical_identity);
    }

    Ok(incompatibilities)
}
/// Join one public constant surface to its matching module constant declaration by exact
/// defining path and convert the folded expression to an owned [`PublicFoldedValue`].
///
/// WHAT: looks up the surface's defining `InternedPath` in the module-constants-by-path index,
/// removes the entry so the same constant cannot join twice, and converts the expression
/// through the shared folded-value conversion. A missing match is a `CompilerError`: a public
/// constant with no matching finalized declaration cannot be projected.
fn join_constant_folded_value(
    defining_path: &InternedPath,
    module_constants_by_path: &mut FxHashMap<&InternedPath, &Declaration>,
    context: &FoldedValueJoinContext,
) -> Result<PublicFoldedValue, CompilerError> {
    let declaration = module_constants_by_path
        .remove(defining_path)
        .ok_or_else(|| {
            CompilerError::compiler_error(
                "public-interface draft join: a constant export binding has no matching finalized \
                 module constant declaration at its defining path; the folded value cannot be \
                 projected without the donor-local AST expression",
            )
        })?;

    convert_expression_to_folded_value(
        &declaration.value,
        context.type_environment,
        context.string_table,
        context.projection_context,
    )
}

#[cfg(test)]
#[path = "tests/public_interface_draft_tests.rs"]
mod tests;
