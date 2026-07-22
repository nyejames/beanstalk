//! The one aggregate pre-HIR public-semantic handoff for a compiled module.
//!
//! WHAT: owns the [`PublicInterfaceDraftBuilder`] and the [`PublicInterfaceDraft`] it produces.
//! The draft is the sole pre-HIR public-semantic handoff that crosses the semantic compilation
//! boundary. It internalizes three projection components as private builder steps:
//! - the accepted direct export-origin projection ([`DefinedPublicExportOrigins`]),
//! - the accepted canonical type-surface projection ([`DefinedPublicTypeSurface`]),
//! - the corrected direct trait-requirement projection ([`DefinedPublicTraitSurface`]).
//!
//! WHY: the compiler design overview and the recovery plan require one aggregate producer
//! boundary instead of parallel `DefinedPublic*` fields on `CompiledModuleResult`. Keeping the
//! three projections behind one builder preserves their proven, total projection logic while
//! ensuring only one draft crosses orchestration. R1 may internally retain the proven
//! projection components; R2 folds them into declaration-centric records and final provenance.
//!
//! ## Trait-requirement projection
//!
//! Direct trait surfaces are keyed by the exact matching direct [`OriginTraitId`] export
//! binding, preserve authored requirement order, and reuse existing [`ValueMode`] and
//! [`ReturnChannel`] facts. Requirement receiver `this_type` is validated against the owning
//! trait `this_type` before mapping immutable or mutable receiver access; a mismatch is a
//! `CompilerError`. Direct parameter or return occurrences of that exact `this_type` become a
//! trait-local [`TraitSurfaceTypeIdentity::SelfType`]; every other `TypeId` projects through
//! the existing canonical type projection as a [`TraitSurfaceTypeIdentity::Concrete`]. No
//! unscoped self type is added to [`CanonicalTypeIdentity`].
//!
//! Boundary: the draft is private to compiler/build orchestration and never reaches backends.
//! It is not the final `PublicSemanticInterface`.

use crate::compiler_frontend::ast::AstPublicInterfaceProjectionInput;
use crate::compiler_frontend::ast::statements::functions::ReturnChannel;
use crate::compiler_frontend::ast::{
    ResolvedPublicTraitRoot, ResolvedTraitRequirementFact, TraitReceiverAccessKind,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalTypeIdentity, CanonicalTypeProjectionContext, ExportedGenericParameterIdentity,
    GenericParameterOriginResolver, project_type_id_to_canonical_identity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterId, TypeId};
use crate::compiler_frontend::defined_public_export_origins::DefinedPublicExportOriginDraft;
use crate::compiler_frontend::defined_public_type_surface::{
    DefinedPublicTypeSurface, TransientNominalOriginResolver, build_defined_public_type_surface,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::semantic_identity::{
    DefinedPublicExportOrigins, ExportBinding, OriginDeclarationId, OriginTraitId, OriginTypeId,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

use rustc_hash::FxHashMap;

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
pub(crate) enum DefinedPublicTraitReceiverAccess {
    Immutable,
    Mutable,
}

/// One non-receiver parameter in a trait requirement surface.
///
/// `name` is the owned authored parameter name, or `None` when the source signature omits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTraitRequirementParameter {
    name: Option<String>,
    value_mode: ValueMode,
    type_identity: TraitSurfaceTypeIdentity,
}

/// One return slot in a trait requirement surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTraitRequirementReturn {
    channel: ReturnChannel,
    type_identity: TraitSurfaceTypeIdentity,
}

/// One method requirement in a trait surface, in authored order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTraitRequirementSurface {
    name: String,
    receiver_access: DefinedPublicTraitReceiverAccess,
    parameters: Vec<DefinedPublicTraitRequirementParameter>,
    returns: Vec<DefinedPublicTraitRequirementReturn>,
}

/// The trait surface for one exported trait authored directly in the active module root.
///
/// Keyed by the exact matching direct [`OriginTraitId`] export binding. Requirements preserve
/// authored order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTraitSurface {
    origin: OriginTraitId,
    requirements: Vec<DefinedPublicTraitRequirementSurface>,
}

// ===========================================================================
//  PublicInterfaceDraft
// ===========================================================================

/// The one aggregate pre-HIR public-semantic handoff for one compiled module.
///
/// WHAT: owns the three proven projection components for declarations authored directly in the
/// active module root: the direct export origins, the canonical type surface and the corrected
/// direct trait surfaces. R1 retains these components internally; R2 folds them into
/// declaration-centric records with provenance. It carries only owned stable values: no
/// donor-local `TypeId`, `NominalTypeId`, `GenericParameterId`, `TraitId`, `InternedPath` or
/// `StringId` crosses this boundary.
///
/// It is deliberately not the final `PublicSemanticInterface`. Folded constant values, generic
/// template bodies, evidence, access/effect summaries, provenance, re-export interfaces and
/// cross-module call lowering remain for later phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicInterfaceDraft {
    export_origins: DefinedPublicExportOrigins,
    type_surface: DefinedPublicTypeSurface,
    trait_surfaces: Vec<DefinedPublicTraitSurface>,
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
    /// WHAT: runs the three internal projection steps in order:
    /// 1. finalize the export-origin component from the receiver catalog,
    /// 2. build the canonical type surface from the resolved root table,
    /// 3. build the corrected trait surfaces from the resolved trait roots.
    ///
    /// Each step is total: a missing, duplicate, unmatched or malformed fact is a
    /// `CompilerError` rather than a silent omission.
    pub(crate) fn build(self) -> Result<PublicInterfaceDraft, CompilerError> {
        let PublicInterfaceDraftBuilderInput {
            export_origin_draft,
            public_interface_projection_input,
            public_source_nominal_type_origins,
            public_source_trait_origins,
            type_environment,
            external_registry,
            string_table,
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
            public_source_nominal_type_origins,
            public_source_trait_origins,
            type_environment,
            external_registry,
            string_table,
        )?;

        Ok(PublicInterfaceDraft {
            export_origins,
            type_surface,
            trait_surfaces,
        })
    }
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
/// [`TraitSurfaceTypeIdentity::Concrete`]. A missing, duplicate, unmatched, wrong-origin or
/// malformed-self fact is a `CompilerError`.
pub(crate) fn build_trait_surfaces(
    trait_roots: &[ResolvedPublicTraitRoot],
    export_bindings: &[ExportBinding],
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

        surfaces.push(DefinedPublicTraitSurface {
            origin: trait_origin.clone(),
            requirements,
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
) -> Result<DefinedPublicTraitRequirementSurface, CompilerError> {
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
        TraitReceiverAccessKind::Immutable => DefinedPublicTraitReceiverAccess::Immutable,
        TraitReceiverAccessKind::Mutable => DefinedPublicTraitReceiverAccess::Mutable,
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
            Ok(DefinedPublicTraitRequirementParameter {
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
            Ok(DefinedPublicTraitRequirementReturn {
                channel: return_slot.channel,
                type_identity,
            })
        })
        .collect::<Result<Vec<_>, CompilerError>>()?;

    Ok(DefinedPublicTraitRequirementSurface {
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

#[cfg(test)]
#[path = "tests/public_interface_draft_tests.rs"]
mod tests;
