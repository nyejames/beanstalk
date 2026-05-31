//! Semantic type resolution for AST construction.
//!
//! WHAT: resolves parsed type syntax into canonical `TypeId`-based semantic identity using
//! module-visible declarations, type aliases, generic parameters, and external symbols.
//! WHY: semantic type resolution is an AST concern, not a syntax-parsing concern. Moving it here
//!      clarifies the boundary between shared declaration-shell parsing and AST-owned lowering.
//!
//! This module owns:
//! - `TypeResolutionContext` and its inputs
//! - recursive `resolve_type` from `DataType` parse placeholders to resolved `DataType`
//! - `resolve_parsed_type_annotation` which bridges `ParsedTypeRef` → `DataType` → `TypeId`
//! - generic nominal instantiation (lazy struct/choice generic instance interning)
//! - checked and optional diagnostic-type-to-`TypeId` conversion helpers
//!
//! This module does NOT own:
//! - token-to-parsed-ref parsing (lives in `declaration_syntax::type_syntax`)
//! - parsed-ref walkers or dependency extraction (lives in `declaration_syntax::type_syntax`)
//! - `parsed_ref_to_data_type` syntax-to-diagnostic spelling (lives in `declaration_syntax::type_syntax`)

use crate::compiler_frontend::ast::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;
use crate::compiler_frontend::ast::type_resolution::TypeResolutionResult;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, InvalidGenericInstantiationReason,
    InvalidTypeAnnotationReason, NameNamespace, NamespaceTypeValueMisuseKind,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::generic_identity_bridge::{
    BuiltinGenericType, GenericBaseType, GenericInstantiationKey, TypeIdentityKey,
    data_type_to_type_identity_key, generic_instantiation_key_argument_type_ids,
};
use crate::compiler_frontend::datatypes::generic_parameters::{
    ActiveGenericTypeContext, GenericParameterScope,
};
use crate::compiler_frontend::datatypes::ids::{
    BuiltinTypeConstructor, FunctionTypeKey, GenericParameterId, TypeConstructor, TypeId,
    builtin_type_ids,
};
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::datatypes::{DataType, diagnostic_type_spelling};
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parsed_ref_to_data_type,
};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::{NamespaceRecord, NamespaceTypeMember};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationKind, GenericDeclarationMetadata,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{SourceLocation, TokenKind};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

pub(crate) struct TypeResolutionContext<'a> {
    pub declaration_table: &'a Rc<TopLevelDeclarationTable>,
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    pub generic_declarations_by_path:
        Option<&'a FxHashMap<InternedPath, GenericDeclarationMetadata>>,
    pub generic_parameters: Option<&'a GenericParameterScope>,
    pub generic_substitutions: Option<&'a FxHashMap<GenericParameterId, TypeId>>,
    /// Resolved struct fields by canonical path, including generic struct templates.
    /// Required for lazy generic struct instantiation.
    pub resolved_struct_fields_by_path: Option<&'a FxHashMap<InternedPath, Vec<Declaration>>>,
    /// Frontend type environment for canonical type identity.
    /// WHY: enables resolution directly to TypeId instead of going through DataType.
    ///      All production type resolution must have access to the canonical environment.
    pub type_environment: &'a mut TypeEnvironment,
    /// Visible namespace records for resolving namespace-qualified type names.
    pub visible_namespace_records: Option<&'a FxHashMap<StringId, NamespaceRecord>>,
}

pub(crate) struct TypeResolutionContextInputs<'a> {
    pub declaration_table: &'a Rc<TopLevelDeclarationTable>,
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    pub generic_declarations_by_path:
        Option<&'a FxHashMap<InternedPath, GenericDeclarationMetadata>>,
    pub resolved_struct_fields_by_path: Option<&'a FxHashMap<InternedPath, Vec<Declaration>>>,
    pub type_environment: &'a mut TypeEnvironment,
    /// Visible namespace records for resolving namespace-qualified type names.
    pub visible_namespace_records: Option<&'a FxHashMap<StringId, NamespaceRecord>>,
}

impl<'a> TypeResolutionContext<'a> {
    #[cfg(test)]
    pub(crate) fn from_declaration_table(
        declaration_table: &'a Rc<TopLevelDeclarationTable>,
        type_environment: &'a mut TypeEnvironment,
    ) -> Self {
        Self {
            declaration_table,
            visible_declaration_ids: None,
            visible_external_symbols: None,
            visible_source_bindings: None,
            visible_type_aliases: None,
            resolved_type_aliases: None,
            generic_declarations_by_path: None,
            generic_parameters: None,
            generic_substitutions: None,
            resolved_struct_fields_by_path: None,
            type_environment,
            visible_namespace_records: None,
        }
    }

    pub(crate) fn from_inputs(inputs: TypeResolutionContextInputs<'a>) -> Self {
        Self {
            declaration_table: inputs.declaration_table,
            visible_declaration_ids: inputs.visible_declaration_ids,
            visible_external_symbols: inputs.visible_external_symbols,
            visible_source_bindings: inputs.visible_source_bindings,
            visible_type_aliases: inputs.visible_type_aliases,
            resolved_type_aliases: inputs.resolved_type_aliases,
            generic_declarations_by_path: inputs.generic_declarations_by_path,
            generic_parameters: None,
            generic_substitutions: None,
            resolved_struct_fields_by_path: inputs.resolved_struct_fields_by_path,
            type_environment: inputs.type_environment,
            visible_namespace_records: inputs.visible_namespace_records,
        }
    }

    pub(crate) fn with_generic_parameters(
        mut self,
        generic_parameters: Option<&'a GenericParameterScope>,
    ) -> Self {
        self.generic_parameters = generic_parameters;
        self
    }

    pub(crate) fn with_active_generic_type_context(
        mut self,
        generic_context: Option<&'a ActiveGenericTypeContext>,
    ) -> Self {
        if let Some(generic_context) = generic_context {
            self.generic_parameters = Some(&generic_context.parameter_scope);
            self.generic_substitutions = generic_context.substitutions.as_ref();
        }

        self
    }
}

/// A parsed type annotation after semantic resolution.
///
/// WHAT: carries the original parsed spelling, the resolved diagnostic spelling,
/// and the canonical `TypeId` when the source actually declared a type.
/// WHY: new AST paths should not re-derive semantic identity from `DataType`
/// after resolution. Keeping both values together makes `DataType` a diagnostic
/// companion instead of the semantic source of truth.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedTypeAnnotation {
    /// Kept with the resolved annotation so follow-up refactors can preserve source
    /// spelling through diagnostics without re-parsing or reverse-converting `DataType`.
    #[allow(dead_code)]
    pub(crate) source_ref: ParsedTypeRef,
    /// Diagnostic spelling stays attached to the TypeId for callers that still
    /// need user-facing type text during the staged migration away from `DataType`.
    #[allow(dead_code)]
    pub(crate) diagnostic_type: DataType,
    pub(crate) type_id: Option<TypeId>,
}

pub(crate) fn resolve_parsed_type_annotation(
    source_ref: ParsedTypeRef,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<ResolvedTypeAnnotation> {
    let diagnostic_type = parsed_ref_to_data_type(&source_ref);
    let resolved_diagnostic_type = resolve_type(&diagnostic_type, location, context, string_table)?;

    let type_id = if matches!(resolved_diagnostic_type, DataType::Inferred) {
        None
    } else {
        Some(resolve_diagnostic_type_to_type_id_checked(
            &resolved_diagnostic_type,
            context.type_environment,
            location,
        )?)
    };

    Ok(ResolvedTypeAnnotation {
        source_ref,
        diagnostic_type: resolved_diagnostic_type,
        type_id,
    })
}

// ------------------------------------
//  Diagnostic / parse-only DataType helpers
// ------------------------------------

/// Converts a parsed or diagnostic `DataType` into a canonical `TypeId`.
///
/// WHAT: parse-resolution bridge from written type syntax to the canonical `TypeId`-based identity.
/// WHY: header parsing, signature parsing, and type annotation resolution still produce `DataType`
///      as an intermediate parsed representation; this converts that representation to `TypeId`.
///
/// DO NOT use this to re-derive semantic `TypeId`s from `DataType` values that were already
/// resolved. Semantic type identity is `TypeId` equality in `TypeEnvironment`, not `DataType`.
///
/// PRECONDITION: `data_type` must be fully resolved. `NamedType` and `Inferred` are not valid
/// and map to `none` as a defensive fallback.
///
/// TEMPORARY: this unchecked helper remains for internal use by `instantiate_generic_nominal`
/// and `returns_diagnostic_type_to_type_id`. Production call sites outside this module should
/// prefer `resolve_diagnostic_type_to_type_id_checked` or `resolve_diagnostic_type_to_type_id_opt`.
pub(crate) fn resolve_diagnostic_type_to_type_id(
    data_type: &DataType,
    type_environment: &mut TypeEnvironment,
) -> TypeId {
    match data_type {
        DataType::Bool => type_environment.builtins().bool,
        DataType::Int => type_environment.builtins().int,
        DataType::Float => type_environment.builtins().float,
        DataType::Decimal => type_environment.builtins().decimal,
        DataType::StringSlice => type_environment.builtins().string,
        DataType::Char => type_environment.builtins().char,
        DataType::Range => type_environment.builtins().range,
        DataType::None => type_environment.builtins().none,
        DataType::Template | DataType::TemplateWrapper => type_environment.builtins().string,
        DataType::True | DataType::False => type_environment.builtins().bool,
        DataType::Option(inner) => {
            let inner_id = resolve_diagnostic_type_to_type_id(inner, type_environment);
            type_environment.intern_option(inner_id)
        }
        DataType::FallibleCarrier { success, error } => {
            let success_id = resolve_diagnostic_type_to_type_id(success, type_environment);
            let error_id = resolve_diagnostic_type_to_type_id(error, type_environment);
            type_environment.intern_fallible_carrier(success_id, error_id)
        }
        DataType::Reference(inner) => resolve_diagnostic_type_to_type_id(inner, type_environment),
        DataType::Struct {
            type_id,
            nominal_path,
            generic_instance_key,
            ..
        }
        | DataType::Choices {
            type_id,
            nominal_path,
            generic_instance_key,
            ..
        } => {
            if *type_id != builtin_type_ids::NONE && generic_instance_key.is_none() {
                // Base nominal with a resolved TypeId.
                *type_id
            } else if let Some(key) = generic_instance_key {
                // Generic instance: intern it now so that later queries return
                // substituted fields/variants. This handles cases where the
                // instance was not interned during resolve_type (e.g. because
                // the TypeResolutionContext had no TypeEnvironment).
                if let Some(nominal_id) = type_environment.nominal_id_for_path(nominal_path) {
                    if let Some(arg_ids) =
                        generic_instantiation_key_argument_type_ids(key, type_environment)
                    {
                        type_environment.intern_generic_instance(nominal_id, arg_ids)
                    } else {
                        type_environment.builtins().none
                    }
                } else {
                    type_environment.builtins().none
                }
            } else if *type_id != builtin_type_ids::NONE {
                // Generic instance that was already interned.
                *type_id
            } else {
                // Fallback for unresolved structs/choices in diagnostic-only paths.
                type_environment
                    .nominal_id_for_path(nominal_path)
                    .and_then(|nominal_id| type_environment.type_id_for_nominal_id(nominal_id))
                    .unwrap_or_else(|| type_environment.builtins().none)
            }
        }
        DataType::TypeParameter {
            canonical_id: Some(canonical_id),
            name,
            ..
        } => type_environment.intern_generic_parameter(*canonical_id, *name),
        DataType::TypeParameter {
            canonical_id: None, ..
        } => type_environment.builtins().none,
        DataType::GenericInstance { base, arguments } => match base {
            GenericBaseType::Builtin(BuiltinGenericType::Collection) => {
                let element_id =
                    resolve_diagnostic_type_to_type_id(&arguments[0], type_environment);
                type_environment.intern_constructed(
                    TypeConstructor::Builtin(BuiltinTypeConstructor::Collection),
                    Box::new([element_id]),
                )
            }
            GenericBaseType::ResolvedNominal(path) => {
                if let Some(nominal_id) = type_environment.nominal_id_for_path(path) {
                    let arg_ids: Box<[TypeId]> = arguments
                        .iter()
                        .map(|arg| resolve_diagnostic_type_to_type_id(arg, type_environment))
                        .collect();
                    type_environment.intern_generic_instance(nominal_id, arg_ids)
                } else {
                    // Base nominal not yet registered (e.g. recursive generic type during
                    // its own resolution). Fallback to none; validation will catch the cycle.
                    type_environment.builtins().none
                }
            }
            _ => type_environment.builtins().none,
        },
        DataType::Function(_, signature) => {
            let param_ids: Box<[TypeId]> = signature
                .parameters
                .iter()
                .map(|p| {
                    resolve_diagnostic_type_to_type_id(&p.value.diagnostic_type, type_environment)
                })
                .collect();
            let return_ids: Box<[TypeId]> = signature
                .returns
                .iter()
                .filter_map(|r| match &r.value {
                    FunctionReturn::Value(dt) => {
                        Some(resolve_diagnostic_type_to_type_id(dt, type_environment))
                    }
                    _ => None,
                })
                .collect();
            type_environment.intern_function(FunctionTypeKey {
                parameters: param_ids,
                returns: return_ids,
                error_return: None,
            })
        }
        DataType::Returns(values) => returns_diagnostic_type_to_type_id(values, type_environment),
        DataType::External { type_id } => type_environment.intern_external(*type_id),
        DataType::Path(_) => type_environment.builtins().string,
        DataType::Parameters(_) => type_environment.builtins().none,
        DataType::Inferred | DataType::NamedType(_) | DataType::NamespacedType { .. } => {
            // Invariant: unresolved types should not reach this function.
            type_environment.builtins().none
        }
    }
}

/// Resolve a fully checked production type spelling into canonical `TypeId` identity.
///
/// WHAT: rejects unresolved parse placeholders instead of mapping them to the builtin `None`
/// type.
/// WHY: executable AST/HIR-bound paths must not silently turn unresolved annotations into
/// valid semantic types.
pub(crate) fn resolve_diagnostic_type_to_type_id_checked(
    data_type: &DataType,
    type_environment: &mut TypeEnvironment,
    location: &SourceLocation,
) -> TypeResolutionResult<TypeId> {
    resolve_diagnostic_type_to_type_id_opt(data_type, type_environment)
        .ok_or_else(|| Box::new(unresolved_type_id_diagnostic(data_type, location)))
}

fn unresolved_type_id_diagnostic(
    data_type: &DataType,
    location: &SourceLocation,
) -> CompilerDiagnostic {
    match data_type {
        DataType::NamedType(name) => {
            CompilerDiagnostic::unknown_type_name(*name, location.to_owned())
        }
        DataType::NamespacedType { name, .. } => {
            CompilerDiagnostic::unknown_type_name(*name, location.to_owned())
        }
        DataType::GenericInstance {
            base: GenericBaseType::Named(name),
            ..
        } => CompilerDiagnostic::unknown_type_name(*name, location.to_owned()),
        _ => CompilerDiagnostic::invalid_type_annotation(
            TypeAnnotationContext::DeclarationTarget,
            InvalidTypeAnnotationReason::ExpectedTypeAnnotation {
                found: TokenKind::Eof,
            },
            location.to_owned(),
        ),
    }
}

/// Same as `resolve_diagnostic_type_to_type_id`, but returns `None` when the type cannot be
/// meaningfully resolved in the current environment (e.g. unregistered nominal
/// paths or generic instances).
///
/// WHAT: lets callers distinguish "the actual `None` type" from "fallback due
/// to unregistered type".
/// WHY: compatibility checks need to fall back to `TypeIdentityKey` comparison
/// when nominal types have not yet been registered in `TypeEnvironment`.
pub(crate) fn resolve_diagnostic_type_to_type_id_opt(
    data_type: &DataType,
    type_environment: &mut TypeEnvironment,
) -> Option<TypeId> {
    match data_type {
        DataType::Bool => Some(type_environment.builtins().bool),
        DataType::Int => Some(type_environment.builtins().int),
        DataType::Float => Some(type_environment.builtins().float),
        DataType::Decimal => Some(type_environment.builtins().decimal),
        DataType::StringSlice => Some(type_environment.builtins().string),
        DataType::Char => Some(type_environment.builtins().char),
        DataType::Range => Some(type_environment.builtins().range),
        DataType::None => Some(type_environment.builtins().none),
        DataType::Template | DataType::TemplateWrapper => Some(type_environment.builtins().string),
        DataType::True | DataType::False => Some(type_environment.builtins().bool),
        DataType::Option(inner) => {
            let inner_id = resolve_diagnostic_type_to_type_id_opt(inner, type_environment)?;
            Some(type_environment.intern_option(inner_id))
        }
        DataType::FallibleCarrier { success, error } => {
            let success_id = resolve_diagnostic_type_to_type_id_opt(success, type_environment)?;
            let error_id = resolve_diagnostic_type_to_type_id_opt(error, type_environment)?;
            Some(type_environment.intern_fallible_carrier(success_id, error_id))
        }
        DataType::Reference(inner) => {
            resolve_diagnostic_type_to_type_id_opt(inner, type_environment)
        }
        DataType::Struct {
            type_id,
            nominal_path,
            generic_instance_key,
            ..
        }
        | DataType::Choices {
            type_id,
            nominal_path,
            generic_instance_key,
            ..
        } => {
            if *type_id != builtin_type_ids::NONE && generic_instance_key.is_none() {
                Some(*type_id)
            } else if let Some(key) = generic_instance_key {
                let nominal_id = type_environment.nominal_id_for_path(nominal_path)?;
                let arg_ids = generic_instantiation_key_argument_type_ids(key, type_environment)?;
                Some(type_environment.intern_generic_instance(nominal_id, arg_ids))
            } else if *type_id != builtin_type_ids::NONE {
                Some(*type_id)
            } else {
                let nominal_id = type_environment.nominal_id_for_path(nominal_path)?;
                type_environment.type_id_for_nominal_id(nominal_id)
            }
        }
        DataType::TypeParameter {
            canonical_id: Some(canonical_id),
            name,
            ..
        } => Some(type_environment.intern_generic_parameter(*canonical_id, *name)),
        DataType::TypeParameter {
            canonical_id: None, ..
        } => None,
        DataType::GenericInstance { base, arguments } => match base {
            GenericBaseType::Builtin(BuiltinGenericType::Collection) => {
                let element_id =
                    resolve_diagnostic_type_to_type_id_opt(&arguments[0], type_environment)?;
                Some(type_environment.intern_constructed(
                    TypeConstructor::Builtin(BuiltinTypeConstructor::Collection),
                    Box::new([element_id]),
                ))
            }
            GenericBaseType::ResolvedNominal(path) => {
                let nominal_id = type_environment.nominal_id_for_path(path)?;
                let arg_ids = arguments
                    .iter()
                    .map(|arg| resolve_diagnostic_type_to_type_id_opt(arg, type_environment))
                    .collect::<Option<Vec<_>>>()?
                    .into_boxed_slice();
                Some(type_environment.intern_generic_instance(nominal_id, arg_ids))
            }
            _ => None,
        },
        DataType::Function(_, signature) => {
            let param_ids: Box<[TypeId]> = signature
                .parameters
                .iter()
                .filter_map(|p| {
                    resolve_diagnostic_type_to_type_id_opt(
                        &p.value.diagnostic_type,
                        type_environment,
                    )
                })
                .collect();
            let return_ids: Box<[TypeId]> = signature
                .returns
                .iter()
                .filter_map(|r| match &r.value {
                    FunctionReturn::Value(dt) => {
                        resolve_diagnostic_type_to_type_id_opt(dt, type_environment)
                    }
                    _ => None,
                })
                .collect();
            Some(type_environment.intern_function(FunctionTypeKey {
                parameters: param_ids,
                returns: return_ids,
                error_return: None,
            }))
        }
        DataType::Returns(values) => {
            returns_diagnostic_type_to_type_id_opt(values, type_environment)
        }
        DataType::External { type_id } => Some(type_environment.intern_external(*type_id)),
        DataType::Path(_) => Some(type_environment.builtins().string),
        DataType::Parameters(_)
        | DataType::Inferred
        | DataType::NamedType(_)
        | DataType::NamespacedType { .. } => None,
    }
}

fn returns_diagnostic_type_to_type_id(
    values: &[DataType],
    type_environment: &mut TypeEnvironment,
) -> TypeId {
    match values {
        [] => type_environment.builtins().none,
        [single] => resolve_diagnostic_type_to_type_id(single, type_environment),
        multiple => {
            let field_ids = multiple
                .iter()
                .map(|value| resolve_diagnostic_type_to_type_id(value, type_environment))
                .collect();
            type_environment.intern_tuple(field_ids)
        }
    }
}

pub(crate) fn returns_diagnostic_type_to_type_id_opt(
    values: &[DataType],
    type_environment: &mut TypeEnvironment,
) -> Option<TypeId> {
    match values {
        [] => Some(type_environment.builtins().none),
        [single] => resolve_diagnostic_type_to_type_id_opt(single, type_environment),
        multiple => {
            let field_ids = multiple
                .iter()
                .map(|value| resolve_diagnostic_type_to_type_id_opt(value, type_environment))
                .collect::<Option<Vec<_>>>()?;
            Some(type_environment.intern_tuple(field_ids))
        }
    }
}

// -----------------
//  Type resolution
// -----------------

pub(crate) fn resolve_type(
    data_type: &DataType,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<DataType> {
    match data_type {
        DataType::NamedType(type_name) => {
            resolve_named_type_from_context(*type_name, location, context, string_table)
        }
        DataType::TypeParameter { .. } => Ok(data_type.to_owned()),
        DataType::GenericInstance { base, arguments } => {
            let resolved_base =
                resolve_generic_base_type(base, arguments, location, context, string_table)?;
            let mut resolved_arguments = Vec::with_capacity(arguments.len());
            for argument in arguments {
                resolved_arguments.push(resolve_type(argument, location, context, string_table)?);
            }

            // Attempt lazy instantiation for user-declared generic structs/choices.
            if let GenericBaseType::ResolvedNominal(base_path) = &resolved_base
                && let Some(metadata) = context
                    .generic_declarations_by_path
                    .and_then(|decls| decls.get(base_path))
                && let Some(instantiated) = instantiate_generic_nominal(
                    base_path,
                    metadata,
                    &resolved_arguments,
                    location,
                    context,
                    string_table,
                )?
            {
                return Ok(instantiated);
            }

            Ok(DataType::GenericInstance {
                base: resolved_base,
                arguments: resolved_arguments,
            })
        }
        DataType::Option(inner) => {
            let resolved_inner = resolve_type(inner, location, context, string_table)?;
            reject_nested_option_type(&resolved_inner, location)?;

            Ok(DataType::Option(Box::new(resolved_inner)))
        }
        DataType::Reference(inner) => Ok(DataType::Reference(Box::new(resolve_type(
            inner,
            location,
            context,
            string_table,
        )?))),
        DataType::Returns(values) => {
            let mut resolved_values = Vec::with_capacity(values.len());
            for value in values {
                resolved_values.push(resolve_type(value, location, context, string_table)?);
            }
            Ok(DataType::Returns(resolved_values))
        }
        DataType::FallibleCarrier { success, error } => Ok(DataType::fallible_carrier(
            resolve_type(success, location, context, string_table)?,
            resolve_type(error, location, context, string_table)?,
        )),
        DataType::Function(receiver, signature) => {
            let resolved_receiver = receiver
                .as_ref()
                .as_ref()
                .map(|receiver_key| receiver_key.to_owned());

            let mut resolved_signature = signature.to_owned();
            for parameter in &mut resolved_signature.parameters {
                parameter.value.diagnostic_type = resolve_type(
                    &parameter.value.diagnostic_type,
                    &parameter.value.location,
                    context,
                    string_table,
                )?;
            }

            for return_slot in &mut resolved_signature.returns {
                match &mut return_slot.value {
                    FunctionReturn::Value(return_type) => {
                        *return_type = resolve_type(return_type, location, context, string_table)?;
                    }
                    FunctionReturn::AliasCandidates { data_type, .. } => {
                        *data_type = resolve_type(data_type, location, context, string_table)?;
                    }
                }
            }

            Ok(DataType::Function(
                Box::new(resolved_receiver),
                resolved_signature,
            ))
        }
        DataType::NamespacedType { namespace, name } => {
            resolve_namespaced_type_from_context(*namespace, *name, location, context, string_table)
        }
        DataType::Struct { .. } | DataType::Choices { .. } => {
            // Struct and choice types no longer carry field/variant payloads in DataType.
            // They are already fully resolved when created; no NamedType placeholders remain.
            Ok(data_type.to_owned())
        }
        DataType::Parameters(parameters) => {
            let mut resolved_parameters = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                let mut resolved_parameter = parameter.to_owned();
                resolved_parameter.value.diagnostic_type = resolve_type(
                    &parameter.value.diagnostic_type,
                    &parameter.value.location,
                    context,
                    string_table,
                )?;
                resolved_parameters.push(resolved_parameter);
            }

            Ok(DataType::Parameters(resolved_parameters))
        }
        _ => Ok(data_type.to_owned()),
    }
}

fn reject_nested_option_type(
    resolved_inner: &DataType,
    location: &SourceLocation,
) -> TypeResolutionResult<()> {
    if matches!(resolved_inner, DataType::Option(_)) {
        return Err(Box::new(CompilerDiagnostic::invalid_type_annotation(
            TypeAnnotationContext::DeclarationTarget,
            InvalidTypeAnnotationReason::NestedOptional,
            location.to_owned(),
        )));
    }

    Ok(())
}

// -------------------------------
//  Generic nominal instantiation
// -------------------------------

/// Resolves a generic struct or choice annotation with concrete type arguments.
///
/// WHAT: interns a canonical generic instance in `TypeEnvironment` and returns display
///       spelling for diagnostics and HIR compatibility metadata.
/// WHY: generic structs/choices must have concrete `TypeId` identity before HIR lowering.
///
/// Returns `Ok(Some(DataType))` on successful instantiation, `Ok(None)` when template data
/// is not available (call site should fall back to GenericInstance), or `Err` on failure.
fn instantiate_generic_nominal(
    base_path: &InternedPath,
    metadata: &GenericDeclarationMetadata,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    _string_table: &StringTable,
) -> TypeResolutionResult<Option<DataType>> {
    let param_count = metadata.parameters.len();
    if arguments.len() != param_count {
        return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
            base_path.name(),
            InvalidGenericInstantiationReason::WrongArgumentCount {
                expected: param_count,
                found: arguments.len(),
            },
            location.to_owned(),
        )));
    }

    // Build argument identity keys for the HIR/diagnostic compatibility bridge.
    // If any argument cannot be keyed (for example, `T` in an unresolved generic
    // function body), the canonical TypeId instance is still interned while the
    // bridge `GenericInstantiationKey` is omitted from display-only DataType data.
    let argument_keys: Option<Vec<TypeIdentityKey>> = arguments
        .iter()
        .map(data_type_to_type_identity_key)
        .collect();
    let instance_key = argument_keys.map(|arguments| GenericInstantiationKey {
        base_path: base_path.to_owned(),
        arguments,
    });

    let instantiated = match metadata.kind {
        GenericDeclarationKind::Struct => {
            let Some(fields_map) = context.resolved_struct_fields_by_path else {
                // Template data unavailable; caller should fall back to GenericInstance.
                return Ok(None);
            };
            if !fields_map.contains_key(base_path) {
                // Template not yet available (e.g. recursive generic type during its own
                // resolution). Fall back to GenericInstance so the caller can reject it
                // with a proper recursive-type diagnostic.
                return Ok(None);
            }

            // Intern the generic instance in TypeEnvironment and use its own TypeId.
            let type_id = {
                let type_environment = &mut *context.type_environment;
                if let Some(nominal_id) = type_environment.nominal_id_for_path(base_path) {
                    let arg_type_ids: Box<[TypeId]> = arguments
                        .iter()
                        .map(|arg| resolve_diagnostic_type_to_type_id(arg, type_environment))
                        .collect();
                    type_environment.intern_generic_instance(nominal_id, arg_type_ids)
                } else {
                    type_environment.builtins().none
                }
            };

            DataType::Struct {
                nominal_path: base_path.to_owned(),
                type_id,
                const_record: false,
                generic_instance_key: instance_key.to_owned(),
            }
        }
        GenericDeclarationKind::Choice => {
            // Intern the generic instance in TypeEnvironment and use its own TypeId.
            let type_id = {
                let type_environment = &mut *context.type_environment;
                if let Some(nominal_id) = type_environment.nominal_id_for_path(base_path) {
                    let arg_type_ids: Box<[TypeId]> = arguments
                        .iter()
                        .map(|arg| resolve_diagnostic_type_to_type_id(arg, type_environment))
                        .collect();
                    type_environment.intern_generic_instance(nominal_id, arg_type_ids)
                } else {
                    type_environment.builtins().none
                }
            };

            DataType::Choices {
                nominal_path: base_path.to_owned(),
                type_id,
                generic_instance_key: instance_key.to_owned(),
            }
        }
        _ => {
            // Not a generic struct or choice; fall back to GenericInstance.
            return Ok(None);
        }
    };

    Ok(Some(instantiated))
}

fn resolve_named_type_from_context(
    type_name: StringId,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<DataType> {
    // 1) Generic parameter scope.
    if let Some(generic_scope) = context.generic_parameters
        && let Some(parameter) = generic_scope.resolve(type_name)
    {
        if let Some(canonical_id) = parameter.canonical_id
            && let Some(substitutions) = context.generic_substitutions
            && let Some(concrete_type_id) = substitutions.get(&canonical_id).copied()
        {
            return Ok(diagnostic_type_spelling(
                concrete_type_id,
                context.type_environment,
            ));
        }

        return Ok(DataType::TypeParameter {
            id: parameter.local_id,
            canonical_id: parameter.canonical_id,
            name: parameter.name,
        });
    }

    // 2) Visible type aliases.
    if let Some(visible_aliases) = context.visible_type_aliases
        && let Some(alias_path) = visible_aliases.get(&type_name)
        && let Some(resolved_aliases) = context.resolved_type_aliases
        && let Some(resolved) = resolved_aliases.get(alias_path)
    {
        // Concrete generic aliases can be parsed before generic template fields are
        // fully resolved. Re-resolve the stored target in the current context so
        // aliases stay transparent once template metadata is available.
        return resolve_type(resolved, location, context, string_table);
    }

    // 3) Visible source declarations (path-based first, then name fallback).
    if let Some(visible_source_bindings) = context.visible_source_bindings
        && let Some(canonical_path) = visible_source_bindings.get(&type_name)
        && let Some(declaration) = resolve_declaration_by_path(
            context.declaration_table,
            context.visible_declaration_ids,
            canonical_path,
        )
    {
        reject_bare_generic_type_name(type_name, canonical_path, location, context, string_table)?;
        return Ok(declaration.value.diagnostic_type.to_owned());
    }

    if let Some(declaration) = visible_declaration_by_name(
        context.declaration_table,
        context.visible_declaration_ids,
        type_name,
    ) {
        reject_bare_generic_type_name(type_name, &declaration.id, location, context, string_table)?;
        return Ok(declaration.value.diagnostic_type.to_owned());
    }

    // 4) Visible external types.
    if let Some(external_symbols) = context.visible_external_symbols
        && let Some(ExternalSymbolId::Type(type_id)) = external_symbols.get(&type_name)
    {
        return Ok(DataType::External { type_id: *type_id });
    }

    // 5) Builtin type names that may still appear as named placeholders.
    if let Some(builtin_type) = builtin_named_type(type_name, string_table) {
        return Ok(builtin_type);
    }

    Err(Box::new(CompilerDiagnostic::unknown_type_name(
        type_name,
        location.to_owned(),
    )))
}

fn resolve_namespaced_type_from_context(
    namespace: StringId,
    name: StringId,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<DataType> {
    let Some(visible_namespace_records) = context.visible_namespace_records else {
        return Err(Box::new(CompilerDiagnostic::unknown_type_name(
            name,
            location.to_owned(),
        )));
    };

    let Some(record) = visible_namespace_records.get(&namespace) else {
        return Err(Box::new(CompilerDiagnostic::unknown_type_name(
            name,
            location.to_owned(),
        )));
    };

    match record.type_members.get(&name) {
        Some(NamespaceTypeMember::SourceDeclaration(canonical_path)) => {
            if let Some(declaration) =
                resolve_declaration_by_path(context.declaration_table, None, canonical_path)
            {
                reject_bare_generic_type_name(
                    name,
                    &declaration.id,
                    location,
                    context,
                    string_table,
                )?;
                return Ok(declaration.value.diagnostic_type.to_owned());
            }
            Err(Box::new(CompilerDiagnostic::unknown_type_name(
                name,
                location.to_owned(),
            )))
        }
        Some(NamespaceTypeMember::ExternalSymbol(ExternalSymbolId::Type(type_id))) => {
            Ok(DataType::External { type_id: *type_id })
        }
        _ if record.value_members.contains_key(&name) => {
            Err(Box::new(CompilerDiagnostic::namespace_type_value_misuse(
                name,
                NamespaceTypeValueMisuseKind::Type,
                NamespaceTypeValueMisuseKind::Value,
                location.to_owned(),
            )))
        }
        _ => Err(Box::new(CompilerDiagnostic::unknown_type_name(
            name,
            location.to_owned(),
        ))),
    }
}

fn resolve_generic_base_type(
    base: &GenericBaseType,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<GenericBaseType> {
    match base {
        GenericBaseType::Named(type_name) => {
            if let Some(reason) = deferred_public_result_option_syntax(*type_name, string_table) {
                return Err(Box::new(CompilerDiagnostic::deferred_feature_reason(
                    reason,
                    location.to_owned(),
                )));
            }

            if let Some(generic_scope) = context.generic_parameters
                && generic_scope.contains_name(*type_name)
            {
                return Err(Box::new(CompilerDiagnostic::namespace_misuse(
                    *type_name,
                    NameNamespace::Type,
                    NameNamespace::Value,
                    location.to_owned(),
                )));
            }

            if let Some(visible_source_bindings) = context.visible_source_bindings
                && let Some(canonical_path) = visible_source_bindings.get(type_name)
            {
                return resolve_generic_base_path(
                    Some(*type_name),
                    canonical_path,
                    arguments,
                    location,
                    context,
                    string_table,
                );
            }

            if let Some(declaration) = visible_declaration_by_name(
                context.declaration_table,
                context.visible_declaration_ids,
                *type_name,
            ) {
                return resolve_generic_base_path(
                    Some(*type_name),
                    &declaration.id,
                    arguments,
                    location,
                    context,
                    string_table,
                );
            }

            if let Some(visible_aliases) = context.visible_type_aliases
                && visible_aliases.contains_key(type_name)
            {
                return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
                    Some(*type_name),
                    InvalidGenericInstantiationReason::TypeDoesNotAcceptArguments,
                    location.to_owned(),
                )));
            }

            if let Some(external_symbols) = context.visible_external_symbols
                && matches!(
                    external_symbols.get(type_name),
                    Some(ExternalSymbolId::Type(_))
                )
            {
                return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
                    Some(*type_name),
                    InvalidGenericInstantiationReason::ExternalTypeArgumentsUnsupported,
                    location.to_owned(),
                )));
            }

            if builtin_named_type(*type_name, string_table).is_some() {
                return Err(Box::new(CompilerDiagnostic::namespace_misuse(
                    *type_name,
                    NameNamespace::Type,
                    NameNamespace::Value,
                    location.to_owned(),
                )));
            }

            Err(Box::new(CompilerDiagnostic::unknown_type_name(
                *type_name,
                location.to_owned(),
            )))
        }
        GenericBaseType::ResolvedNominal(path) => resolve_generic_base_path(
            path.name(),
            path,
            arguments,
            location,
            context,
            string_table,
        ),
        GenericBaseType::External(_) => {
            Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
                None,
                InvalidGenericInstantiationReason::ExternalTypeArgumentsUnsupported,
                location.to_owned(),
            )))
        }
        GenericBaseType::Builtin(BuiltinGenericType::Collection) => {
            // Collection is the only builtin generic type allowed in source.
            // Its arguments are resolved separately by resolve_type.
            Ok(GenericBaseType::Builtin(BuiltinGenericType::Collection))
        }
    }
}

fn deferred_public_result_option_syntax(
    type_name: StringId,
    string_table: &StringTable,
) -> Option<DeferredFeatureReason> {
    match string_table.resolve(type_name) {
        "Option" => Some(DeferredFeatureReason::PublicOptionTypeSyntax),
        "Result" => Some(DeferredFeatureReason::PublicResultTypeSyntax),
        _ => None,
    }
}

fn resolve_generic_base_path(
    visible_name: Option<StringId>,
    canonical_path: &InternedPath,
    arguments: &[DataType],
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    _string_table: &StringTable,
) -> TypeResolutionResult<GenericBaseType> {
    let Some(metadata) = context
        .generic_declarations_by_path
        .and_then(|generic_declarations| generic_declarations.get(canonical_path))
    else {
        return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
            visible_name,
            InvalidGenericInstantiationReason::TypeDoesNotAcceptArguments,
            location.to_owned(),
        )));
    };

    if !matches!(
        metadata.kind,
        GenericDeclarationKind::Struct | GenericDeclarationKind::Choice
    ) {
        return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
            visible_name,
            InvalidGenericInstantiationReason::TypeDoesNotAcceptArguments,
            location.to_owned(),
        )));
    }

    let expected = metadata.parameters.len();
    let actual = arguments.len();
    if actual != expected {
        return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
            visible_name,
            InvalidGenericInstantiationReason::WrongArgumentCount {
                expected,
                found: actual,
            },
            location.to_owned(),
        )));
    }

    Ok(GenericBaseType::ResolvedNominal(canonical_path.to_owned()))
}

fn reject_bare_generic_type_name(
    visible_name: StringId,
    canonical_path: &InternedPath,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
    _string_table: &StringTable,
) -> TypeResolutionResult<()> {
    let Some(metadata) = context
        .generic_declarations_by_path
        .and_then(|generic_declarations| generic_declarations.get(canonical_path))
    else {
        return Ok(());
    };

    if matches!(
        metadata.kind,
        GenericDeclarationKind::Struct | GenericDeclarationKind::Choice
    ) {
        return Err(Box::new(CompilerDiagnostic::invalid_generic_instantiation(
            Some(visible_name),
            InvalidGenericInstantiationReason::MissingTypeArguments,
            location.to_owned(),
        )));
    }

    Ok(())
}

fn resolve_declaration_by_path<'a>(
    declaration_table: &'a TopLevelDeclarationTable,
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    canonical_path: &InternedPath,
) -> Option<&'a Declaration> {
    declaration_table.get_visible_resolved_by_path(canonical_path, visible_declaration_ids)
}

fn visible_declaration_by_name<'a>(
    declaration_table: &'a TopLevelDeclarationTable,
    visible_declaration_ids: Option<&FxHashSet<InternedPath>>,
    name: StringId,
) -> Option<&'a Declaration> {
    declaration_table.get_visible_resolved_by_name(name, visible_declaration_ids)
}

fn builtin_named_type(type_name: StringId, string_table: &StringTable) -> Option<DataType> {
    match string_table.resolve(type_name) {
        "Int" => Some(DataType::Int),
        "Float" => Some(DataType::Float),
        "Bool" => Some(DataType::Bool),
        "String" => Some(DataType::StringSlice),
        "Char" => Some(DataType::Char),
        _ => None,
    }
}
