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
use crate::compiler_frontend::ast::const_values::resolver::ConstResolutionError;
use crate::compiler_frontend::ast::const_values::resolver::{
    ConstValueEnvironment, ConstValueResolver,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_until;
use crate::compiler_frontend::ast::generic_bounds::{
    GenericBoundEvidenceContext, validate_nominal_generic_bound_evidence,
};
use crate::compiler_frontend::ast::module_ast::scope_context::ScopeContext;
use crate::compiler_frontend::ast::statements::functions::FunctionReturn;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::type_resolution::TypeResolutionResult;
use crate::compiler_frontend::compiler_messages::{
    BoundOnlyTraitDiagnosticReason, CompilerDiagnostic, DeferredFeatureReason,
    InvalidCollectionTypeReason, InvalidDynamicTraitTypeReason, InvalidGenericInstantiationReason,
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
    FunctionTypeKey, GenericParameterId, TypeId, builtin_type_ids,
};
use crate::compiler_frontend::datatypes::parsed::ParsedCollectionCapacity;
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
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token};
use crate::compiler_frontend::tokenizer::tokens::{SourceLocation, TokenKind};
use crate::compiler_frontend::traits::definitions::{BoundOnlyTraitReason, TraitDynamicSafety};
use crate::compiler_frontend::traits::environment::TraitEnvironment;
use crate::compiler_frontend::traits::evidence::TraitEvidenceEnvironment;
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

pub(crate) struct TypeResolutionContext<'a> {
    pub declaration_table: &'a Rc<TopLevelDeclarationTable>,
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    /// Parsed-ref-aware alias annotations that preserve original `ParsedTypeRef` for
    /// re-resolution. Used when an alias target is a fixed collection whose capacity
    /// must be folded through the alias declaration's source visibility when possible.
    pub resolved_type_alias_annotations:
        Option<&'a FxHashMap<InternedPath, ResolvedTypeAnnotation>>,
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
    pub trait_environment: Option<&'a TraitEnvironment>,
    pub trait_evidence_environment: Option<&'a TraitEvidenceEnvironment>,
    pub visible_trait_names: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub source_file_scope: Option<&'a InternedPath>,
}

pub(crate) struct TypeResolutionContextInputs<'a> {
    pub declaration_table: &'a Rc<TopLevelDeclarationTable>,
    pub visible_declaration_ids: Option<&'a FxHashSet<InternedPath>>,
    pub visible_external_symbols: Option<&'a FxHashMap<StringId, ExternalSymbolId>>,
    pub visible_source_bindings: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub visible_type_aliases: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub resolved_type_aliases: Option<&'a FxHashMap<InternedPath, DataType>>,
    pub resolved_type_alias_annotations:
        Option<&'a FxHashMap<InternedPath, ResolvedTypeAnnotation>>,
    pub generic_declarations_by_path:
        Option<&'a FxHashMap<InternedPath, GenericDeclarationMetadata>>,
    pub resolved_struct_fields_by_path: Option<&'a FxHashMap<InternedPath, Vec<Declaration>>>,
    pub type_environment: &'a mut TypeEnvironment,
    /// Visible namespace records for resolving namespace-qualified type names.
    pub visible_namespace_records: Option<&'a FxHashMap<StringId, NamespaceRecord>>,
    pub trait_environment: Option<&'a TraitEnvironment>,
    pub trait_evidence_environment: Option<&'a TraitEvidenceEnvironment>,
    pub visible_trait_names: Option<&'a FxHashMap<StringId, InternedPath>>,
    pub source_file_scope: Option<&'a InternedPath>,
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
            resolved_type_alias_annotations: None,
            generic_declarations_by_path: None,
            generic_parameters: None,
            generic_substitutions: None,
            resolved_struct_fields_by_path: None,
            type_environment,
            visible_namespace_records: None,
            trait_environment: None,
            trait_evidence_environment: None,
            visible_trait_names: None,
            source_file_scope: None,
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
            resolved_type_alias_annotations: inputs.resolved_type_alias_annotations,
            generic_declarations_by_path: inputs.generic_declarations_by_path,
            generic_parameters: None,
            generic_substitutions: None,
            resolved_struct_fields_by_path: inputs.resolved_struct_fields_by_path,
            type_environment: inputs.type_environment,
            visible_namespace_records: inputs.visible_namespace_records,
            trait_environment: inputs.trait_environment,
            trait_evidence_environment: inputs.trait_evidence_environment,
            visible_trait_names: inputs.visible_trait_names,
            source_file_scope: inputs.source_file_scope,
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

/// Fold a parsed collection capacity expression into a canonical `usize`.
///
/// WHAT: parses capacity tokens as an `Int` expression, substitutes visible compile-time
///       constants, and folds to a single integer value.
/// WHY: collection type identity requires a compile-time-known capacity; this helper
///      reuses the existing expression parser and constant folder instead of inventing
///      a parallel evaluator.
pub(crate) fn fold_collection_capacity_expression(
    capacity: &ParsedCollectionCapacity,
    scope_context: Option<&ScopeContext>,
    type_environment: &mut TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<usize, CompilerDiagnostic> {
    // Fast path: a single integer literal needs no expression parsing.
    if let [
        Token {
            kind: TokenKind::IntLiteral(value),
            ..
        },
    ] = capacity.tokens.as_slice()
    {
        if *value < 0 {
            return Err(CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::NegativeCapacity,
                capacity.location.clone(),
            ));
        }
        if *value == 0 {
            return Err(CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::ZeroCapacity,
                capacity.location.clone(),
            ));
        }
        let Ok(capacity_value) = usize::try_from(*value) else {
            return Err(CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::CapacityOverflow,
                capacity.location.clone(),
            ));
        };
        return Ok(capacity_value);
    }

    let Some(scope_context) = scope_context else {
        return Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::CapacityNotConstant,
            capacity.location.clone(),
        ));
    };

    // Parse the capacity token slice as an `Int` expression.
    let mut capacity_tokens = capacity.tokens.clone();
    capacity_tokens.push(Token::new(TokenKind::Eof, capacity.location.clone()));
    let mut token_stream = FileTokens::new(capacity.location.scope.clone(), capacity_tokens);

    let mut expected_type = ExpectedType::Known(type_environment.builtins().int);
    let capacity_context =
        scope_context.new_child_expression(vec![type_environment.builtins().int]);
    let mut compatibility_cache = TypeCompatibilityCache::new();
    let mut type_interner = AstTypeInterner::new(type_environment, &mut compatibility_cache);

    let expression = create_expression_until(
        &mut token_stream,
        &capacity_context,
        &mut type_interner,
        &mut expected_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Eof],
        string_table,
    )
    .map_err(|error| capacity_expression_parse_error(capacity, error))?;

    // Build a const-value environment from visible constant declarations.
    let mut const_env = ConstValueEnvironment::default();
    for declaration in scope_context.top_level_declarations.iter() {
        if declaration.value.is_compile_time_constant() {
            const_env.insert(declaration.id.clone(), declaration.value.clone());
        }
    }
    for declaration in &scope_context.local_declarations {
        if declaration.value.is_compile_time_constant() {
            const_env.insert(declaration.id.clone(), declaration.value.clone());
        }
    }

    let mut resolver = ConstValueResolver::new(string_table);
    let resolved = resolver
        .resolve_expression(&expression, &const_env)
        .map_err(|err| {
            let reason = match err {
                ConstResolutionError::CallInConstContext => {
                    InvalidCollectionTypeReason::CapacityNotConstant
                }
                _ => InvalidCollectionTypeReason::CapacityNotConstant,
            };
            CompilerDiagnostic::invalid_collection_type(reason, capacity.location.clone())
        })?;

    match resolved.kind {
        ExpressionKind::Int(value) => {
            if value < 0 {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::NegativeCapacity,
                    capacity.location.clone(),
                ));
            }
            if value == 0 {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::ZeroCapacity,
                    capacity.location.clone(),
                ));
            }
            let Ok(capacity_value) = usize::try_from(value) else {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::CapacityOverflow,
                    capacity.location.clone(),
                ));
            };
            Ok(capacity_value)
        }
        ExpressionKind::Float(_) => Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::CapacityNotInt,
            capacity.location.clone(),
        )),
        _ => Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::CapacityNotConstant,
            capacity.location.clone(),
        )),
    }
}

fn capacity_expression_parse_error(
    capacity: &ParsedCollectionCapacity,
    error: ExpressionParseError,
) -> CompilerDiagnostic {
    let diagnostic = CompilerDiagnostic::from(error);
    match diagnostic.payload {
        crate::compiler_frontend::compiler_messages::DiagnosticPayload::TypeMismatch { .. } => {
            CompilerDiagnostic::invalid_collection_type(
                InvalidCollectionTypeReason::CapacityNotInt,
                capacity.location.clone(),
            )
        }
        _ => diagnostic,
    }
}

/// Resolve a parsed type annotation through the parsed-ref-aware path.
///
/// WHAT: folds fixed-collection capacity expressions, re-resolves type aliases from their
///       stored `ParsedTypeRef`, and produces canonical `TypeId` identity.
/// WHY: this is the semantic entry point for all type annotations that start as
///      `ParsedTypeRef`; it must not hide capacity folding inside `parsed_ref_to_data_type`.
pub(crate) fn resolve_parsed_type_annotation(
    source_ref: ParsedTypeRef,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
    scope_context: Option<&ScopeContext>,
) -> TypeResolutionResult<ResolvedTypeAnnotation> {
    match &source_ref {
        ParsedTypeRef::Collection {
            element,
            fixed_capacity,
            location: collection_location,
        } => {
            let element_annotation = resolve_parsed_type_annotation(
                *element.clone(),
                collection_location,
                context,
                string_table,
                scope_context,
            )?;
            let element_id = element_annotation.type_id.unwrap_or(builtin_type_ids::NONE);

            let folded_capacity = match fixed_capacity {
                Some(capacity) => {
                    match fold_collection_capacity_expression(
                        capacity,
                        scope_context,
                        context.type_environment,
                        string_table,
                    ) {
                        Ok(value) => Some(value),
                        Err(diagnostic) => {
                            // Only fallback to the diagnostic path when the expression is
                            // genuinely non-constant because no scope context is available.
                            // Literal invalid values (zero, overflow) must still be rejected.
                            let is_non_constant_because_no_scope = scope_context.is_none()
                                && matches!(
                                    &diagnostic.payload,
                                    crate::compiler_frontend::compiler_messages::DiagnosticPayload::InvalidCollectionType {
                                        reason: InvalidCollectionTypeReason::CapacityNotConstant,
                                        ..
                                    }
                                );
                            if is_non_constant_because_no_scope {
                                return fallback_parsed_ref_to_data_type(
                                    source_ref,
                                    location,
                                    context,
                                    string_table,
                                );
                            }
                            return Err(Box::new(diagnostic));
                        }
                    }
                }
                None => None,
            };

            let type_id = context
                .type_environment
                .intern_collection(element_id, folded_capacity);
            let diagnostic_type = DataType::GenericInstance {
                base: GenericBaseType::Builtin(BuiltinGenericType::Collection {
                    fixed_capacity: folded_capacity,
                }),
                arguments: vec![element_annotation.diagnostic_type],
            };
            return Ok(ResolvedTypeAnnotation {
                source_ref,
                diagnostic_type,
                type_id: Some(type_id),
            });
        }

        ParsedTypeRef::Named { name, .. } => {
            if let Some((alias_path, annotation)) = visible_type_alias_annotation(*name, context) {
                return resolve_alias_annotation(
                    alias_path,
                    annotation,
                    location,
                    context,
                    string_table,
                    scope_context,
                );
            }
        }

        ParsedTypeRef::Namespaced {
            namespace, name, ..
        } => {
            if let Some((alias_path, annotation)) =
                visible_namespaced_type_alias_annotation(*namespace, *name, context)
            {
                return resolve_alias_annotation(
                    alias_path,
                    annotation,
                    location,
                    context,
                    string_table,
                    scope_context,
                );
            }
        }

        _ => {}
    }

    fallback_parsed_ref_to_data_type(source_ref, location, context, string_table)
}

fn visible_type_alias_annotation(
    name: StringId,
    context: &TypeResolutionContext<'_>,
) -> Option<(InternedPath, ResolvedTypeAnnotation)> {
    let alias_path = context.visible_type_aliases?.get(&name)?;
    let annotation = context
        .resolved_type_alias_annotations?
        .get(alias_path)?
        .clone();

    Some((alias_path.clone(), annotation))
}

fn visible_namespaced_type_alias_annotation(
    namespace: StringId,
    name: StringId,
    context: &TypeResolutionContext<'_>,
) -> Option<(InternedPath, ResolvedTypeAnnotation)> {
    let alias_path = context
        .visible_namespace_records?
        .get(&namespace)
        .and_then(|record| match record.type_members.get(&name) {
            Some(NamespaceTypeMember::SourceDeclaration(path)) => Some(path),
            _ => None,
        })?;
    let annotation = context
        .resolved_type_alias_annotations?
        .get(alias_path)?
        .clone();

    Some((alias_path.clone(), annotation))
}

fn resolve_alias_annotation(
    alias_path: InternedPath,
    annotation: ResolvedTypeAnnotation,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
    scope_context: Option<&ScopeContext>,
) -> TypeResolutionResult<ResolvedTypeAnnotation> {
    if !parsed_ref_contains_fixed_capacity(&annotation.source_ref) {
        let diagnostic_type =
            resolve_type(&annotation.diagnostic_type, location, context, string_table)?;
        let type_id = if matches!(diagnostic_type, DataType::Inferred) {
            None
        } else {
            Some(resolve_diagnostic_type_to_type_id_checked(
                &diagnostic_type,
                context.type_environment,
                location,
            )?)
        };

        return Ok(ResolvedTypeAnnotation {
            source_ref: annotation.source_ref,
            diagnostic_type,
            type_id,
        });
    }

    let Some(alias_scope_context) = alias_scope_context(&alias_path, scope_context) else {
        return resolve_parsed_type_annotation(
            annotation.source_ref,
            location,
            context,
            string_table,
            scope_context,
        );
    };

    let mut alias_context = TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
        declaration_table: context.declaration_table,
        visible_declaration_ids: alias_scope_context.visible_declaration_ids.as_ref(),
        visible_external_symbols: alias_scope_context
            .file_visibility
            .as_ref()
            .map(|visibility| &visibility.visible_external_symbols),
        visible_source_bindings: alias_scope_context
            .file_visibility
            .as_ref()
            .map(|visibility| &visibility.visible_source_names),
        visible_type_aliases: alias_scope_context
            .file_visibility
            .as_ref()
            .map(|visibility| &visibility.visible_type_alias_names),
        resolved_type_aliases: context.resolved_type_aliases,
        resolved_type_alias_annotations: context.resolved_type_alias_annotations,
        generic_declarations_by_path: context.generic_declarations_by_path,
        resolved_struct_fields_by_path: context.resolved_struct_fields_by_path,
        type_environment: context.type_environment,
        visible_namespace_records: alias_scope_context
            .file_visibility
            .as_ref()
            .map(|visibility| &visibility.visible_namespace_records),
        trait_environment: context.trait_environment,
        trait_evidence_environment: context.trait_evidence_environment,
        visible_trait_names: alias_scope_context
            .file_visibility
            .as_ref()
            .map(|visibility| &visibility.visible_trait_names),
        source_file_scope: alias_scope_context.source_file_scope.as_ref(),
    });

    resolve_parsed_type_annotation(
        annotation.source_ref,
        location,
        &mut alias_context,
        string_table,
        Some(&alias_scope_context),
    )
}

fn parsed_ref_contains_fixed_capacity(source_ref: &ParsedTypeRef) -> bool {
    match source_ref {
        ParsedTypeRef::Collection {
            element,
            fixed_capacity,
            ..
        } => fixed_capacity.is_some() || parsed_ref_contains_fixed_capacity(element),
        ParsedTypeRef::Applied {
            base, arguments, ..
        } => {
            parsed_ref_contains_fixed_capacity(base)
                || arguments.iter().any(parsed_ref_contains_fixed_capacity)
        }
        ParsedTypeRef::Optional { inner, .. } => parsed_ref_contains_fixed_capacity(inner),
        ParsedTypeRef::Result { ok, err, .. } => {
            parsed_ref_contains_fixed_capacity(ok) || parsed_ref_contains_fixed_capacity(err)
        }
        _ => false,
    }
}

fn alias_scope_context(
    alias_path: &InternedPath,
    scope_context: Option<&ScopeContext>,
) -> Option<ScopeContext> {
    let scope_context = scope_context?;
    let source_file = scope_context
        .shared
        .lookups
        .module_symbols
        .canonical_source_by_symbol_path
        .get(alias_path)?;
    let visibility = scope_context
        .shared
        .lookups
        .import_environment
        .visibility_for(source_file)
        .ok()?
        .clone();
    let mut alias_scope = scope_context
        .clone()
        .with_file_visibility(Rc::new(visibility))
        .with_source_file_scope(source_file.clone());

    // Type aliases are top-level metadata. Re-resolving an alias target must not see
    // body-local declarations from the use site.
    alias_scope.set_local_declarations(Vec::new());

    Some(alias_scope)
}

/// Fallback path for parsed type annotations that do not need custom handling.
///
/// WHAT: converts `ParsedTypeRef` to diagnostic `DataType` and resolves it through
///       `resolve_type`, preserving the existing behavior for non-collection types.
fn fallback_parsed_ref_to_data_type(
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
            GenericBaseType::Builtin(BuiltinGenericType::Collection { fixed_capacity }) => {
                let element_id =
                    resolve_diagnostic_type_to_type_id(&arguments[0], type_environment);
                type_environment.intern_collection(element_id, *fixed_capacity)
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
        DataType::DynamicTrait { type_id, .. } => *type_id,
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
            GenericBaseType::Builtin(BuiltinGenericType::Collection { fixed_capacity }) => {
                let element_id =
                    resolve_diagnostic_type_to_type_id_opt(&arguments[0], type_environment)?;
                Some(type_environment.intern_collection(element_id, *fixed_capacity))
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
        DataType::DynamicTrait { type_id, .. } => Some(*type_id),
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
            validate_nominal_bound_evidence_for_instantiation(type_id, location, context)?;

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
            validate_nominal_bound_evidence_for_instantiation(type_id, location, context)?;

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

fn validate_nominal_bound_evidence_for_instantiation(
    type_id: TypeId,
    location: &SourceLocation,
    context: &TypeResolutionContext<'_>,
) -> TypeResolutionResult<()> {
    let evidence_context = GenericBoundEvidenceContext {
        type_environment: context.type_environment,
        trait_environment: context.trait_environment,
        trait_evidence_environment: context.trait_evidence_environment,
        visible_trait_names: context.visible_trait_names,
        source_file_scope: context.source_file_scope,
    };

    validate_nominal_generic_bound_evidence(type_id, location.clone(), &evidence_context)
        .map_err(Box::new)
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

    // 5) Visible trait names in normal type positions are dynamic trait value types.
    if let Some(dynamic_trait_type) =
        resolve_dynamic_trait_type(type_name, location, context, string_table)?
    {
        return Ok(dynamic_trait_type);
    }

    // 6) Builtin type names that may still appear as named placeholders.
    if let Some(builtin_type) = builtin_named_type(type_name, string_table) {
        return Ok(builtin_type);
    }

    Err(Box::new(CompilerDiagnostic::unknown_type_name(
        type_name,
        location.to_owned(),
    )))
}

fn resolve_dynamic_trait_type(
    type_name: StringId,
    location: &SourceLocation,
    context: &mut TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> TypeResolutionResult<Option<DataType>> {
    let Some(trait_environment) = context.trait_environment else {
        return Ok(None);
    };

    let Some(trait_id) = visible_dynamic_trait_id(type_name, context, string_table) else {
        return Ok(None);
    };
    let Some(trait_definition) = trait_environment.get(trait_id) else {
        return Ok(None);
    };

    match &trait_definition.dynamic_safety {
        TraitDynamicSafety::DynamicSafe => {
            let type_id = context
                .type_environment
                .intern_dynamic_trait(trait_id, trait_definition.name);
            Ok(Some(DataType::DynamicTrait {
                trait_id,
                type_id,
                name: trait_definition.name,
            }))
        }

        TraitDynamicSafety::BoundOnly {
            reason,
            offending_requirement,
        } => {
            let requirement_name = trait_definition
                .requirements
                .iter()
                .find(|requirement| requirement.id == *offending_requirement)
                .map(|requirement| requirement.name);
            Err(Box::new(CompilerDiagnostic::invalid_dynamic_trait_type(
                trait_definition.name,
                InvalidDynamicTraitTypeReason::BoundOnly {
                    reason: bound_only_reason_for_diagnostic(*reason),
                    requirement_name,
                },
                location.clone(),
            )))
        }
    }
}

fn visible_dynamic_trait_id(
    type_name: StringId,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Option<crate::compiler_frontend::traits::ids::TraitId> {
    let trait_environment = context.trait_environment?;

    let trait_id = context
        .visible_trait_names
        .and_then(|visible_traits| visible_traits.get(&type_name))
        .and_then(|path| trait_environment.id_for_path(path));

    if trait_id.is_some() {
        return trait_id;
    }

    if let Some(id) = trait_environment.displayable_trait_id_for_name(type_name, string_table) {
        return Some(id);
    }

    None
}

fn visible_dynamic_trait_name(
    type_name: StringId,
    context: &TypeResolutionContext<'_>,
    string_table: &StringTable,
) -> Option<StringId> {
    let trait_id = visible_dynamic_trait_id(type_name, context, string_table)?;
    context
        .trait_environment?
        .get(trait_id)
        .map(|definition| definition.name)
}

fn bound_only_reason_for_diagnostic(
    reason: BoundOnlyTraitReason,
) -> BoundOnlyTraitDiagnosticReason {
    match reason {
        BoundOnlyTraitReason::ThisParameter => BoundOnlyTraitDiagnosticReason::ThisParameter,
        BoundOnlyTraitReason::ThisReturn => BoundOnlyTraitDiagnosticReason::ThisReturn,
    }
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

            if let Some(trait_name) = visible_dynamic_trait_name(*type_name, context, string_table)
            {
                return Err(Box::new(CompilerDiagnostic::invalid_dynamic_trait_type(
                    trait_name,
                    InvalidDynamicTraitTypeReason::Applied,
                    location.clone(),
                )));
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
        GenericBaseType::Builtin(BuiltinGenericType::Collection { fixed_capacity }) => {
            // Collection is the only builtin generic type allowed in source.
            // Its arguments are resolved separately by resolve_type.
            Ok(GenericBaseType::Builtin(BuiltinGenericType::Collection {
                fixed_capacity: *fixed_capacity,
            }))
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
