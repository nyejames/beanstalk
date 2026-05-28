//! Identifier-led expression parsing helpers.
//!
//! WHAT: parses identifier-led expression forms such as references, constructors, calls, and
//! namespace records.
//! WHY: identifier tokens fan out into the largest number of semantic cases and need isolated handling.

use super::call_argument::normalize_call_arguments;
use super::choice_constructor::parse_choice_construct;
use super::error::ExpressionParseError;
use super::expression::{Expression, ExpressionKind, HandledFallibleHostFunctionCallInput};
use super::function_calls::{ExternalFunctionCallParseInput, parse_external_function_call};
use super::namespace_member_access::{NamespaceMemberAccessInput, parse_namespace_member_access};
use super::parse_expression_dispatch::push_expression_node;
use super::source_function_calls::{SourceCallableMemberInput, parse_source_callable_member};
use super::struct_instance::{StructConstructorParseInput, parse_struct_constructor_expression};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::statements::fallible_handling::fallible_catch_allowed_in_context;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic, InvalidAssignmentTargetReason,
    InvalidTemplateSlotReason, InvalidThisUsageReason, NameNamespace,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::ExternalConstantValue;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

pub(super) fn parse_identifier_or_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expression: &mut Vec<AstNode>,
    allow_boundary_catch: bool,
    string_table: &mut StringTable,
) -> Result<(), ExpressionParseError> {
    // Fast path for reserved receiver keyword `this`.
    if token_stream.current_token_kind() == &TokenKind::This {
        return parse_this_reference(
            token_stream,
            context,
            type_interner,
            expression,
            allow_boundary_catch,
            string_table,
        );
    }

    // One identifier token can expand into several expression forms: a local/reference read,
    // struct construction, source or external function call, or namespace record access.
    let TokenKind::Symbol(identifier) = token_stream.current_token_kind().to_owned() else {
        return Ok(());
    };

    if context.is_assignment_target_unavailable(identifier) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::UnavailableInCatchRecovery,
            Some(identifier),
            None,
            token_stream.current_location(),
        )
        .into());
    }

    // ------------------------------------------------------------
    //  Local binding: reference, constructor, or function call
    // ------------------------------------------------------------
    if let Some(binding) = context.get_reference(&identifier) {
        // Template slot inserts are only legal inside template bodies, constant
        // initializers, or constant headers.
        if let ExpressionKind::Template(template_value) = &binding.value.kind
            && matches!(template_value.kind, TemplateType::SlotInsert(_))
            && !matches!(
                context.kind,
                ContextKind::Template | ContextKind::Constant | ContextKind::ConstantHeader
            )
        {
            return Err(CompilerDiagnostic::invalid_template_slot(
                InvalidTemplateSlotReason::InsertOutsideParentSlot,
                None,
                token_stream.current_location(),
            )
            .into());
        }

        // Const records are field-access-only in runtime contexts.
        if binding.value.is_const_record_value()
            && token_stream.peek_next_token() != Some(&TokenKind::Dot)
            && !context.kind.is_constant_context()
        {
            return Err(CompilerDiagnostic::const_record_used_as_value(
                identifier,
                token_stream.current_location(),
            )
            .into());
        }

        // Struct constructors are parsed before constant-reference checks.
        // This keeps `x #= MyStruct(...)` on the constructor path so const
        // record coercion can validate field values instead of rejecting the
        // struct symbol itself as a non-constant reference.
        if let DataType::Struct {
            nominal_path,
            type_id,
            ..
        } = &binding.value.diagnostic_type
            && token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        {
            let fields = context
                .resolved_struct_fields_by_path
                .as_ref()
                .and_then(|map| map.get(nominal_path))
                .map(|f| f.as_slice())
                .unwrap_or(&[]);
            let struct_instance = parse_struct_constructor_expression(
                token_stream,
                StructConstructorParseInput {
                    struct_path: nominal_path,
                    struct_name: identifier,
                    fields,
                    struct_value_mode: &binding.value.value_mode,
                    type_id: *type_id,
                },
                context,
                type_interner,
                string_table,
            )?;

            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                expression,
                allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(struct_instance),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                },
            )?;

            return Ok(());
        }

        // Choice constructors are routed through their own parser.
        if token_stream.peek_next_token() == Some(&TokenKind::DoubleColon) {
            if matches!(&binding.value.diagnostic_type, DataType::Choices { .. }) {
                let choice_value = parse_choice_construct(
                    token_stream,
                    binding,
                    context,
                    type_interner,
                    string_table,
                )?;
                let choice_location = choice_value.location.to_owned();

                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    AstNode {
                        kind: NodeKind::Rvalue(choice_value),
                        location: choice_location,
                        scope: context.scope.clone(),
                    },
                )?;

                return Ok(());
            }

            return Err(CompilerDiagnostic::namespace_misuse(
                identifier,
                NameNamespace::Type,
                NameNamespace::Value,
                token_stream.current_location(),
            )
            .into());
        }

        // Constant contexts reject non-constant local references.
        if context.kind.is_constant_context()
            && !binding.value.is_compile_time_constant()
            && !binding.is_unresolved_constant_placeholder()
        {
            return Err(CompilerDiagnostic::compile_time_evaluation_error(
                CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant,
                Some(identifier),
                token_stream.current_location(),
            )
            .into());
        }

        match &binding.value.diagnostic_type {
            DataType::Function(_, signature) => {
                let generic_template = context.lookup_generic_function_template(&binding.id);
                let call_location = token_stream.current_location();

                parse_source_callable_member(SourceCallableMemberInput {
                    token_stream,
                    function_path: &binding.id,
                    signature,
                    generic_template,
                    visible_name: identifier,
                    call_location,
                    context,
                    expression,
                    allow_boundary_catch,
                    type_interner,
                    string_table,
                })?;

                return Ok(());
            }

            DataType::Struct { .. } => {
                // Fall through to normal reference behaviour for non-constructor uses.
                let reference_node =
                    create_reference(token_stream, binding, context, type_interner, string_table)?;
                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    reference_node,
                )?;
                return Ok(());
            }

            _ => {
                let reference_node =
                    create_reference(token_stream, binding, context, type_interner, string_table)?;
                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    reference_node,
                )?;
                return Ok(()); // Will have moved onto the next token already
            }
        }
    }

    // ------------------------------------
    //  Namespace record access
    // ------------------------------------
    if let Some(record) = context
        .file_visibility
        .as_ref()
        .and_then(|fv| fv.visible_namespace_records.get(&identifier))
    {
        if token_stream.peek_next_token() == Some(&TokenKind::Dot) {
            return parse_namespace_member_access(NamespaceMemberAccessInput {
                token_stream,
                context,
                type_interner,
                expression,
                allow_boundary_catch,
                record_name: identifier,
                record,
                string_table,
            });
        }

        return Err(CompilerDiagnostic::import_record_used_as_value(
            identifier,
            token_stream.current_location(),
        )
        .into());
    }

    // ------------------------------------
    //  External constant
    // ------------------------------------
    if let Some((_const_id, const_def)) = context.lookup_visible_external_constant(identifier) {
        token_stream.advance();
        let location = token_stream.current_location();

        if context.kind.is_constant_context() && !const_def.value.is_scalar() {
            return Err(CompilerDiagnostic::compile_time_evaluation_error(
                CompileTimeEvaluationErrorReason::ExternalNonScalarConstantInConstantContext,
                Some(identifier),
                location,
            )
            .into());
        }

        let value_mode = ValueMode::ImmutableOwned;
        let const_expr = match const_def.value {
            ExternalConstantValue::Float(value) => Expression::float(value, location, value_mode),
            ExternalConstantValue::Int(value) => Expression::int(value, location, value_mode),
            ExternalConstantValue::StringSlice(value) => {
                let string_id = string_table.intern(value);
                Expression::string_slice(string_id, location, value_mode)
            }
            ExternalConstantValue::Bool(value) => Expression::bool(value, location, value_mode),
        };

        push_expression_node(
            token_stream,
            context,
            type_interner,
            string_table,
            expression,
            allow_boundary_catch,
            AstNode {
                kind: NodeKind::Rvalue(const_expr),
                location: SourceLocation::default(),
                scope: context.scope.clone(),
            },
        )?;
        return Ok(());
    }

    // ------------------------------------
    //  Host function call
    // ------------------------------------
    if let Some((function_id, host_function_definition)) =
        context.lookup_visible_external_function(identifier)
    {
        if context.kind.is_constant_context() {
            return Err(CompilerDiagnostic::compile_time_evaluation_error(
                CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext,
                Some(identifier),
                token_stream.current_location(),
            )
            .into());
        }

        // External calls parse from metadata directly; do not synthesize fake parameter declarations.
        token_stream.advance();

        let function_call_node = parse_external_function_call(ExternalFunctionCallParseInput {
            token_stream,
            external_function_id: function_id,
            external_function: host_function_definition,
            context,
            value_required: true,
            allow_boundary_catch: allow_boundary_catch
                && expression.is_empty()
                && fallible_catch_allowed_in_context(context),
            warnings: None,
            type_interner,
            string_table,
        })?;

        match function_call_node.kind {
            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                result_type_ids,
                location,
            } => {
                let normalized_args = normalize_call_arguments(&args);
                let func_call_expr = Expression::host_function_call_with_typed_arguments(
                    host_function_id,
                    normalized_args,
                    result_type_ids,
                    type_interner.environment_mut_for_derived_types(),
                    location,
                );

                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    AstNode {
                        kind: NodeKind::Rvalue(func_call_expr),
                        location: SourceLocation::default(),
                        scope: context.scope.clone(),
                    },
                )?;

                return Ok(());
            }

            NodeKind::HandledFallibleHostFunctionCall {
                name: host_function_id,
                args,
                result_type_ids,
                error_type_id,
                handling,
                location,
            } => {
                let normalized_args = normalize_call_arguments(&args);
                let func_call_expr =
                    Expression::handled_fallible_host_function_call_with_typed_arguments(
                        HandledFallibleHostFunctionCallInput {
                            id: host_function_id,
                            args: normalized_args,
                            result_type_ids,
                            error_type_id,
                            handling,
                            location,
                        },
                        type_interner.environment_mut_for_derived_types(),
                    );

                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    AstNode {
                        kind: NodeKind::Rvalue(func_call_expr),
                        location: SourceLocation::default(),
                        scope: context.scope.clone(),
                    },
                )?;

                return Ok(());
            }

            NodeKind::Rvalue(expression_value) => {
                push_expression_node(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    AstNode {
                        kind: NodeKind::Rvalue(expression_value),
                        location: SourceLocation::default(),
                        scope: context.scope.clone(),
                    },
                )?;

                return Ok(());
            }

            _ => {}
        }
    }

    // Receiver methods cannot be called as free functions.
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        && let Some(method_entry) = context.lookup_visible_receiver_method_by_name(identifier)
    {
        let diagnostic = free_function_receiver_method_call_error(
            identifier,
            method_entry,
            token_stream.current_location(),
            string_table,
        );

        return Err(diagnostic.into());
    }

    // External types cannot be constructed with struct literal syntax.
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
        && context.lookup_visible_external_type(identifier).is_some()
    {
        return Err(CompilerDiagnostic::compile_time_evaluation_error(
            CompileTimeEvaluationErrorReason::ExternalTypeConstructionNotSupported,
            Some(identifier),
            token_stream.current_location(),
        )
        .into());
    }

    // Type aliases are type-namespace only.
    if context.is_visible_type_alias_name(identifier) {
        return Err(CompilerDiagnostic::namespace_misuse(
            identifier,
            NameNamespace::Value,
            NameNamespace::Type,
            token_stream.current_location(),
        )
        .into());
    }

    Err(CompilerDiagnostic::unknown_value_name(identifier, token_stream.current_location()).into())
}

/// Parse a `this` reference inside a receiver method body.
///
/// WHAT: validates that `this` is in scope (i.e. the current function declared a receiver)
/// and emits a reference node identical to a normal local read.
/// WHY: `this` is a reserved keyword token, not an ordinary identifier, so it needs its own
/// parse path, but semantically it behaves like any other parameter reference.
fn parse_this_reference(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expression: &mut Vec<AstNode>,
    allow_boundary_catch: bool,
    string_table: &mut StringTable,
) -> Result<(), ExpressionParseError> {
    let this_id = string_table.intern("this");

    if context.is_assignment_target_unavailable(this_id) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::UnavailableInCatchRecovery,
            Some(this_id),
            None,
            token_stream.current_location(),
        )
        .into());
    }

    let Some(receiver_declaration) = context.get_reference(&this_id) else {
        return Err(CompilerDiagnostic::invalid_this_usage(
            InvalidThisUsageReason::NotInReceiverMethod,
            token_stream.current_location(),
        )
        .into());
    };

    let reference_node = create_reference(
        token_stream,
        receiver_declaration,
        context,
        type_interner,
        string_table,
    )?;

    push_expression_node(
        token_stream,
        context,
        type_interner,
        string_table,
        expression,
        allow_boundary_catch,
        reference_node,
    )?;

    Ok(())
}
