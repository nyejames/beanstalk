//! Function-body return statement parsing.
//!
//! WHAT: parses `return` and `return!` statements and emits validated AST return nodes.
//! WHY: return handling is signature-sensitive (arity, channels, coercion), so isolating this
//! logic keeps body dispatch simple and prevents return rules from leaking across modules.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_multiple_expressions,
};
use crate::compiler_frontend::ast::statements::value_production::{
    ValueReceiverKind, try_parse_value_block_at_receiver,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidControlFlowStatementReason, InvalidReturnShapeReason,
    TypeMismatchContext,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

/// Whether the token ends a return statement with no expression following.
fn is_return_terminator(token: &TokenKind) -> bool {
    matches!(token, TokenKind::Newline | TokenKind::End | TokenKind::Eof)
}

// --------------------------
//  Return statement parsing
// --------------------------

#[allow(clippy::result_large_err)]
pub(crate) fn parse_return_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    if context.expected_result_type_ids.is_empty()
        && !matches!(
            context.kind,
            ContextKind::Function | ContextKind::CatchHandler
        )
    {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ReturnOutsideFunction,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();

    // --------------------------
    //  Error return (return!)
    // --------------------------

    if token_stream.current_token_kind() == &TokenKind::Bang {
        let Some(expected_error_type_id) = context.expected_error_type else {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::ReturnBangOutsideErrorFunction,
                token_stream.current_location(),
            ));
        };

        token_stream.advance();
        if is_return_terminator(token_stream.current_token_kind()) {
            return Err(CompilerDiagnostic::invalid_return_shape(
                InvalidReturnShapeReason::MissingReturnBangValue,
                token_stream.current_location(),
            ));
        }

        let mut expected_error = ExpectedType::Known(expected_error_type_id);
        let returned_error = create_expression(
            token_stream,
            context,
            type_interner,
            &mut expected_error,
            &ValueMode::ImmutableOwned,
            false,
            string_table,
        )?;

        let actual_type_id = returned_error.type_id;

        if !is_declaration_compatible(
            expected_error_type_id,
            actual_type_id,
            type_interner.environment(),
        ) {
            return Err(CompilerDiagnostic::type_mismatch(
                expected_error_type_id,
                actual_type_id,
                TypeMismatchContext::ReturnValue,
                returned_error.location.clone(),
            ));
        }

        let returned_error = coerce_expression_to_declared_type(
            returned_error,
            expected_error_type_id,
            type_interner.environment(),
        );

        ast.push(AstNode {
            kind: NodeKind::ReturnError(returned_error),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        });

        return Ok(());
    }

    // --------------------------
    //  Value-producing return if
    // --------------------------

    if token_stream.current_token_kind() == &TokenKind::If
        && !context.expected_result_type_ids.is_empty()
    {
        let value_block_expr = match try_parse_value_block_at_receiver(
            token_stream,
            context,
            type_interner,
            &context.expected_result_type_ids,
            ValueReceiverKind::Return,
            string_table,
        ) {
            Some(Ok(expr)) => expr,
            Some(Err(diagnostic)) => return Err(diagnostic),
            None => {
                // Token was `if` but parsing failed at a deeper level.
                // The helper has already advanced past `if` and reported its
                // own diagnostic, so we should not fall through to normal
                // return parsing which would produce a secondary error.
                return Err(CompilerDiagnostic::invalid_control_flow_statement(
                    InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
                    token_stream.current_location(),
                ));
            }
        };

        // For single-result returns, apply a final coercion guard to preserve
        // existing behavior (e.g. Int -> Float). For multi-result, the value-block
        // parser already validated and coerced each slot.
        let return_expr = if context.expected_result_type_ids.len() == 1 {
            let expected_type_id = context.expected_result_type_ids[0];
            let actual_type_id = value_block_expr.type_id;

            if actual_type_id == expected_type_id {
                value_block_expr
            } else if is_declaration_compatible(
                expected_type_id,
                actual_type_id,
                type_interner.environment(),
            ) {
                coerce_expression_to_declared_type(
                    value_block_expr,
                    expected_type_id,
                    type_interner.environment(),
                )
            } else {
                return Err(CompilerDiagnostic::type_mismatch(
                    expected_type_id,
                    actual_type_id,
                    TypeMismatchContext::ReturnValue,
                    token_stream.current_location(),
                ));
            }
        } else {
            value_block_expr
        };

        ast.push(AstNode {
            kind: NodeKind::Return(vec![return_expr]),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        });

        return Ok(());
    }

    // --------------------------
    //  Normal return values
    // --------------------------

    let returned_values = if context.expected_result_type_ids.is_empty() {
        if is_return_terminator(token_stream.current_token_kind()) {
            Vec::new()
        } else {
            return Err(CompilerDiagnostic::invalid_return_shape(
                InvalidReturnShapeReason::ReturnValuesWithBareSignature,
                token_stream.current_location(),
            ));
        }
    } else {
        if is_return_terminator(token_stream.current_token_kind()) {
            let expected_count = context.expected_result_type_ids.len();
            return Err(CompilerDiagnostic::invalid_return_shape(
                InvalidReturnShapeReason::BareReturnWithExpectedValues { expected_count },
                token_stream.current_location(),
            ));
        }

        let parsed_return_values = create_multiple_expressions(
            token_stream,
            context,
            type_interner,
            "return values",
            false,
            string_table,
        )?;

        if token_stream.current_token_kind() == &TokenKind::Comma {
            let expected_count = context.expected_result_type_ids.len();
            return Err(CompilerDiagnostic::invalid_return_shape(
                InvalidReturnShapeReason::TooManyReturnValues { expected_count },
                token_stream.current_location(),
            ));
        }

        let mut coerced_values: Vec<Expression> = Vec::with_capacity(parsed_return_values.len());

        // Validate each returned value against the corresponding expected type,
        // applying explicit contextual coercion when a return boundary allows it.
        for (returned_value, expected_type_id) in parsed_return_values
            .into_iter()
            .zip(context.expected_result_type_ids.iter())
        {
            let actual_type_id = returned_value.type_id;

            if actual_type_id == *expected_type_id {
                coerced_values.push(returned_value);
                continue;
            }

            if is_declaration_compatible(
                *expected_type_id,
                actual_type_id,
                type_interner.environment(),
            ) {
                coerced_values.push(coerce_expression_to_declared_type(
                    returned_value,
                    *expected_type_id,
                    type_interner.environment(),
                ));
                continue;
            }

            return Err(CompilerDiagnostic::type_mismatch(
                *expected_type_id,
                actual_type_id,
                TypeMismatchContext::ReturnValue,
                returned_value.location.clone(),
            ));
        }

        coerced_values
    };

    ast.push(AstNode {
        kind: NodeKind::Return(returned_values),
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    });

    Ok(())
}
