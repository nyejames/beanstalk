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
use crate::compiler_frontend::semantic_identity::{
    DefinedPublicExportOrigins, ExportBinding, OriginConstantId, OriginDeclarationId,
    OriginFunctionId, OriginTraitId, OriginTypeCategory, OriginTypeId, ReceiverSurfaceOrigins,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::traits::environment::CoreTraitKind;
use crate::compiler_frontend::traits::ids::TraitId;

use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
//  Stable type-surface value types
// ---------------------------------------------------------------------------

/// The defined public TYPE surface for one compiled module.
///
/// WHAT: carries only owned, stable values: canonical type identities and owned authored names.
/// It never embeds `TypeId`, `NominalTypeId`, `GenericParameterId`, `InternedPath`, `StringId`,
/// source locations, absolute paths or donor-local external numeric IDs.
///
/// It is deliberately not `PublicSemanticInterface`. It carries type shapes only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicTypeSurface {
    free_functions: Vec<DefinedPublicFunctionTypeSurface>,
    nominal_types: Vec<DefinedPublicNominalTypeSurface>,
    transparent_aliases: Vec<DefinedPublicAliasTypeSurface>,
    constants: Vec<DefinedPublicConstantTypeSurface>,
    receiver_methods: Vec<DefinedPublicReceiverMethodTypeSurface>,
}

impl DefinedPublicTypeSurface {
    /// The exported free-function type surfaces, in deterministic declaration order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn free_functions(&self) -> &[DefinedPublicFunctionTypeSurface] {
        &self.free_functions
    }

    /// The exported nominal type surfaces (structs and choices), in deterministic order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn nominal_types(&self) -> &[DefinedPublicNominalTypeSurface] {
        &self.nominal_types
    }

    /// The exported transparent alias type surfaces, in deterministic order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn transparent_aliases(&self) -> &[DefinedPublicAliasTypeSurface] {
        &self.transparent_aliases
    }

    /// The exported constant type surfaces, in deterministic order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn constants(&self) -> &[DefinedPublicConstantTypeSurface] {
        &self.constants
    }

    /// The exported receiver-method type surfaces, in deterministic receiver/method order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn receiver_methods(&self) -> &[DefinedPublicReceiverMethodTypeSurface] {
        &self.receiver_methods
    }
}

/// One exported parameter slot in a function or receiver-method type surface.
///
/// `name` is the owned authored parameter name, or `None` when the source signature omits it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DefinedPublicParameterTypeSlot {
    name: Option<String>,
    type_identity: CanonicalTypeIdentity,
}

impl DefinedPublicParameterTypeSlot {
    /// The owned authored parameter name, or `None` when the source signature omits it.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The canonical type identity of this parameter slot.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn type_identity(&self) -> &CanonicalTypeIdentity {
        &self.type_identity
    }
}

/// One exported return slot in a function or receiver-method type surface.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DefinedPublicReturnTypeSlot {
    type_identity: CanonicalTypeIdentity,
}

impl DefinedPublicReturnTypeSlot {
    /// The canonical type identity of this return slot.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn type_identity(&self) -> &CanonicalTypeIdentity {
        &self.type_identity
    }
}

/// One exported generic parameter with its ordered canonical trait bound identities.
///
/// WHAT: pairs the stable [`ExportedGenericParameterIdentity`] (declaration owner + position +
/// authored name, unchanged) with an ordered `Vec<CanonicalTraitIdentity>` resolved from the
/// `TypeEnvironment`'s declaration-site `TraitId` bounds. The identity never carries bounds;
/// the bounds are a separate fact on this surface entry.
/// WHY: the exported generic parameter surface must carry both identity and bounds so a
/// cross-module consumer can see the full constraint shape without donor-local `TraitId`,
/// `GenericParameterId`, `InternedPath`, `StringId`, `FileId`, `CoreTraitKind` registry handle
/// or source location.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DefinedPublicGenericParameterSurface {
    identity: ExportedGenericParameterIdentity,
    bounds: Vec<CanonicalTraitIdentity>,
}

impl DefinedPublicGenericParameterSurface {
    /// The stable exported generic parameter identity (owner + position + authored name).
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn identity(&self) -> &ExportedGenericParameterIdentity {
        &self.identity
    }

    /// The ordered canonical trait bound identities, in declaration-site bound order.
    ///
    /// Empty when the parameter has no bounds.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn bounds(&self) -> &[CanonicalTraitIdentity] {
        &self.bounds
    }
}

/// The type-only surface for one exported free function.
///
/// `parameters` and `returns` preserve authored order. `error_return` is `None` when the
/// function has no error channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicFunctionTypeSurface {
    origin: OriginFunctionId,
    generic_parameters: Vec<DefinedPublicGenericParameterSurface>,
    parameters: Vec<DefinedPublicParameterTypeSlot>,
    returns: Vec<DefinedPublicReturnTypeSlot>,
    error_return: Option<CanonicalTypeIdentity>,
}

impl DefinedPublicFunctionTypeSurface {
    /// The stable origin of this free function.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn origin(&self) -> &OriginFunctionId {
        &self.origin
    }

    /// The exported generic parameter surfaces in declaration-local order.
    ///
    /// Empty for a non-generic free function. Each surface entry carries the stable parameter
    /// identity plus its ordered canonical trait bound identities.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn generic_parameters(&self) -> &[DefinedPublicGenericParameterSurface] {
        &self.generic_parameters
    }

    /// The parameter type slots in authored order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn parameters(&self) -> &[DefinedPublicParameterTypeSlot] {
        &self.parameters
    }

    /// The success-channel return type slots in authored order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn returns(&self) -> &[DefinedPublicReturnTypeSlot] {
        &self.returns
    }

    /// The error-channel return type, if any.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn error_return(&self) -> Option<&CanonicalTypeIdentity> {
        self.error_return.as_ref()
    }
}

/// One exported field in a struct or choice-variant-payload type surface.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DefinedPublicFieldTypeSlot {
    name: String,
    type_identity: CanonicalTypeIdentity,
}

impl DefinedPublicFieldTypeSlot {
    /// The owned authored field name.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The canonical type identity of this field.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn type_identity(&self) -> &CanonicalTypeIdentity {
        &self.type_identity
    }
}

/// One exported choice variant in a choice type surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicChoiceVariantTypeSurface {
    name: String,
    payload_fields: Vec<DefinedPublicFieldTypeSlot>,
}

impl DefinedPublicChoiceVariantTypeSurface {
    /// The owned authored variant name.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The payload field type slots for a record variant. Empty for unit variants.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn payload_fields(&self) -> &[DefinedPublicFieldTypeSlot] {
        &self.payload_fields
    }
}

/// The type-only surface for one exported nominal type (struct or choice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicNominalTypeSurface {
    origin: OriginTypeId,
    generic_parameters: Vec<DefinedPublicGenericParameterSurface>,
    fields: Vec<DefinedPublicFieldTypeSlot>,
    variants: Vec<DefinedPublicChoiceVariantTypeSurface>,
}

impl DefinedPublicNominalTypeSurface {
    /// The stable origin of this nominal type.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn origin(&self) -> &OriginTypeId {
        &self.origin
    }

    /// The exported generic parameter surfaces in declaration-local order.
    ///
    /// Empty for a non-generic struct or choice. Each surface entry carries the stable parameter
    /// identity plus its ordered canonical trait bound identities. Receiver methods reuse their
    /// receiver nominal's parameters and expose no independent list.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn generic_parameters(&self) -> &[DefinedPublicGenericParameterSurface] {
        &self.generic_parameters
    }

    /// The struct field type slots in authored order. Empty for choices.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn fields(&self) -> &[DefinedPublicFieldTypeSlot] {
        &self.fields
    }

    /// The choice variant surfaces in authored order. Empty for structs.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn variants(&self) -> &[DefinedPublicChoiceVariantTypeSurface] {
        &self.variants
    }
}

/// The type-only surface for one exported transparent alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicAliasTypeSurface {
    origin: OriginTypeId,
    target_type_identity: CanonicalTypeIdentity,
}

impl DefinedPublicAliasTypeSurface {
    /// The stable origin of this alias.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn origin(&self) -> &OriginTypeId {
        &self.origin
    }

    /// The canonical identity of the alias target type.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn target_type_identity(&self) -> &CanonicalTypeIdentity {
        &self.target_type_identity
    }
}

/// The type-only surface for one exported constant.
///
/// Only the canonical type is exposed in this slice; folded values remain for a later phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicConstantTypeSurface {
    origin: OriginConstantId,
    type_identity: CanonicalTypeIdentity,
}

impl DefinedPublicConstantTypeSurface {
    /// The stable origin of this constant.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn origin(&self) -> &OriginConstantId {
        &self.origin
    }

    /// The canonical type identity of the constant.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn type_identity(&self) -> &CanonicalTypeIdentity {
        &self.type_identity
    }
}

/// The type-only surface for one exported receiver method.
///
/// The method stays attached to its stable receiver origin. It is not a free namespace entry
/// and cannot be imported or re-exported separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DefinedPublicReceiverMethodTypeSurface {
    receiver_origin: OriginTypeId,
    method_origin: OriginFunctionId,
    parameters: Vec<DefinedPublicParameterTypeSlot>,
    returns: Vec<DefinedPublicReturnTypeSlot>,
    error_return: Option<CanonicalTypeIdentity>,
}

impl DefinedPublicReceiverMethodTypeSurface {
    /// The stable origin of the receiver type that owns this method.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn receiver_origin(&self) -> &OriginTypeId {
        &self.receiver_origin
    }

    /// The stable origin of this receiver method.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn method_origin(&self) -> &OriginFunctionId {
        &self.method_origin
    }

    /// The parameter type slots in authored order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn parameters(&self) -> &[DefinedPublicParameterTypeSlot] {
        &self.parameters
    }

    /// The success-channel return type slots in authored order.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn returns(&self) -> &[DefinedPublicReturnTypeSlot] {
        &self.returns
    }

    /// The error-channel return type, if any.
    #[allow(dead_code)] // Test-only: asserted by focused surface projection tests.
    pub(crate) fn error_return(&self) -> Option<&CanonicalTypeIdentity> {
        self.error_return.as_ref()
    }
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
struct TransientNominalOriginResolver<'a> {
    type_environment: &'a TypeEnvironment,
    public_source_nominal_type_origins: &'a FxHashMap<InternedPath, OriginTypeId>,
}

impl<'a> TransientNominalOriginResolver<'a> {
    fn new(
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
/// The output carries only owned stable values. No `TypeId`, `NominalTypeId`,
/// `GenericParameterId`, `GenericParameterListId`, `TraitId`, `CoreTraitKind`, `InternedPath` or
/// `StringId` crosses the boundary: these donor-local facts are consumed transiently and excluded
/// from the stable surface.
pub(crate) fn build_defined_public_type_surface(
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
    public_source_nominal_type_origins: &FxHashMap<InternedPath, OriginTypeId>,
    public_source_trait_origins: &FxHashMap<InternedPath, OriginTraitId>,
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
                free_functions.push(project_free_function(
                    function_origin.clone(),
                    *generic_parameter_list_id,
                    signature,
                    type_environment,
                    &projection_context,
                    &root_table.trait_source_facts,
                    public_source_trait_origins,
                    string_table,
                )?);
            }
            ResolvedPublicTypeRootKind::Struct { type_id } => {
                let OriginDeclarationId::Type(type_origin) = binding.origin() else {
                    return Err(origin_category_mismatch_error("struct", binding));
                };
                if type_origin.category() != OriginTypeCategory::Struct {
                    return Err(origin_category_mismatch_error("struct", binding));
                }
                nominal_types.push(project_struct(
                    type_origin.clone(),
                    *type_id,
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
    let receiver_methods = project_receiver_methods(
        &root_table.receiver_methods,
        export_origins.receiver_surfaces(),
        type_environment,
        &projection_context,
        string_table,
    )?;

    Ok(DefinedPublicTypeSurface {
        free_functions,
        nominal_types,
        transparent_aliases,
        constants,
        receiver_methods,
    })
}

/// Register generic-parameter origins from function and nominal roots.
///
/// Free functions with a `GenericParameterListId` register their parameters under a
/// `GenericDeclarationOrigin::free_function`. Struct/choice roots register their parameters
/// under a `GenericDeclarationOrigin::nominal_type`. Receiver methods do not register
/// parameters: they reuse their receiver nominal's parameters and must not become owners.
fn register_generic_parameter_origins(
    generic_resolver: &mut TransientGenericParameterOriginResolver,
    root_table: &ResolvedPublicTypeRootTable,
    export_origins: &DefinedPublicExportOrigins,
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

            ResolvedPublicTypeRootKind::Struct { type_id } => {
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
) -> Result<Vec<DefinedPublicGenericParameterSurface>, CompilerError> {
    let Some(list_id) = generic_parameter_list_id else {
        return Ok(Vec::new());
    };

    let list = type_environment.generic_parameters(list_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "defined public type-surface projection: GenericParameterListId({}) is missing from the TypeEnvironment while projecting exported generic parameter surfaces",
            list_id.0
        ))
    })?;

    let mut surfaces: Vec<DefinedPublicGenericParameterSurface> =
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
        if surfaces
            .iter()
            .any(|surface| surface.identity() == &identity)
        {
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

        surfaces.push(DefinedPublicGenericParameterSurface { identity, bounds });
    }

    Ok(surfaces)
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

        let canonical_identity = match source_fact {
            ResolvedTraitSourceFact::Source(path) => {
                let Some(origin) = public_source_trait_origins.get(path) else {
                    return Err(CompilerError::compiler_error(format!(
                        "defined public type-surface projection: a generic parameter bound trait source path {:?} has no retained public source-trait origin; a private, unexported or unowned trait must not enter the public type surface",
                        path
                    )));
                };
                CanonicalTraitIdentity::Source(origin.clone())
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
                CanonicalTraitIdentity::Core(core_identity)
            }
        };

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
            Ok(DefinedPublicParameterTypeSlot {
                name,
                type_identity,
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
) -> Result<
    (
        Vec<DefinedPublicReturnTypeSlot>,
        Option<CanonicalTypeIdentity>,
    ),
    CompilerError,
> {
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
            ReturnChannel::Success => returns.push(DefinedPublicReturnTypeSlot { type_identity }),
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

    let fields = project_fields(struct_definition, type_environment, context, string_table)?;

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

/// Project struct fields into stable field type slots.
fn project_fields(
    struct_definition: &StructTypeDefinition,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<Vec<DefinedPublicFieldTypeSlot>, CompilerError> {
    let mut fields = Vec::with_capacity(struct_definition.fields.len());
    for field in struct_definition.fields.iter() {
        let name = field
            .name
            .name_str(string_table)
            .map(|name| name.to_owned())
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "defined public type-surface projection: a struct field has no resolvable \
                     name (path: {:?})",
                    field.name
                ))
            })?;

        let type_identity =
            project_type_id_to_canonical_identity(field.type_id, type_environment, context)?;

        fields.push(DefinedPublicFieldTypeSlot {
            name,
            type_identity,
        });
    }
    Ok(fields)
}

/// Project choice variants into stable variant type surfaces.
fn project_choice_variants(
    choice_definition: &ChoiceTypeDefinition,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
    string_table: &StringTable,
) -> Result<Vec<DefinedPublicChoiceVariantTypeSurface>, CompilerError> {
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

                    projected_fields.push(DefinedPublicFieldTypeSlot {
                        name: field_name,
                        type_identity,
                    });
                }
                projected_fields
            }
        };

        variants.push(DefinedPublicChoiceVariantTypeSurface {
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
) -> Result<Vec<DefinedPublicReceiverMethodTypeSurface>, CompilerError> {
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
                    Ok(DefinedPublicParameterTypeSlot {
                        name,
                        type_identity,
                    })
                })
                .collect::<Result<Vec<_>, CompilerError>>()?;

            let (returns, error_return) =
                project_return_slots(&entry.signature.returns, type_environment, context)?;

            surfaces.push(DefinedPublicReceiverMethodTypeSurface {
                receiver_origin: receiver_origin.clone(),
                method_origin: method_origin.clone(),
                parameters,
                returns,
                error_return,
            });
        }
    }

    // Every resolved receiver entry must have joined a surface method. An entry left in the
    // index is extra: its receiver surface was not projected, so it would otherwise leak.
    // Deterministic leftover reporting: sort by receiver origin debug string then method name
    // so the error is reproducible. This is diagnostic-only and never affects the projected
    // surface.
    let mut leftover: Vec<(OriginTypeId, String)> = entries_by_origin.into_keys().collect();
    leftover.sort_by(|left, right| {
        format!("{:?}", left.0)
            .cmp(&format!("{:?}", right.0))
            .then_with(|| left.1.cmp(&right.1))
    });
    if let Some(key) = leftover.first() {
        return Err(CompilerError::compiler_error(format!(
            "defined public type-surface projection: a resolved receiver-method entry has no \
             matching receiver surface (receiver {:?}, method '{}'); every resolved entry must \
             join exactly one surface method",
            key.0, key.1
        )));
    }

    Ok(surfaces)
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
