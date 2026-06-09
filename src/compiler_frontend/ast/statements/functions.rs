//! Function-signature parsing and function-call AST helpers.
//!
//! WHAT: parses function signatures, return lists, and host/user call metadata used by AST construction.
//! WHY: function syntax has enough dedicated parsing and type-shape rules to live outside the general statement parser.

#![allow(clippy::result_large_err)]
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    ExpressionTrailingPolicy, create_expression_with_trailing_newline_policy,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::type_resolution::{
    ResolvedTypeAnnotation, TypeResolutionContext, TypeResolutionContextInputs,
    resolve_diagnostic_type_to_type_id, resolve_parsed_type_annotation,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticPayload, InvalidCollectionTypeReason, NameNamespace,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    FunctionReturnSyntax, FunctionSignatureSyntax, ReturnChannelSyntax, ReturnSlotSyntax,
    SignatureMemberSyntax, alias_return_type_mismatch_diagnostic, parse_function_signature_syntax,
};
use crate::compiler_frontend::declaration_syntax::type_syntax::parsed_ref_to_data_type;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_type_id;

/// One function return slot, either a concrete value type or a parameter-alias set.
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionReturn {
    Value(DataType),
    AliasCandidates {
        parameter_indices: Vec<usize>,
        data_type: DataType,
    },
}

impl FunctionReturn {
    pub fn data_type(&self) -> &DataType {
        match self {
            FunctionReturn::Value(data_type) => data_type,
            FunctionReturn::AliasCandidates { data_type, .. } => data_type,
        }
    }

    pub fn alias_candidates(&self) -> Option<&[usize]> {
        match self {
            FunctionReturn::Value(_) => None,
            FunctionReturn::AliasCandidates {
                parameter_indices, ..
            } => Some(parameter_indices.as_slice()),
        }
    }
}

/// Whether a return slot carries success-channel or error-channel values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnChannel {
    Success,
    Error,
}

/// Parsed return slot with a value shape, optional resolved type, and channel discriminator.
#[derive(Clone, Debug, PartialEq)]
pub struct ReturnSlot {
    pub value: FunctionReturn,
    /// Canonical `TypeId` for this return slot, populated during type resolution.
    /// `None` before resolution; `Some` afterwards.
    pub type_id: Option<TypeId>,
    pub channel: ReturnChannel,
}

impl ReturnSlot {
    pub fn success(value: FunctionReturn) -> Self {
        Self {
            value,
            type_id: None,
            channel: ReturnChannel::Success,
        }
    }

    pub fn error(value: FunctionReturn) -> Self {
        Self {
            value,
            type_id: None,
            channel: ReturnChannel::Error,
        }
    }

    pub fn data_type(&self) -> &DataType {
        self.value.data_type()
    }
}

impl PartialEq<FunctionReturn> for ReturnSlot {
    fn eq(&self, other: &FunctionReturn) -> bool {
        self.channel == ReturnChannel::Success && &self.value == other
    }
}

impl PartialEq<ReturnSlot> for FunctionReturn {
    fn eq(&self, other: &ReturnSlot) -> bool {
        other == self
    }
}

/// Parsed function signature with parameter declarations and return slots.
#[derive(Clone, Debug, Default)]
pub struct FunctionSignature {
    pub parameters: Vec<Declaration>,
    pub returns: Vec<ReturnSlot>,
}

#[derive(Clone, Copy)]
pub(crate) enum SignatureTypeFallbackPolicy {
    StrictCapacity,
    AllowUnresolvedCapacity,
}

impl FunctionSignature {
    pub(crate) fn new(
        token_stream: &mut FileTokens,
        warnings: &mut Vec<CompilerDiagnostic>,
        string_table: &mut StringTable,
        function_path: &InternedPath,
        parent_context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
    ) -> Result<Self, CompilerDiagnostic> {
        let signature_syntax =
            parse_function_signature_syntax(token_stream, warnings, string_table, function_path)?;

        let signature_context =
            ScopeContext::new_constant(function_path.to_owned(), parent_context);

        function_signature_from_syntax_with_unresolved_types(
            &signature_syntax,
            &signature_context,
            type_interner,
            string_table,
            SignatureTypeFallbackPolicy::StrictCapacity,
        )
    }

    /// Success-channel return types for diagnostics only.
    pub fn diagnostic_return_types(&self) -> Vec<DataType> {
        self.success_returns()
            .iter()
            .map(|return_value| return_value.data_type().clone())
            .collect()
    }

    pub fn success_returns(&self) -> Vec<&FunctionReturn> {
        self.returns
            .iter()
            .filter(|slot| slot.channel == ReturnChannel::Success)
            .map(|slot| &slot.value)
            .collect()
    }

    pub fn error_return(&self) -> Option<&FunctionReturn> {
        self.returns
            .iter()
            .find(|slot| slot.channel == ReturnChannel::Error)
            .map(|slot| &slot.value)
    }

    pub fn error_return_index(&self) -> Option<usize> {
        self.returns
            .iter()
            .position(|slot| slot.channel == ReturnChannel::Error)
    }

    pub fn has_error_slot(&self) -> bool {
        self.error_return().is_some()
    }

    /// Canonical TypeIds for success-channel return slots.
    /// Only meaningful after type resolution.
    pub fn success_return_type_ids(&self) -> Vec<TypeId> {
        self.returns
            .iter()
            .filter(|slot| slot.channel == ReturnChannel::Success)
            .filter_map(|slot| slot.type_id)
            .collect()
    }

    /// Canonical TypeId for the error return slot, if any.
    /// Only meaningful after type resolution.
    pub fn error_return_type_id(&self) -> Option<TypeId> {
        self.returns
            .iter()
            .find(|slot| slot.channel == ReturnChannel::Error)
            .and_then(|slot| slot.type_id)
    }

    /// Remap all interned string IDs in parameters and return slots.
    ///
    /// WHAT: updates parameter declaration names/expressions and return-slot diagnostic types.
    /// WHY: per-file header parsing produces function signatures using local string tables;
    ///      remapping keeps them valid after merge into the module/global table.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for parameter in &mut self.parameters {
            parameter.remap_string_ids(remap);
        }
        for return_slot in &mut self.returns {
            return_slot.remap_string_ids(remap);
        }
    }
}

impl FunctionReturn {
    /// Remap interned string IDs in the diagnostic type spelling.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            FunctionReturn::Value(data_type) => {
                data_type.remap_string_ids(remap);
            }

            FunctionReturn::AliasCandidates { data_type, .. } => {
                data_type.remap_string_ids(remap);
            }
        }
    }
}

impl ReturnSlot {
    /// Remap interned string IDs in the return slot's diagnostic type.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.value.remap_string_ids(remap);
        // type_id is a canonical semantic ID and must not be remapped.
        // channel is an enum discriminator with no string data.
    }
}

/// Build a `FunctionSignature` from raw syntax, leaving parameter types unresolved.
///
/// WHAT: converts parsed signature members and return slots into AST declarations and return metadata.
/// WHY: signature syntax is resolved before body parsing so arity and channel information is available
///      to callers without re-parsing the token stream.
pub(crate) fn function_signature_from_syntax_with_unresolved_types(
    syntax: &FunctionSignatureSyntax,
    expression_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    fallback_policy: SignatureTypeFallbackPolicy,
) -> Result<FunctionSignature, CompilerDiagnostic> {
    let mut parameters = Vec::with_capacity(syntax.parameters.len());
    for parameter in &syntax.parameters {
        parameters.push(signature_member_to_declaration(
            parameter,
            expression_context,
            type_interner,
            string_table,
            fallback_policy,
        )?);
    }

    let mut returns = Vec::with_capacity(syntax.returns.len());
    for return_slot in &syntax.returns {
        returns.push(return_slot_from_syntax(
            return_slot,
            &parameters,
            expression_context,
            type_interner,
            string_table,
            fallback_policy,
        )?);
    }

    Ok(FunctionSignature {
        parameters,
        returns,
    })
}

pub(crate) fn signature_member_to_declaration(
    member: &SignatureMemberSyntax,
    expression_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    fallback_policy: SignatureTypeFallbackPolicy,
) -> Result<Declaration, CompilerDiagnostic> {
    let resolved = resolve_signature_type_annotation(
        member.type_annotation.clone(),
        &member.location,
        expression_context,
        type_interner,
        string_table,
    );

    let (type_id, data_type) = match resolved {
        Ok(annotation) => (
            annotation.type_id.unwrap_or(builtin_type_ids::NONE),
            annotation.diagnostic_type,
        ),
        Err(diagnostic) if should_fallback_signature_type(&diagnostic, fallback_policy) => {
            // Signature parsing may encounter generic parameters that are not yet
            // resolvable in the current context. Early nominal-member shell parsing
            // may also see capacity constants before constants have been folded.
            // Keep that fallback narrow so literal invalid capacities are still
            // reported instead of being erased to growable collection types.
            let data_type = parsed_ref_to_data_type(&member.type_annotation);
            let type_id = resolve_diagnostic_type_to_type_id(
                &data_type,
                type_interner.environment_mut_for_derived_types(),
            );
            (type_id, data_type)
        }
        Err(diagnostic) => return Err(*diagnostic),
    };

    let value = if member.default_tokens.is_empty() {
        Expression::new(
            ExpressionKind::NoValue,
            member.location.clone(),
            type_id,
            data_type,
            member.value_mode.clone(),
        )
    } else {
        parse_signature_default_expression(
            member,
            type_id,
            expression_context,
            type_interner,
            string_table,
        )?
    };

    Ok(Declaration {
        id: member.id.clone(),
        value,
    })
}

/// Resolve a parsed type annotation inside a function-style signature.
///
/// WHAT: builds the AST type-resolution context from the active `ScopeContext`.
/// WHY: parameters, struct/choice member shells, and explicit return slots all share
///      the same fixed-capacity folding rules and alias visibility model.
fn resolve_signature_type_annotation(
    type_annotation: ParsedTypeRef,
    location: &SourceLocation,
    expression_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<ResolvedTypeAnnotation, Box<CompilerDiagnostic>> {
    let mut type_resolution_context =
        TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
            declaration_table: &expression_context.top_level_declarations,
            visible_declaration_ids: expression_context.visible_declaration_ids.as_ref(),
            visible_external_symbols: expression_context
                .file_visibility
                .as_ref()
                .map(|fv| &fv.visible_external_symbols),
            visible_source_bindings: expression_context
                .file_visibility
                .as_ref()
                .map(|fv| &fv.visible_source_names),
            visible_type_aliases: expression_context
                .file_visibility
                .as_ref()
                .map(|fv| &fv.visible_type_alias_names),
            resolved_type_aliases: expression_context.resolved_type_aliases.as_deref(),
            resolved_type_alias_annotations: expression_context
                .resolved_type_alias_annotations
                .as_deref(),
            generic_declarations_by_path: expression_context
                .generic_declarations_by_path
                .as_deref(),
            resolved_struct_fields_by_path: expression_context
                .resolved_struct_fields_by_path
                .as_deref(),
            type_environment: type_interner.environment_mut_for_derived_types(),
            visible_namespace_records: expression_context
                .file_visibility
                .as_ref()
                .map(|fv| &fv.visible_namespace_records),
            trait_environment: expression_context.trait_environment_override.as_deref(),
            trait_evidence_environment: None,
            visible_trait_names: expression_context
                .file_visibility
                .as_ref()
                .map(|fv| &fv.visible_trait_names),
            source_file_scope: expression_context.source_file_scope.as_ref(),
        })
        .with_active_generic_type_context(expression_context.active_generic_type_context());

    resolve_parsed_type_annotation(
        type_annotation,
        location,
        &mut type_resolution_context,
        string_table,
        Some(expression_context),
    )
}

fn should_fallback_signature_type(
    diagnostic: &CompilerDiagnostic,
    fallback_policy: SignatureTypeFallbackPolicy,
) -> bool {
    match &diagnostic.payload {
        DiagnosticPayload::UnknownName {
            namespace: NameNamespace::Type,
            ..
        } => true,
        DiagnosticPayload::InvalidCollectionType {
            reason: InvalidCollectionTypeReason::CapacityNotConstant,
            ..
        } => matches!(
            fallback_policy,
            SignatureTypeFallbackPolicy::AllowUnresolvedCapacity
        ),
        _ => false,
    }
}

/// Parse the default-value expression for a single signature parameter.
fn parse_signature_default_expression(
    member: &SignatureMemberSyntax,
    type_id: TypeId,
    expression_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    let mut parameter_context = expression_context.to_owned();
    parameter_context.expected_result_type_ids = vec![type_id];

    let mut expected_type = parse_expectation_for_type_id(type_id, type_interner.environment());
    let mut expression_stream = token_stream_with_eof(&member.default_tokens)?;

    create_expression_with_trailing_newline_policy(
        &mut expression_stream,
        &parameter_context,
        type_interner,
        &mut expected_type,
        &member.value_mode,
        ExpressionTrailingPolicy {
            consume_closing_parenthesis: false,
            skip_trailing_newlines: true,
            allow_boundary_catch: false,
            allow_expected_result_evidence: true,
        },
        string_table,
    )
    .map_err(CompilerDiagnostic::from)
}

/// Wrap a raw token slice in a `FileTokens` stream terminated by EOF.
fn token_stream_with_eof(tokens: &[Token]) -> Result<FileTokens, CompilerDiagnostic> {
    let Some(first_token) = tokens.first() else {
        return Err(CompilerDiagnostic::unexpected_end_of_file(
            None,
            SourceLocation::default(),
        ));
    };

    let mut tokens_with_eof = tokens.to_vec();
    let eof_location = tokens
        .last()
        .map(|token| token.location.clone())
        .unwrap_or_else(|| first_token.location.clone());
    let src_path = first_token.location.scope.clone();

    tokens_with_eof.push(Token::new(TokenKind::Eof, eof_location));

    Ok(FileTokens::new(src_path, tokens_with_eof))
}

/// Build a `ReturnSlot` from parsed syntax, validating alias-candidate type consistency.
fn return_slot_from_syntax(
    return_slot: &ReturnSlotSyntax,
    parameters: &[Declaration],
    expression_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    fallback_policy: SignatureTypeFallbackPolicy,
) -> Result<ReturnSlot, CompilerDiagnostic> {
    let channel = match return_slot.channel {
        ReturnChannelSyntax::Success => ReturnChannel::Success,
        ReturnChannelSyntax::Error => ReturnChannel::Error,
    };

    let value = match &return_slot.value {
        FunctionReturnSyntax::Value {
            type_annotation,
            location,
        } => {
            let resolved = resolve_signature_type_annotation(
                type_annotation.clone(),
                location,
                expression_context,
                type_interner,
                string_table,
            );

            let data_type = match resolved {
                Ok(annotation) => annotation.diagnostic_type,
                Err(diagnostic) if should_fallback_signature_type(&diagnostic, fallback_policy) => {
                    // Generic return types may be resolved later once the function's
                    // declaration-site generic parameter scope is active.
                    parsed_ref_to_data_type(type_annotation)
                }
                Err(diagnostic) => return Err(*diagnostic),
            };

            FunctionReturn::Value(data_type)
        }

        FunctionReturnSyntax::AliasCandidates {
            parameter_indices,
            location,
        } => {
            let first_index = parameter_indices[0];
            let first_parameter = &parameters[first_index];
            let first_type_id = first_parameter.value.type_id;
            let mut data_type = first_parameter.value.diagnostic_type.clone();

            for parameter_index in parameter_indices.iter().copied().skip(1) {
                let parameter = &parameters[parameter_index];
                if parameter.value.type_id != first_type_id {
                    return Err(alias_return_type_mismatch_diagnostic(
                        first_type_id,
                        parameter.value.type_id,
                        location.clone(),
                    ));
                }
                data_type = parameter.value.diagnostic_type.clone();
            }

            FunctionReturn::AliasCandidates {
                parameter_indices: parameter_indices.clone(),
                data_type,
            }
        }
    };

    Ok(ReturnSlot {
        value,
        type_id: None,
        channel,
    })
}

#[cfg(test)]
#[path = "tests/function_parsing_tests.rs"]
mod function_parsing_tests;

#[cfg(test)]
#[path = "tests/signature_remap_tests.rs"]
mod signature_remap_tests;
