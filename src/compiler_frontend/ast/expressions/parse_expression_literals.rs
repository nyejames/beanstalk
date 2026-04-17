//! Literal expression parsing helpers.
//!
//! WHAT: parses literal tokens and option-none literal rules.
//! WHY: literal semantics are independent from identifier/call logic and easier to validate in isolation.

use super::expression::Expression;
use super::parse_expression_dispatch::push_expression_node;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

pub(super) fn parse_literal_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expected_type: &DataType,
    ownership: &Ownership,
    expression: &mut Vec<AstNode>,
    next_number_negative: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match token_stream.current_token_kind().to_owned() {
        TokenKind::FloatLiteral(mut float) => {
            if *next_number_negative {
                float = -float;
                *next_number_negative = false;
            }

            let location = token_stream.current_location();
            let float_expr = Expression::float(float, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(float_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::IntLiteral(mut int) => {
            if *next_number_negative {
                *next_number_negative = false;
                int = -int;
            };

            let location = token_stream.current_location();
            let int_expr = Expression::int(int, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
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
                Expression::string_slice(string, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
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
            let bool_expr = Expression::bool(value, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
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
            let char_expr = Expression::char(value, location.to_owned(), ownership.to_owned());
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
                AstNode {
                    kind: NodeKind::Rvalue(char_expr),
                    location,
                    scope: context.scope.clone(),
                },
            )?;
            Ok(())
        }

        TokenKind::NoneLiteral => {
            let inner_type = if let DataType::Option(inner_type) = expected_type {
                inner_type.as_ref().to_owned()
            } else if token_stream.index > 0
                && matches!(
                    token_stream.previous_token(),
                    TokenKind::Is | TokenKind::Not
                )
            {
                // Comparisons like `value is none` infer the option shape from the
                // left-hand side expression during evaluation.
                DataType::Inferred
            } else {
                return_rule_error!(
                    "The 'none' literal requires an explicit optional type context",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Use 'none' only where a concrete optional type is expected (for example 'String?')",
                    }
                );
            };

            let location = token_stream.current_location();
            // Propagate the binding's ownership so that `name ~String? = none`
            // produces a mutable binding. Other literals (int, float, string)
            // already receive ownership from the same parameter; none must too.
            let mut none_expr = Expression::option_none(inner_type, location.clone());
            none_expr.ownership = ownership.to_owned();
            token_stream.advance();
            push_expression_node(
                token_stream,
                context,
                string_table,
                expression,
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
