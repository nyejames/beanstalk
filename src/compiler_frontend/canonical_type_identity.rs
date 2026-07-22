//! Canonical cross-module type identity vocabulary and projection from module-local `TypeId`.
//!
//! WHAT: owns the owned, hashable, cross-build identity values for the closed types that a
//! `TypeEnvironment` can fully resolve without exported generic-parameter ownership, plus the
//! narrow projection owner that converts a module-local `TypeId` into one canonical identity.
//! WHY: cross-module interfaces must compare canonical type identities rather than donor-local
//! `TypeId` values. This module is the single owner of the canonical closed-type identity
//! vocabulary and its projection, so later phases embed stable identities without leaking
//! process-local IDs, source locations, absolute paths or rendered display names.
//!
//! Boundary: this module does not own `PublicSemanticInterface`, exported generic-parameter
//! ownership, or any exported surface projection. The existing
//! `datatypes::generic_identity_bridge::TypeIdentityKey` remains the module-local HIR/diagnostic
//! bridge and is not repurposed here. The two are intentionally separate:
//! `TypeIdentityKey` carries `InternedPath`, `StringId` and `ExternalTypeId` because HIR
//! lowering and diagnostics operate inside one module's `TypeEnvironment` and `StringTable`.
//! `CanonicalTypeIdentity` carries only owned, stable, cross-build values because it crosses
//! module boundaries. Consolidating their recursive shape-matching would blur the
//! HIR/diagnostic bridge boundary, so the duplication is superficial and the owners remain
//! separate.
//!
//! Dead-code allowance: the vocabulary and projection are permanent foundational types that do
//! not yet have production wiring. They are consumed by the next plan slice (Phase 7c2:
//! exported generic-parameter ownership and public semantic surface projection), which builds
//! `PublicSemanticInterface` over these identities. No earlier production caller can construct a
//! `CanonicalTypeProjectionContext` because the nominal-origin resolver is owned by the
//! provider-interface binding work that follows this slice.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{
    BuiltinTypeConstructor, BuiltinTypeKey, NominalTypeId, TypeConstructor, TypeId,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::external_packages::ExternalSymbolPath;
use crate::compiler_frontend::semantic_identity::OriginTypeId;

// ---------------------------------------------------------------------------
//  Canonical closed-type identity vocabulary
// ---------------------------------------------------------------------------

/// Owned, hashable, cross-build canonical identity for one closed type that a
/// `TypeEnvironment` can fully resolve without exported generic-parameter ownership.
///
/// WHAT: carries only stable, owned values. It never embeds `TypeId`, `NominalTypeId`,
/// `GenericParameterId`, `InternedPath`, `StringId`, `ExternalPackageId`, `ExternalTypeId`,
/// source locations, absolute paths or rendered display names.
/// WHY: this is the identity a cross-module consumer compares. Two types with the same
/// canonical identity are the same semantic type across module boundaries, checkout roots and
/// build invocations.
///
/// Transparent aliases are transparent by construction: the projection resolves an alias to its
/// target `TypeId` before producing a canonical identity, so there is no alias variant here.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CanonicalTypeIdentity {
    Builtin(CanonicalBuiltinType),
    SourceNominal(OriginTypeId),
    ExternalOpaque(ExternalOpaqueTypeIdentity),
    Collection(CollectionTypeIdentity),
    OrderedMap(OrderedMapTypeIdentity),
    Option(Box<CanonicalTypeIdentity>),
    FallibleCarrier(FallibleCarrierTypeIdentity),
    GenericInstance(GenericInstanceTypeIdentity),
}

/// Builtin scalar canonical type identity, including the semantically seeded `None` identity.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CanonicalBuiltinType {
    Bool,
    Int,
    Float,
    // Decimal is intentionally inactive in the Alpha surface. The variant is kept to mirror the
    // stable builtin TypeId layout seeded by `TypeEnvironment::new`.
    Decimal,
    String,
    Char,
    Range,
    None,
}

/// Binding-backed opaque external type identity.
///
/// WHAT: owned stable package path and structured external symbol path. Never
/// `ExternalPackageId` or `ExternalTypeId` alone.
/// WHY: a binding-backed type is identified by where it lives in its package namespace, not by a
/// build-local ID. Two builds that register the same opaque type under the same package and
/// symbol path produce the same canonical identity.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExternalOpaqueTypeIdentity {
    package_path: String,
    symbol_path: ExternalSymbolPath,
}

impl ExternalOpaqueTypeIdentity {
    /// Construct the owned stable identity from a package path and structured symbol path.
    ///
    /// Compiler-internal: only the projection owner builds these from a registry reverse lookup.
    pub(crate) fn new(package_path: String, symbol_path: ExternalSymbolPath) -> Self {
        Self {
            package_path,
            symbol_path,
        }
    }

    /// The owned stable package path spelling.
    #[allow(dead_code)]
    pub(crate) fn package_path(&self) -> &str {
        &self.package_path
    }

    /// The structured package-local symbol path.
    #[allow(dead_code)]
    pub(crate) fn symbol_path(&self) -> &ExternalSymbolPath {
        &self.symbol_path
    }
}

/// Growable or fixed collection canonical identity.
///
/// `fixed_capacity` is `None` for growable `{T}` and `Some(cap)` for fixed `{N T}`. Fixed
/// capacity is semantic identity, not an allocation hint, so the two shapes are distinct.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CollectionTypeIdentity {
    element: Box<CanonicalTypeIdentity>,
    fixed_capacity: Option<usize>,
}

impl CollectionTypeIdentity {
    /// Construct a growable or fixed collection identity.
    ///
    /// Compiler-internal: only the projection owner builds these.
    pub(crate) fn new(element: CanonicalTypeIdentity, fixed_capacity: Option<usize>) -> Self {
        Self {
            element: Box::new(element),
            fixed_capacity,
        }
    }

    /// The canonical element identity.
    #[allow(dead_code)]
    pub(crate) fn element(&self) -> &CanonicalTypeIdentity {
        &self.element
    }

    /// The fixed capacity, or `None` for a growable collection.
    #[allow(dead_code)]
    pub(crate) fn fixed_capacity(&self) -> Option<usize> {
        self.fixed_capacity
    }
}

/// Ordered map canonical identity. Key and value are stored directly so `{K = V}` order is
/// preserved.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OrderedMapTypeIdentity {
    key: Box<CanonicalTypeIdentity>,
    value: Box<CanonicalTypeIdentity>,
}

impl OrderedMapTypeIdentity {
    /// Construct an ordered map identity from canonical key and value identities.
    ///
    /// Compiler-internal: only the projection owner builds these.
    pub(crate) fn new(key: CanonicalTypeIdentity, value: CanonicalTypeIdentity) -> Self {
        Self {
            key: Box::new(key),
            value: Box::new(value),
        }
    }

    /// The canonical key identity.
    #[allow(dead_code)]
    pub(crate) fn key(&self) -> &CanonicalTypeIdentity {
        &self.key
    }

    /// The canonical value identity.
    #[allow(dead_code)]
    pub(crate) fn value(&self) -> &CanonicalTypeIdentity {
        &self.value
    }
}

/// Fallible carrier canonical identity. Success and error are stored in order.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FallibleCarrierTypeIdentity {
    success: Box<CanonicalTypeIdentity>,
    error: Box<CanonicalTypeIdentity>,
}

impl FallibleCarrierTypeIdentity {
    /// Construct a fallible carrier identity from canonical success and error identities.
    ///
    /// Compiler-internal: only the projection owner builds these.
    pub(crate) fn new(success: CanonicalTypeIdentity, error: CanonicalTypeIdentity) -> Self {
        Self {
            success: Box::new(success),
            error: Box::new(error),
        }
    }

    /// The canonical success-channel identity.
    #[allow(dead_code)]
    pub(crate) fn success(&self) -> &CanonicalTypeIdentity {
        &self.success
    }

    /// The canonical error-channel identity.
    #[allow(dead_code)]
    pub(crate) fn error(&self) -> &CanonicalTypeIdentity {
        &self.error
    }
}

/// Concrete source nominal generic instance canonical identity.
///
/// WHAT: keyed by the stable base `OriginTypeId` plus recursively canonical concrete arguments.
/// WHY: two instances of the same generic nominal with the same canonical arguments share one
/// canonical identity across module boundaries.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GenericInstanceTypeIdentity {
    base: OriginTypeId,
    arguments: Box<[CanonicalTypeIdentity]>,
}

impl GenericInstanceTypeIdentity {
    /// Construct a concrete generic instance identity from a stable base origin and canonical
    /// concrete arguments.
    ///
    /// Compiler-internal: only the projection owner builds these after validating exact arity.
    pub(crate) fn new(base: OriginTypeId, arguments: Box<[CanonicalTypeIdentity]>) -> Self {
        Self { base, arguments }
    }

    /// The stable base nominal origin identity.
    #[allow(dead_code)]
    pub(crate) fn base(&self) -> &OriginTypeId {
        &self.base
    }

    /// The recursively canonical concrete arguments.
    #[allow(dead_code)]
    pub(crate) fn arguments(&self) -> &[CanonicalTypeIdentity] {
        &self.arguments
    }
}

// ---------------------------------------------------------------------------
//  Projection context
// ---------------------------------------------------------------------------

/// Resolves a module-local `NominalTypeId` to its stable source-nominal `OriginTypeId`.
///
/// WHAT: the projection receives this resolver so it can map source nominal struct/choice types
/// to their stable cross-module origin without embedding donor-local `NominalTypeId` values in
/// the canonical identity.
/// WHY: a missing source nominal origin is a `CompilerError`, never a silently omitted fact. The
/// resolver is supplied by the provider-interface binding owner (Phase 7c2), not by the
/// projection itself. For focused tests a simple map-backed implementation is sufficient.
pub(crate) trait NominalOriginResolver {
    /// Returns the stable origin identity for a module-local nominal, or a `CompilerError` when
    /// the nominal has no exported origin.
    fn resolve_nominal_origin(
        &self,
        nominal_id: NominalTypeId,
    ) -> Result<OriginTypeId, CompilerError>;
}

/// Explicit context for projecting a `TypeId` into a canonical identity.
///
/// WHAT: carries the source-nominal origin resolver and the external package registry. Both are
/// borrowed for the duration of the projection.
/// WHY: keeps the projection function's signature narrow and explicit about its two external
/// dependencies. The projection itself owns no state.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
pub(crate) struct CanonicalTypeProjectionContext<'a> {
    nominal_origins: &'a dyn NominalOriginResolver,
    external_registry: &'a ExternalPackageRegistry,
}

impl<'a> CanonicalTypeProjectionContext<'a> {
    /// Construct the projection context from its two borrowed dependencies.
    ///
    /// Compiler-internal: the provider-interface binding owner (Phase 7c2) builds this once per
    /// module compilation. Focused tests build it directly.
    #[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
    pub(crate) fn new(
        nominal_origins: &'a dyn NominalOriginResolver,
        external_registry: &'a ExternalPackageRegistry,
    ) -> Self {
        Self {
            nominal_origins,
            external_registry,
        }
    }
}

// ---------------------------------------------------------------------------
//  Projection
// ---------------------------------------------------------------------------

/// Projects a module-local `TypeId` into a canonical cross-module type identity.
///
/// WHAT: reads the `TypeDefinition` for `type_id` from `type_environment`, resolves source
/// nominal origins through the context, resolves binding-backed opaque types through the
/// external registry, and recursively projects constructed and generic-instance arguments.
/// WHY: this is the single owner of the `TypeId -> CanonicalTypeIdentity` conversion. It
/// returns `CompilerError` for every incomplete or unsupported state instead of returning
/// `None`, using a sentinel, guessing from rendered names or panicking.
///
/// The following states return `CompilerError` with precise invariant context:
/// - missing source nominal origin (the nominal has no exported `OriginTypeId`)
/// - missing external stable identity (the `ExternalTypeId` was never registered)
/// - function types (not a closed canonical type)
/// - tuple and other internal-only constructed shapes
/// - unresolved generic parameters
/// - malformed constructed arity (wrong argument count for a builtin constructor)
/// - malformed generic-instance arity (argument count differs from the base nominal's declared
///   generic parameter count)
///
/// Transparent aliases are transparent: if the `TypeEnvironment` ever stores an alias, the
/// projection follows its resolved target `TypeId` and does not manufacture an alias variant.
#[allow(dead_code)] // Consumed by Phase 7c2 public semantic surface projection.
pub(crate) fn project_type_id_to_canonical_identity(
    type_id: TypeId,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
) -> Result<CanonicalTypeIdentity, CompilerError> {
    let definition = type_environment.get(type_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "canonical type projection received an unregistered TypeId({}); the TypeEnvironment \
             has no definition for it, so this is an internal invariant violation",
            type_id.0
        ))
    })?;

    match definition {
        TypeDefinition::Builtin(builtin) => {
            Ok(CanonicalTypeIdentity::Builtin(project_builtin(builtin.key)))
        }
        TypeDefinition::Struct(def) => {
            let origin = context
                .nominal_origins
                .resolve_nominal_origin(def.id)
                .map_err(|error| {
                    CompilerError::compiler_error(format!(
                        "canonical type projection could not resolve a source-nominal origin for \
                         struct NominalTypeId({}): {error_msg}",
                        def.id.0,
                        error_msg = error.msg
                    ))
                })?;
            Ok(CanonicalTypeIdentity::SourceNominal(origin))
        }
        TypeDefinition::Choice(def) => {
            let origin = context
                .nominal_origins
                .resolve_nominal_origin(def.id)
                .map_err(|error| {
                    CompilerError::compiler_error(format!(
                        "canonical type projection could not resolve a source-nominal origin for \
                         choice NominalTypeId({}): {error_msg}",
                        def.id.0,
                        error_msg = error.msg
                    ))
                })?;
            Ok(CanonicalTypeIdentity::SourceNominal(origin))
        }
        TypeDefinition::External(def) => {
            let (package_path, symbol_path) = context
                .external_registry
                .resolve_type_package_and_symbol_path(def.type_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "canonical type projection could not resolve a stable package/symbol \
                         identity for ExternalTypeId({}); the type was not registered through \
                         the single registration path, so this is an inconsistent-registry \
                         invariant",
                        def.type_id.0
                    ))
                })?;
            Ok(CanonicalTypeIdentity::ExternalOpaque(
                ExternalOpaqueTypeIdentity::new(package_path.to_owned(), symbol_path.clone()),
            ))
        }
        TypeDefinition::Constructed(constructed) => {
            project_constructed(constructed, type_environment, context)
        }
        TypeDefinition::GenericInstance(instance) => {
            project_generic_instance(instance, type_environment, context)
        }
        TypeDefinition::Function(_) => Err(CompilerError::compiler_error(format!(
            "canonical type projection does not support function types; TypeId({}) is a function \
             type, which is not a closed canonical type identity",
            type_id.0
        ))),
        TypeDefinition::GenericParameter(_) => Err(CompilerError::compiler_error(format!(
            "canonical type projection does not support unresolved generic parameters; \
             TypeId({}) is a generic parameter, which has no canonical closed-type identity \
             without exported generic-parameter ownership",
            type_id.0
        ))),
    }
}

/// Maps a builtin scalar key to its canonical builtin identity.
fn project_builtin(key: BuiltinTypeKey) -> CanonicalBuiltinType {
    match key {
        BuiltinTypeKey::Bool => CanonicalBuiltinType::Bool,
        BuiltinTypeKey::Int => CanonicalBuiltinType::Int,
        BuiltinTypeKey::Float => CanonicalBuiltinType::Float,
        BuiltinTypeKey::Decimal => CanonicalBuiltinType::Decimal,
        BuiltinTypeKey::String => CanonicalBuiltinType::String,
        BuiltinTypeKey::Char => CanonicalBuiltinType::Char,
        BuiltinTypeKey::Range => CanonicalBuiltinType::Range,
        BuiltinTypeKey::None => CanonicalBuiltinType::None,
    }
}

/// Projects a constructed type, validating exact arity for each builtin constructor.
fn project_constructed(
    constructed: &crate::compiler_frontend::datatypes::definitions::ConstructedTypeDefinition,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
) -> Result<CanonicalTypeIdentity, CompilerError> {
    let arguments = constructed.arguments.as_ref();
    match &constructed.constructor {
        TypeConstructor::Builtin(BuiltinTypeConstructor::Collection { fixed_capacity }) => {
            let [element_id] = arguments else {
                return Err(malformed_arity_error("collection", 1, arguments.len()));
            };
            let element =
                project_type_id_to_canonical_identity(*element_id, type_environment, context)?;
            Ok(CanonicalTypeIdentity::Collection(
                CollectionTypeIdentity::new(element, *fixed_capacity),
            ))
        }
        TypeConstructor::Builtin(BuiltinTypeConstructor::Option) => {
            let [inner_id] = arguments else {
                return Err(malformed_arity_error("option", 1, arguments.len()));
            };
            let inner =
                project_type_id_to_canonical_identity(*inner_id, type_environment, context)?;
            Ok(CanonicalTypeIdentity::Option(Box::new(inner)))
        }
        TypeConstructor::Builtin(BuiltinTypeConstructor::FallibleCarrier) => {
            let [success_id, error_id] = arguments else {
                return Err(malformed_arity_error(
                    "fallible carrier",
                    2,
                    arguments.len(),
                ));
            };
            let success =
                project_type_id_to_canonical_identity(*success_id, type_environment, context)?;
            let error =
                project_type_id_to_canonical_identity(*error_id, type_environment, context)?;
            Ok(CanonicalTypeIdentity::FallibleCarrier(
                FallibleCarrierTypeIdentity::new(success, error),
            ))
        }
        TypeConstructor::Builtin(BuiltinTypeConstructor::OrderedMap) => {
            let [key_id, value_id] = arguments else {
                return Err(malformed_arity_error("ordered map", 2, arguments.len()));
            };
            let key = project_type_id_to_canonical_identity(*key_id, type_environment, context)?;
            let value =
                project_type_id_to_canonical_identity(*value_id, type_environment, context)?;
            Ok(CanonicalTypeIdentity::OrderedMap(
                OrderedMapTypeIdentity::new(key, value),
            ))
        }
        TypeConstructor::Builtin(BuiltinTypeConstructor::Tuple) => {
            Err(CompilerError::compiler_error(
                "canonical type projection does not support tuple or internal-only constructed \
                 shapes; tuples are not part of the canonical closed-type identity vocabulary",
            ))
        }
    }
}

/// Projects a generic instance, validating that the argument count matches the base nominal's
/// declared generic parameter count.
fn project_generic_instance(
    instance: &crate::compiler_frontend::datatypes::definitions::GenericInstanceDefinition,
    type_environment: &TypeEnvironment,
    context: &CanonicalTypeProjectionContext,
) -> Result<CanonicalTypeIdentity, CompilerError> {
    let base_origin = context
        .nominal_origins
        .resolve_nominal_origin(instance.base)
        .map_err(|error| {
            CompilerError::compiler_error(format!(
                "canonical type projection could not resolve a source-nominal origin for \
                 generic-instance base NominalTypeId({}): {error_msg}",
                instance.base.0,
                error_msg = error.msg
            ))
        })?;

    let expected_arity = validate_generic_instance_base_arity(instance.base, type_environment)?;
    if instance.arguments.len() != expected_arity {
        return Err(CompilerError::compiler_error(format!(
            "canonical type projection found a malformed generic-instance arity: \
             NominalTypeId({}) declares {expected_arity} generic parameters but the instance \
             carries {} concrete arguments",
            instance.base.0,
            instance.arguments.len()
        )));
    }

    let mut projected_arguments = Vec::with_capacity(instance.arguments.len());
    for argument_id in instance.arguments.iter() {
        let projected =
            project_type_id_to_canonical_identity(*argument_id, type_environment, context)?;
        projected_arguments.push(projected);
    }

    Ok(CanonicalTypeIdentity::GenericInstance(
        GenericInstanceTypeIdentity::new(base_origin, projected_arguments.into_boxed_slice()),
    ))
}

/// Validates the generic-instance base and returns its declared generic parameter count.
///
/// WHAT: rejects an unknown/missing nominal base, a struct or choice base whose
/// `generic_parameters` is `None` (even when the instance carries zero arguments), and a
/// referenced generic parameter list missing from `TypeEnvironment`.
/// WHY: a generic instance must be built from a nominal that actually declares a complete
/// generic parameter list. The previous silent `0` fallback let a zero-argument instance of a
/// non-generic nominal project as if it were a legal concrete instance.
fn validate_generic_instance_base_arity(
    nominal_id: NominalTypeId,
    type_environment: &TypeEnvironment,
) -> Result<usize, CompilerError> {
    let (generic_parameters, kind) = match (
        type_environment.struct_definition(nominal_id),
        type_environment.choice_definition(nominal_id),
    ) {
        (Some(def), _) => (def.generic_parameters, "struct"),
        (None, Some(def)) => (def.generic_parameters, "choice"),
        (None, None) => {
            return Err(CompilerError::compiler_error(format!(
                "canonical type projection found a generic-instance base NominalTypeId({}) that \
                 is neither a registered struct nor a choice; a generic instance must be built \
                 from a known nominal base",
                nominal_id.0
            )));
        }
    };

    let list_id = generic_parameters.ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "canonical type projection found a generic instance of {kind} \
             NominalTypeId({}) whose generic parameter list is absent; a generic instance \
             requires a base that actually declares generic parameters",
            nominal_id.0
        ))
    })?;

    let list = type_environment
        .generic_parameters(list_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "canonical type projection found a generic instance of {kind} \
             NominalTypeId({}) whose declared generic parameter list \
             GenericParameterListId({}) is missing from the TypeEnvironment",
                nominal_id.0, list_id.0
            ))
        })?;

    Ok(list.parameters.len())
}

/// Constructs a `CompilerError` for a malformed constructed-type arity.
fn malformed_arity_error(constructor_name: &str, expected: usize, actual: usize) -> CompilerError {
    CompilerError::compiler_error(format!(
        "canonical type projection found a malformed {constructor_name} arity: expected \
         {expected} argument(s) but the constructed type carries {actual}"
    ))
}
