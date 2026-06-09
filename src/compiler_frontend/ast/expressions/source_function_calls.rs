//! Source function and generic free-function call parsing.
//!
//! WHAT: handles calls to source-defined functions, including generic templates,
//! and converts parsed call nodes into expression Rvalues.
//! WHY: identifier and namespace parsing both route source callable members here
//! so generic and non-generic call behavior stays consistent.

use super::call_argument::normalize_call_arguments;
use super::error::ExpressionParseError;
use super::expression::Expression;
use super::function_calls::{FunctionCallParseInput, parse_function_call};
use super::parse_expression_dispatch::push_expression_node;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::generic_functions::{
    GenericCallExpectedContext, GenericFunctionCallParseInput, GenericFunctionTemplate,
    parse_generic_function_call, validate_generic_function_template_call,
};
use crate::compiler_frontend::ast::statements::fallible_handling::fallible_catch_allowed_in_context;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidGenericInstantiationReason,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};

/// Input bundle for source callable member parsing.
///
/// WHAT: carries everything needed to parse a call to a source-defined function,
/// whether generic or non-generic.
/// WHY: avoids threading a long argument list through both bare-identifier and
/// namespace-member call sites.
pub(super) struct SourceCallableMemberInput<'a, 'env> {
    pub(super) token_stream: &'a mut FileTokens,
    pub(super) function_path: &'a InternedPath,
    pub(super) signature: &'a FunctionSignature,
    pub(super) generic_template: Option<&'a GenericFunctionTemplate>,
    pub(super) visible_name: StringId,
    pub(super) call_location: SourceLocation,
    pub(super) context: &'a ScopeContext,
    pub(super) expression: &'a mut Vec<AstNode>,
    pub(super) allow_boundary_catch: bool,
    pub(super) expected_result_evidence_allowed: bool,
    pub(super) type_interner: &'a mut AstTypeInterner<'env>,
    pub(super) string_table: &'a mut StringTable,
}

/// Parse a call to a source-defined function (generic or non-generic) and push
/// the resulting expression node onto the expression buffer.
///
/// WHAT: resolves generic vs non-generic dispatch, enforces the "generic function
/// values are deferred" rule, and converts the returned AST call node into an
/// expression Rvalue.
/// WHY: both bare identifier and namespace member access share this path, so
/// keeping it in one place guarantees consistent behavior.
pub(super) fn parse_source_callable_member(
    input: SourceCallableMemberInput<'_, '_>,
) -> Result<(), ExpressionParseError> {
    let SourceCallableMemberInput {
        token_stream,
        function_path,
        signature,
        generic_template,
        visible_name,
        call_location,
        context,
        expression,
        allow_boundary_catch,
        expected_result_evidence_allowed,
        type_interner,
        string_table,
    } = input;

    let expression_is_boundary_leading = expression.is_empty();
    let allow_call_boundary_catch = allow_boundary_catch
        && expression_is_boundary_leading
        && fallible_catch_allowed_in_context(context);

    // ------------------------
    //  Generic source call
    // ------------------------
    if let Some(template) = generic_template {
        match token_stream.peek_next_token() {
            // Explicit call-site type arguments are not part of the Alpha surface.
            // Reject the known foreign spellings before they can be interpreted as
            // generic function values, comparisons, or templates.
            Some(TokenKind::Of | TokenKind::LessThan | TokenKind::TemplateHead) => {
                let explicit_syntax_location = token_stream
                    .tokens
                    .get(token_stream.index + 1)
                    .map(|token| token.location.clone())
                    .unwrap_or_else(|| call_location.clone());

                return Err(explicit_generic_call_type_arguments_error(
                    visible_name,
                    explicit_syntax_location,
                )
                .into());
            }

            // Generic functions must be called; using them as first-class values is
            // deferred for Alpha. Require an immediate `(` to route into the call parser.
            Some(TokenKind::OpenParenthesis) => {}

            _ => {
                return Err(CompilerDiagnostic::invalid_generic_instantiation(
                    Some(visible_name),
                    InvalidGenericInstantiationReason::GenericFunctionValueDeferred,
                    call_location,
                )
                .into());
            }
        }

        // Move from the visible generic function name to the `(` consumed by the shared call parser.
        token_stream.advance();

        let expected_context = if expected_result_evidence_allowed
            && expression_is_boundary_leading
            && !context.expected_result_type_ids.is_empty()
        {
            GenericCallExpectedContext::ImmediateResult(context.expected_result_type_ids.as_slice())
        } else {
            GenericCallExpectedContext::None
        };

        let generic_call_input = GenericFunctionCallParseInput {
            token_stream,
            template,
            context,
            expected_context,
            value_required: true,
            allow_boundary_catch: allow_call_boundary_catch,
            call_location,
            warnings: None,
            type_interner,
            string_table,
        };

        let function_call_node = if context.generic_template_validation {
            validate_generic_function_template_call(generic_call_input)
        } else {
            parse_generic_function_call(generic_call_input)
        }?;

        push_call_expression_node(
            function_call_node,
            token_stream,
            context,
            type_interner,
            string_table,
            expression,
            allow_boundary_catch,
        )?;

        return Ok(());
    }

    // ------------------------
    //  Non-generic source call
    // ------------------------
    token_stream.advance();

    let function_call_node = parse_function_call(FunctionCallParseInput {
        token_stream,
        id: function_path,
        context,
        signature,
        value_required: true,
        allow_boundary_catch: allow_call_boundary_catch,
        warnings: None,
        type_interner,
        string_table,
    })?;

    push_call_expression_node(
        function_call_node,
        token_stream,
        context,
        type_interner,
        string_table,
        expression,
        allow_boundary_catch,
    )?;

    Ok(())
}

fn explicit_generic_call_type_arguments_error(
    function_name: StringId,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::invalid_generic_instantiation(
        Some(function_name),
        InvalidGenericInstantiationReason::ExplicitCallTypeArgumentsUnsupported,
        location,
    )
}

/// Convert a `FunctionCall` or `HandledFallibleFunctionCall` AST node into an
/// expression Rvalue and push it onto the expression buffer.
///
/// WHAT: owns the repeated `normalize_call_arguments` + expression construction
/// that appears after both generic and non-generic call parsing.
/// WHY: eliminates duplication between the bare-identifier and namespace-member
/// call paths.
fn push_call_expression_node(
    function_call_node: AstNode,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
    expression: &mut Vec<AstNode>,
    allow_boundary_catch: bool,
) -> Result<(), ExpressionParseError> {
    let function_call_location = function_call_node.location.to_owned();

    match function_call_node.kind {
        NodeKind::FunctionCall {
            name,
            args,
            result_type_ids,
            location,
        } => {
            let normalized_args = normalize_call_arguments(&args);
            let function_call_expression = Expression::function_call_with_typed_arguments(
                name,
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
                    kind: NodeKind::Rvalue(function_call_expression),
                    location: function_call_location,
                    scope: context.scope.clone(),
                },
            )?;
        }

        NodeKind::HandledFallibleFunctionCall {
            name,
            args,
            result_type_ids,
            handling,
            location,
        } => {
            let normalized_args = normalize_call_arguments(&args);
            let function_call_expression =
                Expression::handled_fallible_function_call_with_typed_arguments(
                    name,
                    normalized_args,
                    result_type_ids,
                    handling,
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
                    kind: NodeKind::Rvalue(function_call_expression),
                    location: function_call_location,
                    scope: context.scope.clone(),
                },
            )?;
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
                    location: function_call_location,
                    scope: context.scope.clone(),
                },
            )?;
        }

        // Call parsing only produces `FunctionCall` or `HandledFallibleFunctionCall`
        // nodes, plus value-block `Rvalue` for value-producing catch recovery.
        _ => {}
    }

    Ok(())
}
