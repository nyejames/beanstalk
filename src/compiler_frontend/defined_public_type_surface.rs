//! Production owner for the directly-defined public TYPE surface projection.
//!
//! WHAT: consumes the transient AST-owned [`ResolvedPublicTypeRootTable`] and joins each root
//! to the existing [`DefinedPublicExportOrigins`] stable declaration origin, then projects every
//! required `TypeId` into owned, hashable [`CanonicalTypeIdentity`] values through the existing
//! `canonical_type_identity` projection owner. The output is the type-only public surface: free
//! function signatures, nominal field/variant types, transparent alias targets, constant types
//! and receiver-method signatures, each attached to its stable declaration origin.
//!
//! This is **not** `PublicSemanticInterface`. It carries only canonical type shapes and owned
//! names plus ordered canonical generic trait bounds. Folded constant values, generic
//! template bodies, trait requirements and evidence, access/effect summaries, provenance,
//! re-export interfaces and cross-module call lowering remain for later phases.
//!
//! WHY: the compiler design overview requires AST to own canonical export projection and to
//! emit stable semantic identities before HIR. Donor-local `TypeId` values must not cross the
//! module result boundary. This module is the single production consumer of the transient root
//! table: it takes the table and the `TypeEnvironment` while both are still available, builds the
//! stable surface, and leaves no donor-local `TypeId` in the output.
//!
//! ## Transient resolvers
//!
//! Two transient resolvers implement the `canonical_type_identity` traits for the duration of one
//! projection:
//!
//! - [`TransientNominalOriginResolver`]: maps `NominalTypeId` to `OriginTypeId` through
//!   `TypeEnvironment` nominal paths plus the transient expanded public source-nominal origin
//!   index of source nominals targeted by retained public exports. Direct public declarations,
//!   imported project-graph nominals and private normal-file nominals exposed through a public
//!   alias resolve to their owning module origin; unexported, unregistered and source-package
//!   nominals without a project-module owner fail through `CompilerError`.
//!
//! - [`TransientGenericParameterOriginResolver`]: maps `GenericParameterId` to
//!   `ExportedGenericParameterIdentity` from the roots' own generic parameter lists and the
//!   stable declaration origins. Free functions and struct/choice declarations are the only
//!   generic declaration owners; receiver methods reuse their receiver nominal's parameters and
//!   must not become owners.
//!
//! ## Determinism
//!
//! Top-level surface entries are ordered by the deterministic export-binding order already
//! established in `DefinedPublicExportOrigins`. Receiver method entries are ordered by stable
//! receiver origin, then by stable method origin. Authored parameter, return, field and variant
//! order is preserved within each declaration. No ordering depends on `FxHashMap` iteration.

use crate::compiler_frontend::ast::ReceiverMethodEntry;
use crate::compiler_frontend::ast::ResolvedTraitSourceFact;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::{
    ResolvedPublicTypeRoot, ResolvedPublicTypeRootKind, ResolvedPublicTypeRootTable,
};
use crate::compiler_frontend::canonical_type_identity::{
    CanonicalCoreTraitIdentity, CanonicalTraitIdentity, CanonicalTypeIdentity,
    CanonicalTypeProjectionContext, ExportedGenericParameterIdentity, GenericDeclarationOrigin,
    GenericParameterOriginResolver, NominalOriginResolver, project_type_id_to_canonical_identity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::datatypes::definitions::{
    ChoiceTypeDefinition, ChoiceVariantPayloadDefinition, StructTypeDefinition, TypeDefinition,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{
    GenericParameterId, GenericParameterListId, NominalTypeId, TypeId,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::folded_value::{
    PublicFoldedValue, convert_expression_to_folded_value,
};
use crate::compiler_frontend::semantic_identity::{
    DefinedPublicExportOrigins, ExportBinding, OriginConstantId, OriginDeclarationId,
    OriginFunctionId, OriginTraitId, OriginTypeCategory, OriginTypeId, ReceiverSurfaceOrigins,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::traits::environment::CoreTraitKind;
use crate::compiler_frontend::traits::ids::TraitId;

use rustc_hash::{FxHashMap, FxHashSet};

// ---------------------------------------------------------------------------
//  Stable type-surface value types
// ---------------------------------------------------------------------------

/// The defined public TYPE surface for one compiled module: an internal projection
/// intermediate consumed by the [`PublicInterfaceDraftBuilder`] before the draft boundary.
///
/// WHAT: carries only owned, stable values: canonical type identities and owned authored names.
/// Its `public_callable_origin_seeds` are a separate transient path-to-origin side table; they
/// are consumed before the draft boundary and never enter declaration records or module
/// artefacts. The same exact relationship is used for HIR lowering and validated generic-body
/// extraction while donor-local AST facts still exist.
/// The stable type-surface entries themselves never embed `TypeId`, `NominalTypeId`,
/// `GenericParameterId`, `InternedPath`, `StringId`, source locations, absolute paths or
/// donor-local external numeric IDs.
///
/// It is deliberately not `PublicSemanticInterface`. It carries type shapes only and does not
/// cross the draft boundary as a stored component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTypeSurface {
    pub(super) free_functions: Vec<DefinedPublicFunctionTypeSurface>,
    pub(super) nominal_types: Vec<DefinedPublicNominalTypeSurface>,
    pub(super) transparent_aliases: Vec<DefinedPublicAliasTypeSurface>,
    pub(super) constants: Vec<DefinedPublicConstantTypeSurface>,
    pub(super) receiver_methods: Vec<DefinedPublicReceiverMethodTypeSurface>,
    pub(super) public_callable_origin_seeds: Vec<PublicCallableOriginSeed>,
}

/// Exact transient declaration-path-to-origin facts for directly exported callables.
///
/// WHAT: pairs one donor-local declaration path with its stable free-function or receiver-method
/// origin and records whether the validated body is generic. This is a transient AST/type-surface
/// join fact, not HIR identity and not public-interface identity.
/// WHY: generic body extraction must distinguish same-named receiver methods by exact path and
/// stable origin, while HIR must receive only the non-generic subset as local function seeds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PublicCallableOriginSeed {
    pub(crate) path: InternedPath,
    pub(crate) origin: OriginFunctionId,
    pub(crate) generic_template: bool,
}

/// One parameter slot in a public function or receiver-method semantic record.
///
/// WHAT: a draft/public semantic leaf type that crosses the draft boundary inside
/// [`PublicFunctionSemantics`] and [`PublicReceiverMethodSemantics`]. `name` is the owned
/// authored parameter name, or `None` when the source signature omits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicParameterTypeSlot {
    pub(crate) name: Option<String>,
    pub(crate) type_identity: CanonicalTypeIdentity,
    /// The owned folded default value, or `None` when the parameter has no default.
    ///
    /// WHAT: retains the compile-time default expression as an owned backend-neutral
    /// [`PublicFoldedValue`]. Constant references are resolved and inlined by the
    /// established function-signature and struct-default owners before finalization;
    /// finalization normalizes template payloads and synchronizes emitted declarations
    /// into the retained root table and receiver catalog. The receiver parameter itself
    /// normally has no default and remains `None`. Choice payload fields remain
    /// default-free.
    pub(crate) folded_default: Option<PublicFoldedValue>,
}

/// One return slot in a public function or receiver-method semantic record.
///
/// WHAT: a draft/public semantic leaf type that crosses the draft boundary inside
/// [`PublicFunctionSemantics`] and [`PublicReceiverMethodSemantics`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PublicReturnTypeSlot {
    pub(crate) type_identity: CanonicalTypeIdentity,
}

/// One generic parameter with its ordered canonical trait bound identities in a public
/// declaration record.
///
/// WHAT: a draft/public semantic leaf type that crosses the draft boundary inside
/// [`PublicFunctionSemantics`], [`PublicStructSemantics`] and [`PublicChoiceSemantics`]. It
/// pairs the stable [`ExportedGenericParameterIdentity`] (declaration owner + position +
/// authored name, unchanged) with an ordered `Vec<CanonicalTraitIdentity>` resolved from the
/// `TypeEnvironment`'s declaration-site `TraitId` bounds. The identity never carries bounds;
/// the bounds are a separate fact on this entry.
/// WHY: the exported generic parameter must carry both identity and bounds so a cross-module
/// consumer can see the full constraint shape without donor-local `TraitId`,
/// `GenericParameterId`, `InternedPath`, `StringId`, `FileId`, `CoreTraitKind` registry handle
/// or source location.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PublicGenericParameterSurface {
    pub(crate) identity: ExportedGenericParameterIdentity,
    pub(crate) bounds: Vec<CanonicalTraitIdentity>,
}

/// The type-only surface for one exported free function.
///
/// `parameters` and `returns` preserve authored order. `error_return` is `None` when the
/// function has no error channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicFunctionTypeSurface {
    pub(crate) origin: OriginFunctionId,
    pub(crate) generic_parameters: Vec<PublicGenericParameterSurface>,
    pub(crate) parameters: Vec<PublicParameterTypeSlot>,
    pub(crate) returns: Vec<PublicReturnTypeSlot>,
    pub(crate) error_return: Option<CanonicalTypeIdentity>,
}

/// One field in a public struct semantic record or a choice-variant payload.
///
/// WHAT: a draft/public semantic leaf type that crosses the draft boundary inside
/// [`PublicStructSemantics`] and [`PublicChoiceVariantSurface`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicFieldTypeSlot {
    pub(crate) name: String,
    pub(crate) type_identity: CanonicalTypeIdentity,
    /// The owned folded default value, or `None` when the field has no default.
    ///
    /// WHAT: retains the compile-time default expression as an owned backend-neutral
    /// [`PublicFoldedValue`]. Constant references are resolved and inlined by the
    /// established function-signature and struct-default owners before finalization;
    /// finalization normalizes template payloads and synchronizes emitted declarations
    /// into the retained root table and receiver catalog. Choice payload fields remain
    /// default-free.
    pub(crate) folded_default: Option<PublicFoldedValue>,
}

/// One choice variant in a public choice semantic record.
///
/// WHAT: a draft/public semantic leaf type that crosses the draft boundary inside
/// [`PublicChoiceSemantics`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublicChoiceVariantSurface {
    pub(crate) name: String,
    pub(crate) payload_fields: Vec<PublicFieldTypeSlot>,
}

/// The type-only surface for one exported nominal type (struct or choice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicNominalTypeSurface {
    pub(crate) origin: OriginTypeId,
    pub(crate) generic_parameters: Vec<PublicGenericParameterSurface>,
    pub(crate) fields: Vec<PublicFieldTypeSlot>,
    pub(crate) variants: Vec<PublicChoiceVariantSurface>,
}

/// The type-only surface for one exported transparent alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicAliasTypeSurface {
    pub(crate) origin: OriginTypeId,
    pub(crate) target_type_identity: CanonicalTypeIdentity,
}

/// The transient type-and-path surface for one exported constant.
///
/// WHAT: carries the stable constant origin, the canonical type identity and the exact
/// defining [`InternedPath`] retained from the resolved public type root. The defining
/// path is a transient join key: the declaration-centric draft consumes it to locate the
/// matching finalized `Ast::module_constants` entry by exact declaration path, then the
/// path does not survive the draft boundary. The folded value itself is owned by the
/// declaration draft, not by this transient surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicConstantTypeSurface {
    pub(crate) origin: OriginConstantId,
    pub(crate) type_identity: CanonicalTypeIdentity,
    pub(crate) defining_path: InternedPath,
}

/// The type-only surface for one exported receiver method.
///
/// The method stays attached to its stable receiver origin. It is not a free namespace entry
/// and cannot be imported or re-exported separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicReceiverMethodTypeSurface {
    pub(crate) receiver_origin: OriginTypeId,
    pub(crate) method_origin: OriginFunctionId,
    /// Exact donor-local path used only to classify a retained generic template before the
    /// surface is consumed by the declaration-centric draft join.
    pub(crate) function_path: InternedPath,
    /// Set by the draft builder from the authoritative retained generic-template path map.
    /// Generic receiver methods have no base local HIR function until the R3 sidecar worklist.
    pub(crate) generic_template: bool,
    pub(crate) parameters: Vec<PublicParameterTypeSlot>,
    pub(crate) returns: Vec<PublicReturnTypeSlot>,
    pub(crate) error_return: Option<CanonicalTypeIdentity>,
}

// ---------------------------------------------------------------------------
//  Transient nominal origin resolver
// ---------------------------------------------------------------------------

/// Transient resolver that maps module-local `NominalTypeId` to stable `OriginTypeId`.
///
/// WHAT: looks up the nominal's `InternedPath` through `TypeEnvironment` then resolves it
/// through the transient expanded public source-nominal origin index. Direct public declarations,
/// imported project-graph nominals and private normal-file nominals exposed through a public alias
/// resolve to their owning module origin; unexported, unregistered and source-package nominals
/// without a project-module owner fail through `CompilerError`.
pub(crate) struct TransientNominalOriginResolver<'a> {
    type_environment: &'a TypeEnvironment,
    public_source_nominal_type_origins: &'a FxHashMap<InternedPath, OriginTypeId>,
}

impl<'a> TransientNominalOriginResolver<'a> {
    pub(crate) fn new(
        type_environment: &'a TypeEnvironment,
        public_source_nominal_type_origins: &'a FxHashMap<InternedPath, OriginTypeId>,
    ) -> Self {
        Self {
            type_environment,
            public_source_nominal_type_origins,
        }
    }
}

impl NominalOriginResolver for TransientNominalOriginResolver<'_> {
    fn resolve_nominal_origin(
        &self,
        nominal_id: NominalTypeId,
    ) -> Result<OriginTypeId, CompilerError> {
        let path = self
            .type_environment
            .nominal_path_by_id(nominal_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: NominalTypeId({}) has no registered \
                 path in the TypeEnvironment",
                    nominal_id.0
                ))
            })?;

        self.public_source_nominal_type_origins
            .get(path)
            .cloned()
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: NominalTypeId({}) resolves to a \
                     path that is not targeted by a retained public source export with a stable \
                     owning source-module origin",
                    nominal_id.0
                ))
            })
    }
}

// ---------------------------------------------------------------------------
//  Transient generic-parameter origin resolver
// ---------------------------------------------------------------------------

/// Transient resolver that maps module-local `GenericParameterId` to stable
/// `ExportedGenericParameterIdentity`.
///
/// WHAT: built once from the resolved public type roots and the stable declaration origins.
/// Free functions and struct/choice declarations are the only generic declaration owners.
/// Receiver methods reuse their receiver nominal's parameters and never add entries.
/// A `GenericParameterId` with no registered owner fails through `CompilerError`.
struct TransientGenericParameterOriginResolver {
    origins: FxHashMap<GenericParameterId, ExportedGenericParameterIdentity>,
}

impl TransientGenericParameterOriginResolver {
    fn new() -> Self {
        Self {
            origins: FxHashMap::default(),
        }
    }

    /// Register every generic parameter in a list under the given declaration origin.
    fn register_list(
        &mut self,
        type_environment: &TypeEnvironment,
        list_id: GenericParameterListId,
        declaration_origin: GenericDeclarationOrigin,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        let list = type_environment
            .generic_parameters(list_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: GenericParameterListId({}) is missing \
                 from the TypeEnvironment while registering generic-parameter origins",
                    list_id.0
                ))
            })?;

        for (position, parameter) in list.parameters.iter().enumerate() {
            let authored_name = string_table.resolve(parameter.name).to_owned();
            let identity = ExportedGenericParameterIdentity::new(
                declaration_origin.clone(),
                position as u32,
                authored_name,
            );
            if self.origins.insert(parameter.id, identity).is_some() {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: GenericParameterId({}) is \
                     registered under two different declaration origins; ambiguous generic \
                     ownership",
                    parameter.id.0
                )));
            }
        }

        Ok(())
    }

    /// Register one donor-local `GenericParameterId` under an already-established stable
    /// identity.
    ///
    /// WHAT: used by receiver-method parameter aliasing so the method's local parameter ID
    /// maps to the receiver nominal's stable exported identity without making the method a
    /// generic declaration owner. A donor-local ID already registered under the same identity
    /// is idempotent; under a conflicting identity it is a `CompilerError`.
    fn register_aligned_parameter_alias(
        &mut self,
        parameter_id: GenericParameterId,
        identity: ExportedGenericParameterIdentity,
    ) -> Result<(), CompilerError> {
        match self.origins.get(&parameter_id) {
            Some(existing) if existing == &identity => Ok(()),
            Some(existing) => Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: GenericParameterId({}) is already                  registered under a different stable identity ({:?}); receiver-method                  parameter aliasing cannot override an existing registration",
                parameter_id.0, existing
            ))),
            None => {
                self.origins.insert(parameter_id, identity);
                Ok(())
            }
        }
    }
}

impl GenericParameterOriginResolver for TransientGenericParameterOriginResolver {
    fn resolve_generic_parameter_origin(
        &self,
        parameter_id: GenericParameterId,
    ) -> Result<ExportedGenericParameterIdentity, CompilerError> {
        self.origins.get(&parameter_id).cloned().ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "defined public type-surface projection: GenericParameterId({}) has no \
                 registered exported generic declaration owner; receiver methods must not become \
                 generic declaration owners and private or unregistered parameters cannot \
                 enter the public type surface",
                parameter_id.0
            ))
        })
    }
}

// ---------------------------------------------------------------------------
//  Root-to-binding join index
// ---------------------------------------------------------------------------

/// Indexes the resolved roots by their public declaration name for deterministic join to export
/// bindings.
///
/// The export bindings are keyed by `public_name: String`, which is the last component of the
/// root's declaration path. Building this index by name lets the projection iterate over the
/// already-sorted export bindings and find each matching root without re-scanning headers.
///
/// Construction is total: a root without a resolvable name is a `CompilerError`, and two roots
/// sharing a public name is a duplicate that is rejected rather than silently overwriting the
/// first. Roots are removed as bindings consume them, so a root left unmatched after every
/// binding has joined is a stale/extra root that is reported explicitly.
struct RootIndex<'a> {
    roots_by_name: FxHashMap<String, &'a ResolvedPublicTypeRoot>,
}

impl<'a> RootIndex<'a> {
    fn new(
        roots: &'a [ResolvedPublicTypeRoot],
        string_table: &StringTable,
    ) -> Result<Self, CompilerError> {
        let mut roots_by_name = FxHashMap::default();
        for root in roots {
            let name = root.path.name_str(string_table).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a public type root has no \
                     resolvable defining name (path: {:?})",
                    root.path
                ))
            })?;
            if roots_by_name.insert(name.to_owned(), root).is_some() {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: two public type roots share the \
                     public name '{}'; a duplicate public root must not silently overwrite the \
                     first",
                    name
                )));
            }
        }
        Ok(Self { roots_by_name })
    }

    /// Remove and return the root for `public_name`, or `CompilerError` when no root matches.
    ///
    /// Consuming the root guarantees each transient root joins at most one binding and lets the
    /// caller detect a root left unmatched after every binding has joined.
    fn take(&mut self, public_name: &str) -> Result<&'a ResolvedPublicTypeRoot, CompilerError> {
        self.roots_by_name.remove(public_name).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "defined public type-surface projection: the export binding '{}' has no \
                 matching public type root; every non-trait binding must join exactly one root",
                public_name
            ))
        })
    }

    /// The remaining unmatched root names in deterministic sorted order, for an
    /// unmatched-extra-root diagnostic. Determinism here is diagnostic-only: it never affects
    /// the projected surface, only which names appear in the error.
    fn remaining_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.roots_by_name.keys().cloned().collect();
        names.sort();
        names
    }
}

// ---------------------------------------------------------------------------
//  Projection
// ---------------------------------------------------------------------------

/// Build the defined public type-only surface from the transient root table and stable origins.
///
/// WHAT: takes the `ResolvedPublicTypeRootTable`, `DefinedPublicExportOrigins`,
/// the transient expanded public source-nominal origin index and the transient expanded public
/// source-trait origin index,
/// `TypeEnvironment`, `ExternalPackageRegistry` and `StringTable`, then:
/// 1. builds the transient nominal and generic-parameter origin resolvers,
/// 2. creates the canonical projection context,
/// 3. joins each root to its stable export-binding origin in deterministic binding order,
///    consuming each root exactly once and rejecting a binding with no root (except traits),
/// 4. projects every required `TypeId` into canonical identities and each exported generic
///    parameter's ordered canonical trait bound identities through the transient trait source
///    facts on the root table and the transient source-trait origin index,
/// 5. projects receiver methods in deterministic receiver/method order, joining each by exact
///    stable receiver origin and method name.
///
/// The stable surface carries only owned stable values. No `TypeId`, `NominalTypeId`,
/// `GenericParameterId`, `GenericParameterListId`, `TraitId`, `CoreTraitKind`, `InternedPath` or
/// `StringId` enters its declaration-shaped entries. The separate exact callable-origin path
/// side table is consumed by HIR lowering and generic-template extraction before the
/// module-result boundary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_defined_public_type_surface(
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
    public_source_nominal_type_origins: &FxHashMap<InternedPath, OriginTypeId>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    generic_function_templates: &FxHashMap<InternedPath, GenericFunctionTemplate>,
    type_environment: &TypeEnvironment,
    external_registry: &ExternalPackageRegistry,
    string_table: &StringTable,
) -> Result<DefinedPublicTypeSurface, CompilerError> {
    let nominal_resolver =
        TransientNominalOriginResolver::new(type_environment, public_source_nominal_type_origins);

    let mut generic_resolver = TransientGenericParameterOriginResolver::new();
    register_generic_parameter_origins(
        &mut generic_resolver,
        root_table,
        export_origins,
        generic_function_templates,
        &nominal_resolver,
        type_environment,
        string_table,
    )?;

    let projection_context = CanonicalTypeProjectionContext::new(
        &nominal_resolver,
        &generic_resolver,
        external_registry,
    );

    let mut root_index = RootIndex::new(&root_table.roots, string_table)?;

    let mut free_functions = Vec::new();
    let mut nominal_types = Vec::new();
    let mut transparent_aliases = Vec::new();
    let mut constants = Vec::new();
    let mut public_callable_origin_seeds = Vec::new();

    // Iterate over export bindings in their deterministic order. A trait binding carries no
    // type root in this type-only slice, so it is skipped without consuming a root. Every
    // function, type or constant binding must join exactly one matching root; a missing root
    // is a `CompilerError` rather than a silent skip. Each joined root is consumed so a stale
    // or extra root is detected after the loop.
    for binding in export_origins.export_bindings() {
        // Traits own no type surface in this slice, so a trait binding intentionally carries no
        // type root and consumes none. Every other binding must join exactly one root.
        if matches!(binding.origin(), OriginDeclarationId::Trait(_)) {
            continue;
        }

        let root = root_index.take(binding.public_name())?;

        match &root.kind {
            ResolvedPublicTypeRootKind::Function {
                signature,
                generic_parameter_list_id,
            } => {
                let OriginDeclarationId::Function(function_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("function", binding));
                };
                let function_surface = project_free_function(
                    function_origin.clone(),
                    *generic_parameter_list_id,
                    signature,
                    type_environment,
                    &projection_context,
                    &root_table.trait_source_facts,
                    public_source_trait_origins,
                    string_table,
                )?;
                push_public_callable_origin_seed(
                    &mut public_callable_origin_seeds,
                    root.path.clone(),
                    function_origin.clone(),
                    generic_parameter_list_id.is_some(),
                )?;
                free_functions.push(function_surface);
            }
            ResolvedPublicTypeRootKind::Struct { type_id, fields } => {
                let OriginDeclarationId::Type(type_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("struct", binding));
                };
                if type_origin.category() != OriginTypeCategory::Struct {
                    return Err(origin_category_mismatch_error("struct", binding));
                }
                nominal_types.push(project_struct(
                    type_origin.clone(),
                    *type_id,
                    fields,
                    type_environment,
                    &projection_context,
                    &root_table.trait_source_facts,
                    public_source_trait_origins,
                    string_table,
                )?);
            }
            ResolvedPublicTypeRootKind::Choice { type_id } => {
                let OriginDeclarationId::Type(type_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("choice", binding));
                };
                if type_origin.category() != OriginTypeCategory::Choice {
                    return Err(origin_category_mismatch_error("choice", binding));
                }
                nominal_types.push(project_choice(
                    type_origin.clone(),
                    *type_id,
                    type_environment,
                    &projection_context,
                    &root_table.trait_source_facts,
                    public_source_trait_origins,
                    string_table,
                )?);
            }
            ResolvedPublicTypeRootKind::TransparentAlias { target_type_id } => {
                let OriginDeclarationId::Type(type_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("alias", binding));
                };
                if type_origin.category() != OriginTypeCategory::TransparentAlias {
                    return Err(origin_category_mismatch_error("alias", binding));
                }
                let target_identity = project_type_id_to_canonical_identity(
                    *target_type_id,
                    type_environment,
                    &projection_context,
                )?;
                transparent_aliases.push(DefinedPublicAliasTypeSurface {
                    origin: type_origin.clone(),
                    target_type_identity: target_identity,
                });
            }
            ResolvedPublicTypeRootKind::Constant { type_id } => {
                let OriginDeclarationId::Constant(constant_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("constant", binding));
                };
                let type_identity = project_type_id_to_canonical_identity(
                    *type_id,
                    type_environment,
                    &projection_context,
                )?;
                constants.push(DefinedPublicConstantTypeSurface {
                    origin: constant_origin.clone(),
                    type_identity,
                    defining_path: root.path.clone(),
                });
            }
        }
    }

    // Every non-trait root must have joined a binding. A root left in the index is stale or
    // extra: it has no matching export binding, so it would otherwise leak into no surface.
    let remaining = root_index.remaining_names();
    if !remaining.is_empty() {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: the public type root(s) {} have no matching \
             export binding; every non-trait root must join exactly one binding",
            remaining.join(", ")
        )));
    }

    // Receiver methods: project in the deterministic receiver-surface order from
    // DefinedPublicExportOrigins, matching each method to its resolved entry.
    let (receiver_methods, receiver_callable_origin_seeds) = project_receiver_methods(
        &root_table.receiver_methods,
        export_origins.receiver_surfaces(),
        type_environment,
        &projection_context,
        string_table,
    )?;
    public_callable_origin_seeds.extend(receiver_callable_origin_seeds);

    Ok(DefinedPublicTypeSurface {
        free_functions,
        nominal_types,
        transparent_aliases,
        constants,
        receiver_methods,
        public_callable_origin_seeds,
    })
}

/// Retain one exact declaration path-to-origin relationship for direct exported callable joins.
///
/// WHAT: rejects duplicate stable function origins and duplicate generic declaration paths while
/// the AST-owned path is still available. Non-generic receiver methods may share a leaf path when
/// their stable receiver origins differ; the resulting seed is consumed before the module-result
/// boundary and its non-generic subset is later converted into HIR-local function-origin seeds.
/// WHY: the later join must not reconstruct identity from rendered names, paths or declaration
/// order, and no donor-local path may enter a completed public declaration record.
fn push_public_callable_origin_seed(
    seeds: &mut Vec<PublicCallableOriginSeed>,
    path: InternedPath,
    origin: OriginFunctionId,
    generic_template: bool,
) -> Result<(), CompilerError> {
    if seeds.iter().any(|existing| existing.origin == origin) {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: duplicate stable function origin {:?}",
            origin
        )));
    }
    if seeds
        .iter()
        .any(|existing| existing.path == path && (existing.generic_template || generic_template))
    {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: duplicate public callable declaration path \
             {:?}",
            path
        )));
    }
    seeds.push(PublicCallableOriginSeed {
        path,
        origin,
        generic_template,
    });
    Ok(())
}

/// Register generic-parameter origins from function and nominal roots, then alias
/// receiver-method generic parameters to their receiver nominal's stable identities.
///
/// Free functions with a `GenericParameterListId` register their parameters under a
/// `GenericDeclarationOrigin::free_function`. Struct/choice roots register their parameters
/// under a `GenericDeclarationOrigin::nominal_type`. Receiver methods with a validated
/// `GenericFunctionTemplate` alias their local `GenericParameterId` values to the receiver
/// nominal's already-registered stable identities without becoming declaration owners.
fn register_generic_parameter_origins(
    generic_resolver: &mut TransientGenericParameterOriginResolver,
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
    generic_function_templates: &FxHashMap<InternedPath, GenericFunctionTemplate>,
    nominal_resolver: &TransientNominalOriginResolver,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Build a name-to-function-origin lookup from the export bindings.
    let mut function_origin_by_name: FxHashMap<&str, &OriginFunctionId> = FxHashMap::default();
    for binding in export_origins.export_bindings() {
        if let OriginDeclarationId::Function(function_origin) = binding.origin() {
            function_origin_by_name.insert(binding.public_name(), function_origin);
        }
    }

    for root in &root_table.roots {
        match &root.kind {
            ResolvedPublicTypeRootKind::Function {
                generic_parameter_list_id: Some(list_id),
                ..
            } => {
                let function_name = root.path.name_str(string_table).ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "defined public type-surface projection: a public free-function root \
                         has no resolvable name (path: {:?})",
                        root.path
                    ))
                })?;

                let function_origin = function_origin_by_name
                    .get(function_name)
                    .copied()
                    .ok_or_else(|| {
                        CompilerError::compiler_error(format!(
                            "defined public type-surface projection: the generic free \
                                 function '{}' has no matching export binding",
                            function_name
                        ))
                    })?;

                let declaration_origin =
                    GenericDeclarationOrigin::free_function(function_origin.clone())?;

                generic_resolver.register_list(
                    type_environment,
                    *list_id,
                    declaration_origin,
                    string_table,
                )?;
            }
            ResolvedPublicTypeRootKind::Function {
                generic_parameter_list_id: None,
                ..
            } => {}

            ResolvedPublicTypeRootKind::Struct { type_id, .. } => {
                register_nominal_generic_origins(
                    generic_resolver,
                    *type_id,
                    type_environment,
                    nominal_resolver,
                    string_table,
                )?;
            }
            ResolvedPublicTypeRootKind::Choice { type_id } => {
                register_nominal_generic_origins(
                    generic_resolver,
                    *type_id,
                    type_environment,
                    nominal_resolver,
                    string_table,
                )?;
            }
            ResolvedPublicTypeRootKind::TransparentAlias { .. }
            | ResolvedPublicTypeRootKind::Constant { .. } => {}
        }
    }

    // After nominal origins are registered, alias receiver-method local generic parameter IDs
    // to their receiver nominal's stable identities. Receiver methods must not become
    // generic declaration owners; they reuse the nominal's parameters by alignment.
    register_receiver_method_generic_parameter_aliases(
        generic_resolver,
        &root_table.receiver_methods,
        generic_function_templates,
        type_environment,
        string_table,
    )?;

    Ok(())
}

/// Register generic-parameter origins for one nominal (struct or choice) root.
fn register_nominal_generic_origins(
    generic_resolver: &mut TransientGenericParameterOriginResolver,
    type_id: TypeId,
    type_environment: &TypeEnvironment,
    nominal_resolver: &TransientNominalOriginResolver,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let (nominal_id, generic_parameter_list_id) = match type_environment.get(type_id) {
        Some(TypeDefinition::Struct(def)) => (def.id, def.generic_parameters),
        Some(TypeDefinition::Choice(def)) => (def.id, def.generic_parameters),
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: a nominal root TypeId({}) is neither a \
                 struct nor a choice definition",
                type_id.0
            )));
        }
    };

    let Some(list_id) = generic_parameter_list_id else {
        return Ok(());
    };

    let nominal_origin =
        NominalOriginResolver::resolve_nominal_origin(nominal_resolver, nominal_id)?;

    let declaration_origin = GenericDeclarationOrigin::nominal_type(nominal_origin)?;
    generic_resolver.register_list(type_environment, list_id, declaration_origin, string_table)?;

    Ok(())
}

/// Alias receiver-method local generic parameter IDs to their receiver nominal's stable
/// exported generic parameter identities.
///
/// WHAT: for each receiver method with a validated `GenericFunctionTemplate`, resolves the
/// method's `GenericParameterListId` and the receiver nominal's `GenericParameterListId` from
/// the `TypeEnvironment`, verifies position-by-position authored-name alignment, then aliases
/// each receiver-local `GenericParameterId` to the nominal's already-registered
/// `ExportedGenericParameterIdentity`. The receiver method must not become a
/// `GenericDeclarationOrigin` owner.
///
/// WHY: a generic receiver method's signature references receiver-local `GenericParameterId`
/// values distinct from the nominal's IDs. The nominal's parameters are already registered by
/// `register_nominal_generic_origins`; aliasing lets the method's type projection resolve its
/// local IDs to the same stable identities without registering a second declaration owner.
fn register_receiver_method_generic_parameter_aliases(
    generic_resolver: &mut TransientGenericParameterOriginResolver,
    receiver_method_entries: &[ReceiverMethodEntry],
    generic_function_templates: &FxHashMap<InternedPath, GenericFunctionTemplate>,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for entry in receiver_method_entries {
        let Some(template) = generic_function_templates.get(&entry.function_path) else {
            continue;
        };

        let receiver_path = match &entry.receiver {
            ReceiverKey::Struct(path) | ReceiverKey::Choice(path) => path,
            ReceiverKey::External(_) | ReceiverKey::BuiltinScalar(_) => {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a receiver method with a                      validated generic template carries a non-nominal receiver key ({:?});                      aligned generic parameters require a nominal receiver",
                    entry.receiver
                )));
            }
        };

        let nominal_id = type_environment
            .nominal_id_for_path(receiver_path)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a generic receiver method's                      receiver path is not a registered nominal (path: {:?})",
                    receiver_path
                ))
            })?;

        let nominal_generic_list_id = match type_environment.struct_definition(nominal_id) {
            Some(def) => def.generic_parameters,
            None => match type_environment.choice_definition(nominal_id) {
                Some(def) => def.generic_parameters,
                None => {
                    return Err(CompilerError::compiler_error(format!(
                        "defined public type-surface projection: a generic receiver method's                          nominal ID ({}) is neither a struct nor a choice definition",
                        nominal_id.0
                    )));
                }
            },
        };

        let Some(nominal_list_id) = nominal_generic_list_id else {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: a generic receiver method on                  non-generic nominal {:?} has a validated generic template; a generic                  receiver method requires a generic receiver nominal",
                receiver_path
            )));
        };

        alias_aligned_generic_parameters(
            generic_resolver,
            type_environment,
            nominal_list_id,
            template.generic_parameter_list_id,
            string_table,
        )?;
    }

    Ok(())
}

/// Alias each receiver-method generic parameter to the receiver nominal's already-registered
/// stable identity, verifying authored-name alignment at each position.
///
/// WHAT: resolves both parameter lists from the `TypeEnvironment`, checks count and per-position
/// authored-name equality, then for each pair resolves the nominal's stable identity from the
/// already-built resolver and registers the method's local ID under it. A count or name mismatch
/// is a `CompilerError` rather than a silent coercion.
fn alias_aligned_generic_parameters(
    generic_resolver: &mut TransientGenericParameterOriginResolver,
    type_environment: &TypeEnvironment,
    nominal_list_id: GenericParameterListId,
    method_list_id: GenericParameterListId,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let nominal_list = type_environment
        .generic_parameters(nominal_list_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "defined public type-surface projection: the receiver nominal's                  GenericParameterListId({}) is missing from the TypeEnvironment while                  aliasing receiver-method generic parameters",
                nominal_list_id.0
            ))
        })?;

    let method_list = type_environment
        .generic_parameters(method_list_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "defined public type-surface projection: the receiver method's                  GenericParameterListId({}) is missing from the TypeEnvironment while                  aliasing receiver-method generic parameters",
                method_list_id.0
            ))
        })?;

    if nominal_list.parameters.len() != method_list.parameters.len() {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: a generic receiver method has {}              generic parameters but its receiver nominal has {}; aligned parameters              must match in count",
            method_list.parameters.len(),
            nominal_list.parameters.len()
        )));
    }

    for (nominal_param, method_param) in nominal_list
        .parameters
        .iter()
        .zip(method_list.parameters.iter())
    {
        let nominal_name = string_table.resolve(nominal_param.name);
        let method_name = string_table.resolve(method_param.name);
        if nominal_name != method_name {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: a generic receiver method's                  parameter '{}' does not match the receiver nominal's parameter '{}';                  aligned parameters must match in authored name and order",
                method_name, nominal_name
            )));
        }

        // The nominal's GenericParameterId is already registered; resolve its stable identity
        // and alias the method's local ID under it.
        let nominal_identity =
            generic_resolver.resolve_generic_parameter_origin(nominal_param.id)?;
        generic_resolver.register_aligned_parameter_alias(method_param.id, nominal_identity)?;
    }

    Ok(())
}

/// Project one root's exported generic parameter surfaces (identity plus ordered bounds) in
/// declaration-local order.
///
/// WHAT: iterates the retained `GenericParameterList` in declaration-local order and resolves
/// each `GenericParameterId` through the already-built total
/// `TransientGenericParameterOriginResolver`. Each surface entry pairs the stable
/// `ExportedGenericParameterIdentity` (declaration owner + position + authored name) with its
/// ordered `Vec<CanonicalTraitIdentity>` bounds resolved from the declaration-site `TraitId`
/// bounds. A non-generic declaration (`list_id` is `None`) exposes an empty list. Each resolved
/// identity must name the `expected_origin`: a wrong-owner, missing, duplicate or inconsistent
/// parameter is a `CompilerError` rather than a silent omission. The output carries only owned
/// stable identities and canonical bound identities; no `GenericParameterId`,
/// `GenericParameterListId`, `TraitId`, `StringId` or `InternedPath` crosses the boundary.
fn project_exported_generic_parameter_surfaces(
    generic_parameter_list_id: Option<GenericParameterListId>,
    type_environment: &TypeEnvironment,
    generic_resolver: &dyn GenericParameterOriginResolver,
    expected_origin: &GenericDeclarationOrigin,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
) -> Result<Vec<PublicGenericParameterSurface>, CompilerError> {
    let Some(list_id) = generic_parameter_list_id else {
        return Ok(Vec::new());
    };

    let list = type_environment.generic_parameters(list_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "defined public type-surface projection: GenericParameterListId({}) is missing from the TypeEnvironment while projecting exported generic parameter surfaces",
            list_id.0
        ))
    })?;

    let mut surfaces: Vec<PublicGenericParameterSurface> =
        Vec::with_capacity(list.parameters.len());
    for parameter in &list.parameters {
        let identity = generic_resolver.resolve_generic_parameter_origin(parameter.id)?;

        // Validate exact owner identity. The resolver already registers each parameter under
        // the correct declaration origin, so a mismatch means a list was joined to the wrong
        // root or the owner table is inconsistent. Both are internal `CompilerError` failures.
        if identity.declaration_origin() != expected_origin {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: an exported generic parameter resolved to declaration origin {:?} but its root owner is {:?}; a wrong-owner parameter must not enter the public type surface",
                identity.declaration_origin(),
                expected_origin,
            )));
        }

        // Reject a duplicate resolved identity. Two distinct `GenericParameterId`s that resolve
        // to the same owner, position and name signal an inconsistent owner table rather than a
        // legitimate second declaration.
        if surfaces.iter().any(|surface| surface.identity == identity) {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: two exported generic parameters resolved to the same identity {:?}; a duplicate parameter identity must not enter the public type surface",
                identity,
            )));
        }

        // Project ordered canonical trait bound identities from the declaration-site `TraitId`
        // bounds. Each local `TraitId` is resolved through the transient trait source facts
        // retained on the root table, then through the public source-trait origin index.
        let bounds = project_generic_parameter_bounds(
            parameter.id,
            type_environment,
            trait_source_facts,
            public_source_trait_origins,
        )?;

        surfaces.push(PublicGenericParameterSurface { identity, bounds });
    }

    Ok(surfaces)
}

/// Project one resolved trait source fact to its stable canonical trait identity.
///
/// WHAT: a source trait ([`ResolvedTraitSourceFact::Source`]) resolves to
/// `CanonicalTraitIdentity::Source` through the public source-trait origin index; a core
/// trait ([`ResolvedTraitSourceFact::Core`]) resolves to its stable
/// [`CanonicalCoreTraitIdentity`]. A source trait whose canonical path has no retained
/// public source-trait origin is a `CompilerError`.
/// WHY: this is the single source/core mapping owner shared by generic-bound projection and
/// direct public trait incompatibility projection, so both paths resolve a retained trait
/// source fact to the same canonical identity through one implementation. Extracting it
/// keeps the source/core classification logic in the type-surface projection owner rather
/// than duplicating it in the draft builder.
pub(crate) fn project_trait_source_fact_to_canonical_identity(
    source_fact: &ResolvedTraitSourceFact,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
) -> Result<CanonicalTraitIdentity, CompilerError> {
    match source_fact {
        ResolvedTraitSourceFact::Source(path) => {
            let Some(origin) = public_source_trait_origins.get(path) else {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a trait source path {:?} has no retained public source-trait origin; a private, unexported or unowned trait must not enter the public type surface",
                    path
                )));
            };
            Ok(CanonicalTraitIdentity::Source(origin.clone()))
        }
        ResolvedTraitSourceFact::Core(kind) => {
            let core_identity = match kind {
                CoreTraitKind::Displayable => CanonicalCoreTraitIdentity::Displayable,
                CoreTraitKind::Castable {
                    target,
                    fallibility,
                } => CanonicalCoreTraitIdentity::Castable {
                    target: *target,
                    fallibility: *fallibility,
                },
            };
            Ok(CanonicalTraitIdentity::Core(core_identity))
        }
    }
}

/// Project ordered canonical trait bound identities for one generic parameter.
///
/// WHAT: reads the declaration-site `TraitId` bounds from the `TypeEnvironment` in their
/// recorded order, then resolves each through the transient trait source facts and the public
/// source-trait origin index. A source trait (`ResolvedTraitSourceFact::Source`) resolves to
/// its exact `OriginTraitId` through the trait origin index; a core trait
/// (`ResolvedTraitSourceFact::Core`) resolves to its stable `CanonicalCoreTraitIdentity`.
/// A missing source origin (private/unexported/unowned), a missing local mapping or a
/// conflicting mapping is a `CompilerError`. A duplicate canonical bound identity is rejected.
/// WHY: the stable output must carry only canonical trait identities, never donor-local
/// `TraitId`, `InternedPath`, `StringId`, `FileId`, `CoreTraitKind` registry handle or source
/// location. The bound order is preserved exactly as the `TypeEnvironment` records it.
fn project_generic_parameter_bounds(
    parameter_id: GenericParameterId,
    type_environment: &TypeEnvironment,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
) -> Result<Vec<CanonicalTraitIdentity>, CompilerError> {
    let Some(bounds) = type_environment.trait_bounds_for_generic_parameter(parameter_id) else {
        return Ok(Vec::new());
    };

    let mut canonical_bounds = Vec::with_capacity(bounds.len());
    for trait_id in bounds {
        let Some(source_fact) = trait_source_facts.get(trait_id) else {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: a generic parameter bound TraitId({}) has no retained trait source fact; a missing local mapping is an internal invariant violation",
                trait_id.0
            )));
        };

        let canonical_identity = project_trait_source_fact_to_canonical_identity(
            source_fact,
            public_source_trait_origins,
        )?;

        // Reject a duplicate canonical bound identity. Two distinct `TraitId`s that resolve to
        // the same canonical trait identity signal inconsistent internal metadata rather than a
        // legitimate second bound.
        if canonical_bounds.contains(&canonical_identity) {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: two generic parameter bounds resolved to the same canonical trait identity {:?}; a duplicate bound identity must not enter the public type surface",
                canonical_identity
            )));
        }

        canonical_bounds.push(canonical_identity);
    }

    Ok(canonical_bounds)
}

/// Project one free-function signature into the stable type surface.
#[allow(clippy::too_many_arguments)]
fn project_free_function(
    function_origin: OriginFunctionId,
    generic_parameter_list_id: Option<GenericParameterListId>,
    signature: &FunctionSignature,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    string_table: &StringTable,
) -> Result<DefinedPublicFunctionTypeSurface, CompilerError> {
    let expected_origin = GenericDeclarationOrigin::free_function(function_origin.clone())?;

    let generic_parameters = project_exported_generic_parameter_surfaces(
        generic_parameter_list_id,
        type_environment,
        context.generic_parameter_origins(),
        &expected_origin,
        trait_source_facts,
        public_source_trait_origins,
    )?;

    let parameters = signature
        .parameters
        .iter()
        .map(|declaration| {
            let name = declaration
                .id
                .name_str(string_table)
                .map(|name| name.to_owned());
            let type_identity = project_type_id_to_canonical_identity(
                declaration.value.type_id,
                type_environment,
                context,
            )?;
            let folded_default = project_folded_default(
                &declaration.value,
                type_environment,
                context,
                string_table,
            )?;
            Ok(PublicParameterTypeSlot {
                name,
                type_identity,
                folded_default,
            })
        })
        .collect::<Result<Vec<_>, CompilerError>>()?;

    let (returns, error_return) =
        project_return_slots(&signature.returns, type_environment, context)?;

    Ok(DefinedPublicFunctionTypeSurface {
        origin: function_origin,
        generic_parameters,
        parameters,
        returns,
        error_return,
    })
}

/// Project success and error return slots, returning them separately.
///
/// A resolved public signature slot missing `TypeId` is `CompilerError`; no slot is omitted.
fn project_return_slots(
    return_slots: &[ReturnSlot],
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
) -> Result<(Vec<PublicReturnTypeSlot>, Option<CanonicalTypeIdentity>), CompilerError> {
    let mut returns = Vec::new();
    let mut error_return = None;

    for slot in return_slots {
        let type_id = slot.type_id.ok_or_else(|| {
            CompilerError::compiler_error(
                "defined public type-surface projection: a resolved public signature return \
                 slot has no TypeId; the signature was not fully resolved before projection",
            )
        })?;

        let type_identity =
            project_type_id_to_canonical_identity(type_id, type_environment, context)?;

        match slot.channel {
            ReturnChannel::Success => returns.push(PublicReturnTypeSlot { type_identity }),
            ReturnChannel::Error => {
                if error_return.is_some() {
                    return Err(CompilerError::compiler_error(
                        "defined public type-surface projection: a public signature carries \
                         multiple error-channel return slots",
                    ));
                }
                error_return = Some(type_identity);
            }
        }
    }

    Ok((returns, error_return))
}

/// Project one struct root into the stable nominal type surface.
#[allow(clippy::too_many_arguments)]
fn project_struct(
    type_origin: OriginTypeId,
    type_id: TypeId,
    fields: &[Declaration],
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    string_table: &StringTable,
) -> Result<DefinedPublicNominalTypeSurface, CompilerError> {
    let definition = type_environment.get(type_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "defined public type-surface projection: struct root TypeId({}) is not registered \
             in the TypeEnvironment",
            type_id.0
        ))
    })?;

    let struct_definition = match definition {
        TypeDefinition::Struct(def) => def,
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: struct root TypeId({}) does not \
                 resolve to a struct definition",
                type_id.0
            )));
        }
    };

    // Validate that the nominal resolves through the public nominal origin resolver to the
    // same origin as the export binding. This admits only public source nominals with a stable
    // owning source-module origin and rejects private or unregistered nominals.
    let resolved_origin = context
        .nominal_origins()
        .resolve_nominal_origin(struct_definition.id)?;
    if resolved_origin != type_origin {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: struct root TypeId({}) resolves to \
             origin {:?} but the export binding carries origin {:?}",
            type_id.0, resolved_origin, type_origin
        )));
    }

    let expected_origin = GenericDeclarationOrigin::nominal_type(type_origin.clone())?;

    let generic_parameters = project_exported_generic_parameter_surfaces(
        struct_definition.generic_parameters,
        type_environment,
        context.generic_parameter_origins(),
        &expected_origin,
        trait_source_facts,
        public_source_trait_origins,
    )?;

    let fields = project_fields_with_defaults(
        type_id,
        struct_definition,
        fields,
        type_environment,
        context,
        string_table,
    )?;

    Ok(DefinedPublicNominalTypeSurface {
        origin: type_origin,
        generic_parameters,
        fields,
        variants: Vec::new(),
    })
}

/// Project one choice root into the stable nominal type surface.
#[allow(clippy::too_many_arguments)]
fn project_choice(
    type_origin: OriginTypeId,
    type_id: TypeId,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    trait_source_facts: &FxHashMap<TraitId, ResolvedTraitSourceFact>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
    string_table: &StringTable,
) -> Result<DefinedPublicNominalTypeSurface, CompilerError> {
    let definition = type_environment.get(type_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "defined public type-surface projection: choice root TypeId({}) is not registered \
             in the TypeEnvironment",
            type_id.0
        ))
    })?;

    let choice_definition = match definition {
        TypeDefinition::Choice(def) => def,
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: choice root TypeId({}) does not \
                 resolve to a choice definition",
                type_id.0
            )));
        }
    };

    // Validate that the nominal resolves through the public nominal origin resolver to the
    // same origin as the export binding. This admits only public source nominals with a stable
    // owning source-module origin and rejects private or unregistered nominals.
    let resolved_origin = context
        .nominal_origins()
        .resolve_nominal_origin(choice_definition.id)?;
    if resolved_origin != type_origin {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: choice root TypeId({}) resolves to \
             origin {:?} but the export binding carries origin {:?}",
            type_id.0, resolved_origin, type_origin
        )));
    }

    let expected_origin = GenericDeclarationOrigin::nominal_type(type_origin.clone())?;

    let generic_parameters = project_exported_generic_parameter_surfaces(
        choice_definition.generic_parameters,
        type_environment,
        context.generic_parameter_origins(),
        &expected_origin,
        trait_source_facts,
        public_source_trait_origins,
    )?;

    let variants =
        project_choice_variants(choice_definition, type_environment, context, string_table)?;

    Ok(DefinedPublicNominalTypeSurface {
        origin: type_origin,
        generic_parameters,
        fields: Vec::new(),
        variants,
    })
}

/// Total-join retained struct field declarations against the canonical
/// [`StructTypeDefinition`] fields and project stable field type slots with folded defaults.
///
/// WHAT: the canonical `StructTypeDefinition.fields` is the sole type authority for field
/// names, order and `TypeId`s. The retained `Declaration` values supply only the folded
/// default expression for each field. The join rejects count, name/order, duplicate-name and
/// declaration-value `TypeId` mismatches with a `CompilerError` so the retained declaration
/// vector is never trusted as a parallel type authority. `root_type_id` is the canonical
/// `TypeId` for the struct root, used in diagnostics; `StructTypeDefinition.id` is a nominal
/// ID and does not identify the root `TypeId`.
fn project_fields_with_defaults(
    root_type_id: TypeId,
    struct_definition: &StructTypeDefinition,
    field_declarations: &[Declaration],
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<Vec<PublicFieldTypeSlot>, CompilerError> {
    if struct_definition.fields.len() != field_declarations.len() {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: struct root TypeId({}) has {} canonical fields but {} retained field declarations; the retained declaration count must match the canonical struct definition",
            root_type_id.0,
            struct_definition.fields.len(),
            field_declarations.len(),
        )));
    }

    let mut seen_names: FxHashSet<StringId> = FxHashSet::default();

    let mut projected_fields = Vec::with_capacity(struct_definition.fields.len());
    for (canonical_field, declaration) in struct_definition
        .fields
        .iter()
        .zip(field_declarations.iter())
    {
        let canonical_name_id = canonical_field.name.name();
        let declaration_name_id = declaration.id.name();

        if canonical_name_id != declaration_name_id {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: struct root TypeId({}) has a field name or order mismatch at canonical field {:?}; the retained declaration carries {:?}; the retained declarations must match the canonical field order",
                root_type_id.0, canonical_field.name, declaration.id,
            )));
        }

        if let Some(name_id) = canonical_name_id
            && !seen_names.insert(name_id)
        {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: struct root TypeId({}) has a duplicate \
                 field name {:?}; canonical struct definitions must not contain duplicates",
                root_type_id.0, canonical_field.name,
            )));
        }

        let name = canonical_field
            .name
            .name_str(string_table)
            .map(|name| name.to_owned())
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a struct field has no resolvable name (path: {:?})",
                    canonical_field.name
                ))
            })?;

        if canonical_field.type_id != declaration.value.type_id {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: struct root TypeId({}) field {:?} has a TypeId mismatch: canonical TypeId({}) vs retained declaration TypeId({}); the retained declaration must agree with the canonical struct definition",
                root_type_id.0,
                canonical_field.name,
                canonical_field.type_id.0,
                declaration.value.type_id.0,
            )));
        }

        let type_identity = project_type_id_to_canonical_identity(
            canonical_field.type_id,
            type_environment,
            context,
        )?;

        let folded_default =
            project_folded_default(&declaration.value, type_environment, context, string_table)?;
        projected_fields.push(PublicFieldTypeSlot {
            name,
            type_identity,
            folded_default,
        });
    }
    Ok(projected_fields)
}

/// Project one parameter or field default expression to an owned [`PublicFoldedValue`].
///
/// WHAT: a NoValue expression means the slot has no default and returns None. Any other
/// expression kind is converted through the shared folded-value converter, which expects the
/// expression to be already normalized: templates folded to StringSlice by finalization and
/// constant references resolved and inlined by the established function-signature and
/// struct-default owners before finalization. A Template or Reference reaching this boundary
/// is an internal CompilerError naming the invariant violation.
fn project_folded_default(
    expression: &Expression,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<Option<PublicFoldedValue>, CompilerError> {
    if matches!(expression.kind, ExpressionKind::NoValue) {
        return Ok(None);
    }
    convert_expression_to_folded_value(expression, type_environment, string_table, context)
        .map(Some)
}

/// Project choice variants into stable variant type surfaces.
fn project_choice_variants(
    choice_definition: &ChoiceTypeDefinition,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<Vec<PublicChoiceVariantSurface>, CompilerError> {
    let mut variants = Vec::with_capacity(choice_definition.variants.len());
    for variant in choice_definition.variants.iter() {
        let name = string_table.resolve(variant.name).to_owned();

        let payload_fields = match &variant.payload {
            ChoiceVariantPayloadDefinition::Unit => Vec::new(),
            ChoiceVariantPayloadDefinition::Record { fields } => {
                let mut projected_fields = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let field_name = field
                        .name
                        .name_str(string_table)
                        .map(|name| name.to_owned())
                        .ok_or_else(|| {
                            CompilerError::compiler_error(format!(
                                "defined public type-surface projection: a choice variant \
                                 payload field has no resolvable name (path: {:?})",
                                field.name
                            ))
                        })?;

                    let type_identity = project_type_id_to_canonical_identity(
                        field.type_id,
                        type_environment,
                        context,
                    )?;

                    projected_fields.push(PublicFieldTypeSlot {
                        name: field_name,
                        type_identity,
                        folded_default: None,
                    });
                }
                projected_fields
            }
        };

        variants.push(PublicChoiceVariantSurface {
            name,
            payload_fields,
        });
    }
    Ok(variants)
}

/// Project receiver methods in deterministic receiver/method order.
///
/// WHAT: builds an index keyed by the exact stable receiver origin plus owned method name, then
/// iterates `receiver_surfaces` (already sorted by receiver origin then method name) and consumes
/// one matching entry per method. The receiver origin is resolved through
/// `TypeEnvironment::nominal_id_for_path` and the same `NominalOriginResolver` used for canonical
/// type projection, so two same-named nominal receivers from different stable module origins
/// never collide. Entries are removed as methods consume them, so a resolved entry left unmatched
/// after every surface has joined is reported explicitly. No `FxHashMap` iteration selects among
/// candidates or affects output order.
fn project_receiver_methods(
    receiver_method_entries: &[ReceiverMethodEntry],
    receiver_surfaces: &[ReceiverSurfaceOrigins],
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<
    (
        Vec<DefinedPublicReceiverMethodTypeSurface>,
        Vec<PublicCallableOriginSeed>,
    ),
    CompilerError,
> {
    // Index receiver method entries by their exact stable receiver origin plus owned method
    // name. The receiver origin is resolved through the same nominal origin resolver used for
    // canonical type projection, so the key carries full package/module identity rather than a
    // rendered name. A duplicate key or a non-nominal receiver is a `CompilerError`.
    let mut entries_by_origin: FxHashMap<(OriginTypeId, String), &ReceiverMethodEntry> =
        FxHashMap::default();
    for entry in receiver_method_entries {
        let key = receiver_method_key(entry, type_environment, context, string_table)?;
        if entries_by_origin.insert(key.clone(), entry).is_some() {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: two resolved receiver-method entries \
                 share the exact stable receiver origin and method name (receiver {:?}, \
                 method '{}'); a duplicate receiver method must not silently overwrite the first",
                key.0, key.1
            )));
        }
    }

    let mut surfaces = Vec::new();
    let mut public_callable_origin_seeds = Vec::new();

    for surface in receiver_surfaces {
        let receiver_origin = surface.receiver().clone();

        for method_origin in surface.methods() {
            let method_name = method_origin.defining_name();

            // The surface method origin already carries the exact stable receiver origin and
            // defining name, so the join key is exact. Consuming the entry guarantees each
            // resolved receiver entry joins at most one surface method.
            let key = (receiver_origin.clone(), method_name.to_owned());
            let entry = entries_by_origin.remove(&key).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: receiver method '{}' on receiver \
                        '{}' has no resolved signature entry",
                    method_name,
                    receiver_origin.defining_name()
                ))
            })?;

            // Validate that the joined method origin is a receiver method for the exact surface
            // receiver and that its defining name matches the resolved entry method name.
            let Some(joined_receiver) = method_origin.receiver() else {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: the receiver-surface method '{}' \
                     carries a free-function origin rather than a receiver method origin",
                    method_name
                )));
            };
            if joined_receiver != &receiver_origin {
                return Err(CompilerError::compiler_error(format!(
                    "defined public type-surface projection: the receiver-surface method '{}' \
                     origin names receiver {:?} but the surface receiver is {:?}",
                    method_name, joined_receiver, receiver_origin
                )));
            }

            let parameters = entry
                .signature
                .parameters
                .iter()
                .map(|declaration| {
                    let name = declaration
                        .id
                        .name_str(string_table)
                        .map(|name| name.to_owned());
                    let type_identity = project_type_id_to_canonical_identity(
                        declaration.value.type_id,
                        type_environment,
                        context,
                    )?;
                    let folded_default = project_folded_default(
                        &declaration.value,
                        type_environment,
                        context,
                        string_table,
                    )?;
                    Ok(PublicParameterTypeSlot {
                        name,
                        type_identity,
                        folded_default,
                    })
                })
                .collect::<Result<Vec<_>, CompilerError>>()?;

            let (returns, error_return) =
                project_return_slots(&entry.signature.returns, type_environment, context)?;

            surfaces.push(DefinedPublicReceiverMethodTypeSurface {
                receiver_origin: receiver_origin.clone(),
                method_origin: method_origin.clone(),
                function_path: entry.function_path.clone(),
                generic_template: false,
                parameters,
                returns,
                error_return,
            });
            push_public_callable_origin_seed(
                &mut public_callable_origin_seeds,
                entry.function_path.clone(),
                method_origin.clone(),
                false,
            )?;
        }
    }

    // Every resolved receiver entry must have joined a surface method. An entry left in the
    // index is extra: its receiver surface was not projected, so it would otherwise leak.
    // Deterministic leftover reporting: sort by receiver origin debug string then method name
    // so the error is reproducible. This is diagnostic-only and never affects the projected
    // surface.
    let mut leftover: Vec<(OriginTypeId, String)> = entries_by_origin.into_keys().collect();
    leftover.sort();
    if let Some(key) = leftover.first() {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: a resolved receiver-method entry has no \
             matching receiver surface (receiver {:?}, method '{}'); every resolved entry must \
             join exactly one surface method",
            key.0, key.1
        )));
    }

    Ok((surfaces, public_callable_origin_seeds))
}

/// Build the exact stable join key for one resolved receiver-method entry.
///
/// WHAT: resolves the entry's `ReceiverKey` nominal path to a `NominalTypeId` through the
/// `TypeEnvironment`, then to a stable `OriginTypeId` through the same `NominalOriginResolver`
/// used for canonical type projection. The key is the exact receiver origin plus the entry's
/// owned defining method name. A non-nominal receiver key is a `CompilerError`, and the resolved
/// origin category must match the receiver key variant (`Struct` -> `OriginTypeCategory::Struct`,
/// `Choice` -> `OriginTypeCategory::Choice`); a mismatch is a `CompilerError` rather than a
/// coercion. A missing nominal registration or method name is also reported.
fn receiver_method_key(
    entry: &ReceiverMethodEntry,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<(OriginTypeId, String), CompilerError> {
    let (receiver_path, expected_category) = match &entry.receiver {
        ReceiverKey::Struct(path) => (path, OriginTypeCategory::Struct),
        ReceiverKey::Choice(path) => (path, OriginTypeCategory::Choice),
        ReceiverKey::External(_) | ReceiverKey::BuiltinScalar(_) => {
            return Err(CompilerError::compiler_error(format!(
                "defined public type-surface projection: a resolved receiver-method entry \
                 carries a non-nominal receiver key ({:?}); receiver methods must live on a \
                 nominal struct or choice",
                entry.receiver
            )));
        }
    };

    let nominal_id = type_environment
        .nominal_id_for_path(receiver_path)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "defined public type-surface projection: a receiver-method entry's receiver path \
             is not a registered nominal in the TypeEnvironment (path: {:?})",
                receiver_path
            ))
        })?;

    let receiver_origin = context
        .nominal_origins()
        .resolve_nominal_origin(nominal_id)?;

    // The receiver key variant names the semantic category the resolved origin must have. A
    // struct receiver key must join a `Struct` origin and a choice receiver key a `Choice`
    // origin; a mismatch is a precise `CompilerError` rather than a silent coercion, so a
    // renamed or re-categorised declaration can never join the wrong receiver surface.
    if receiver_origin.category() != expected_category {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: a receiver-method entry's receiver key \
             expects a {expected_category:?} origin but the resolved nominal origin is a {:?} \
             (receiver path: {:?}); the receiver key and stable origin category disagree",
            receiver_origin.category(),
            receiver_path
        )));
    }

    let method_name = entry.function_path.name_str(string_table).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "defined public type-surface projection: a receiver-method entry has no resolvable \
             defining method name (path: {:?})",
            entry.function_path
        ))
    })?;

    Ok((receiver_origin, method_name.to_owned()))
}

/// Construct a `CompilerError` for a root-to-binding origin category mismatch.
fn origin_category_mismatch_error(expected: &str, binding: &ExportBinding) -> CompilerError {
    CompilerError::compiler_error(format!(
        "defined public type-surface projection: a {} root matched an export binding with \
         origin {:?} (public name '{}'); the root category and binding origin category disagree",
        expected,
        binding.origin(),
        binding.public_name()
    ))
}

#[cfg(test)]
#[path = "tests/defined_public_type_surface_tests.rs"]
mod tests;
