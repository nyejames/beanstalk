//! Literal expression parsing helpers.
//!
//! WHAT: parses literal tokens and option-none literal rules.
//! WHY: literal semantics are independent from identifier/call logic and easier to validate in isolation.

use super::error::ExpressionParseError;
use super::expression::Expression;
use super::parse_expression_dispatch::push_expression_node;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

pub(super) struct LiteralParseState<'a> {
    pub(super) expected_type: &'a ExpectedType,
    pub(super) value_mode: &'a ValueMode,
    pub(super) expression: &'a mut Vec<AstNode>,
    pub(super) next_number_negative: &'a mut bool,
    pub(super) allow_boundary_catch: bool,
}

/// Parse a single literal token and push the resulting AST node.
///
/// WHAT: handles numeric, text, boolean, and option-none literals.
/// WHY: literals are self-contained tokens that do not need identifier resolution or
/// postfix chaining, so they can be validated and emitted in one step.
///
/// `next_number_negative` is set by the dispatch layer when a unary `-` operator precedes
/// a number literal. We fold the sign into the literal value here so that `-42` becomes a
/// single constant rather than a unary-minus expression node.
pub(super) fn parse_literal_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    state: &mut LiteralParseState<'_>,
    string_table: &mut StringTable,
) -> Result<(), ExpressionParseError> {
    match token_stream.current_token_kind().to_owned() {
        TokenKind::FloatLiteral(mut float) => {
            if *state.next_number_negative {
                float = -float;
                *state.next_number_negative = false;
            }

            let location = token_stream.current_location();
            let float_expr =
                Expression::float(float, location.to_owned(), state.value_mode.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(float_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::IntLiteral(mut int) => {
            if *state.next_number_negative {
                *state.next_number_negative = false;

                // Fold the unary minus into the literal, but reject i64::MIN overflow
                // since the positive value cannot be represented.
                int = match int.checked_neg() {
                    Some(value) => value,
                    None => {
                        return Err(CompilerDiagnostic::compile_time_evaluation_error(
                            CompileTimeEvaluationErrorReason::IntegerOverflow,
                            None,
                            token_stream.current_location(),
                        )
                        .into());
                    }
                };
            }

            let location = token_stream.current_location();
            let int_expr = Expression::int(int, location.to_owned(), state.value_mode.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(int_expr),
                    scope: context.scope.clone(),
                    location,
                },
            )?;
            Ok(())
        }

        TokenKind::StringSliceLiteral(string) => {
            let location = token_stream.current_location();
            let string_expr =
                Expression::string_slice(string, location.to_owned(), state.value_mode.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(string_expr),
                    scope: context.scope.clone(),
                    location,
                },
            )?;
            Ok(())
        }

        TokenKind::BoolLiteral(value) => {
            let location = token_stream.current_location();
            let bool_expr =
                Expression::bool(value, location.to_owned(), state.value_mode.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(bool_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::CharLiteral(value) => {
            let location = token_stream.current_location();
            let char_expr =
                Expression::char(value, location.to_owned(), state.value_mode.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(char_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::NoneLiteral => {
            let (inner_type_id, inner_diagnostic_type) =
                if let ExpectedType::Known(expected_type_id) = state.expected_type {
                    let type_environment = type_interner.environment();
                    let Some(inner_type_id) = type_environment.option_inner_type(*expected_type_id)
                    else {
                        return Err(CompilerDiagnostic::compile_time_evaluation_error(
                        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext,
                        None,
                        token_stream.current_location(),
                    )
                    .into());
                    };

                    (
                        inner_type_id,
                        diagnostic_type_spelling(inner_type_id, type_environment),
                    )
                } else if none_literal_has_option_equality_context(token_stream) {
                    // Comparisons like `value is none` and `none is value` infer the option
                    // shape from the opposite operand during evaluation.
                    (type_interner.builtins().none, DataType::Inferred)
                } else {
                    return Err(CompilerDiagnostic::compile_time_evaluation_error(
                        CompileTimeEvaluationErrorReason::NoneLiteralRequiresOptionalTypeContext,
                        None,
                        token_stream.current_location(),
                    )
                    .into());
                };

            let location = token_stream.current_location();
            let mut none_expr = Expression::option_none_with_type_id(
                inner_type_id,
                inner_diagnostic_type,
                type_interner.environment_mut_for_derived_types(),
                location.clone(),
            );
            none_expr.value_mode = state.value_mode.to_owned();
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                type_interner,
                string_table,
                state.expression,
                state.allow_boundary_catch,
                AstNode {
                    kind: NodeKind::Rvalue(none_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        _ => Ok(()),
    }
}

/// Detect whether `none` appears next to an `is` or `not` operator.
///
/// WHAT: allows bare `none` in equality comparisons (`value is none`, `none is value`)
/// even when there is no explicit `ExpectedType` context.
/// WHY: the type of `none` can be inferred from the other operand during later
/// type-checking, so rejecting it here would be overly strict.
fn none_literal_has_option_equality_context(token_stream: &FileTokens) -> bool {
    let follows_equality_operator = token_stream.index > 0
        && matches!(
            token_stream.previous_token(),
            TokenKind::Is | TokenKind::Not
        );
    let leads_equality_operator = matches!(token_stream.peek_next_token(), Some(TokenKind::Is));

    follows_equality_operator || leads_equality_operator
}

#[cfg(test)]
#[path = "tests/parse_expression_literals_tests.rs"]
mod tests;
