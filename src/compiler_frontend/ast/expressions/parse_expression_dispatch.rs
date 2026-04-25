//! Expression token dispatch helpers.
//!
//! WHAT: routes one token at a time through expression-position parsing.
//! WHY: keeps delimiter/grammar ownership explicit while specialized helpers own detailed token families.

use super::eval_expression::evaluate_expression;
use super::expression::{Expression, Operator};
use super::parse_expression::create_expression;
use super::parse_expression_identifiers::parse_identifier_or_call;
use super::parse_expression_literals::parse_literal_expression;
use super::parse_expression_places::{
    parse_copy_place_expression, parse_mutable_receiver_expression,
};
use super::parse_expression_templates::parse_template_expression;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::field_access::{ReceiverAccessMode, parse_postfix_chain};
use crate::compiler_frontend::ast::statements::result_handling::parse_result_handling_suffix_for_expression;
use crate::compiler_frontend::builtins::expression_parsing::{
    parse_builtin_cast_expression, parse_collection_expression,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::syntax_errors::expression_position::check_expression_common_mistake;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{ast_log, return_syntax_error, return_type_error};

pub(super) enum ExpressionTokenStep {
    Continue,
    Advance,
    Break,
    Return(Box<Expression>),
}

pub(super) struct ExpressionDispatchState<'a> {
    pub(super) data_type: &'a mut DataType,
    pub(super) value_mode: &'a ValueMode,
    pub(super) consume_closing_parenthesis: bool,
    pub(super) expression: &'a mut Vec<AstNode>,
    pub(super) next_number_negative: &'a mut bool,
}

pub(super) fn push_expression_node(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
    expression: &mut Vec<AstNode>,
    node: AstNode,
) -> Result<(), CompilerError> {
    // Postfix parsing happens after the primary node exists so chains like `value.field ! fallback`
    // bind to the fully-built primary expression instead of only the leading identifier token.
    let node = if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Dot
    {
        parse_postfix_chain(
            token_stream,
            node,
            ReceiverAccessMode::Shared,
            context,
            string_table,
        )?
    } else {
        node
    };

    // Detect `!=` (Bang + Assign) before treating `!` as a result-handling suffix.
    if token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Bang
        && token_stream.peek_next_token() == Some(&TokenKind::Assign)
    {
        return Err(check_expression_common_mistake(token_stream, false)
            .expect("Bang+Assign should always produce an error"));
    }

    let node = if token_stream.index < token_stream.length
        && (token_stream.current_token_kind() == &TokenKind::Bang
            || (matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
                && token_stream.peek_next_token() == Some(&TokenKind::Bang)))
    {
        let handled = parse_result_handling_suffix_for_expression(
            token_stream,
            context,
            node.get_expr()?,
            true,
            None,
            string_table,
        )?;
        AstNode {
            kind: NodeKind::Rvalue(handled),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        }
    } else {
        node
    };

    expression.push(node);
    Ok(())
}

fn parse_unary_operator(
    token_stream: &FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    next_number_negative: &mut bool,
) -> bool {
    match token_stream.current_token_kind() {
        TokenKind::Negative => {
            *next_number_negative = true;
            true
        }
        TokenKind::Not => {
            expression.push(AstNode {
                kind: NodeKind::Operator(Operator::Not),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            true
        }
        _ => false,
    }
}

fn push_operator_node(
    expression: &mut Vec<AstNode>,
    context: &ScopeContext,
    location: SourceLocation,
    operator: Operator,
) {
    expression.push(AstNode {
        kind: NodeKind::Operator(operator),
        location,
        scope: context.scope.clone(),
    });
}

pub(super) fn dispatch_expression_token(
    token: TokenKind,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    state: &mut ExpressionDispatchState<'_>,
    string_table: &mut StringTable,
) -> Result<ExpressionTokenStep, CompilerError> {
    // This state machine is intentionally flat: each token either appends one AST node, advances
    // past a nested parse, or signals the caller that the surrounding grammar owns the delimiter.
    match token {
        TokenKind::CloseCurly
        | TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::TemplateClose
        | TokenKind::Arrow
        | TokenKind::StartTemplateBody
        | TokenKind::Colon
        | TokenKind::End => {
            if state.expression.is_empty() {
                match token {
                    TokenKind::Comma => {
                        let mut error = CompilerError::new_syntax_error(
                            "Unexpected ',' in expression. Commas separate list items, function arguments, or return declarations.",
                            token_stream.current_location(),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::CompilationStage,
                            String::from("Expression Parsing"),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::PrimarySuggestion,
                            String::from("Add a value before ',' or remove the comma"),
                        );
                        return Err(error);
                    }

                    TokenKind::Arrow => {
                        let mut error = CompilerError::new_syntax_error(
                            "Unexpected '->' in expression. Arrow syntax is only valid in function signatures.",
                            token_stream.current_location(),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::CompilationStage,
                            String::from("Expression Parsing"),
                        );
                        error.new_metadata_entry(
                            ErrorMetaDataKey::PrimarySuggestion,
                            String::from(
                                "Use '->' only in function signatures like '|args| -> Type:'",
                            ),
                        );
                        return Err(error);
                    }

                    _ => {}
                }
            }

            if state.consume_closing_parenthesis {
                return_syntax_error!(
                    format!("Unexpected token: '{:?}'. Seems to be missing a closing parenthesis at the end of this expression.", token),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add a closing parenthesis ')' at the end of the expression",
                        SuggestedInsertion => ")",
                    }
                )
            }

            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::CloseParenthesis => {
            if state.consume_closing_parenthesis {
                token_stream.advance();
            }

            if state.expression.is_empty() {
                return_syntax_error!(
                    "Empty expression found. Expected a value, variable, or expression.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add a value, variable reference, or expression inside the parentheses",
                    }
                );
            }

            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::OpenParenthesis => {
            token_stream.advance();
            let value = create_expression(
                token_stream,
                context,
                state.data_type,
                state.value_mode,
                true,
                string_table,
            )?;

            push_expression_node(
                token_stream,
                context,
                string_table,
                state.expression,
                AstNode {
                    kind: NodeKind::Rvalue(value),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::DatatypeInt | TokenKind::DatatypeFloat => {
            let cast_expression = parse_builtin_cast_expression(
                token_stream,
                context,
                state.value_mode,
                string_table,
            )?;
            let cast_location = cast_expression.location.clone();

            push_expression_node(
                token_stream,
                context,
                string_table,
                state.expression,
                AstNode {
                    kind: NodeKind::Rvalue(cast_expression),
                    location: cast_location,
                    scope: context.scope.clone(),
                },
            )?;

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::OpenCurly => {
            parse_collection_expression(
                token_stream,
                context,
                state.data_type,
                state.value_mode,
                state.expression,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Newline => {
            let previous_token = if token_stream.index == 0 {
                &TokenKind::Newline
            } else {
                token_stream.previous_token()
            };
            if state.consume_closing_parenthesis
                || (previous_token.continues_expression()
                    && !matches!(previous_token, TokenKind::End))
            {
                token_stream.skip_newlines();
                return Ok(ExpressionTokenStep::Continue);
            }

            // Look ahead past newlines to find the next meaningful token.
            // If that token continues the expression, skip newlines and keep parsing.
            let saved_index = token_stream.index;
            token_stream.skip_newlines();
            if token_stream.index < token_stream.length
                && token_stream.current_token_kind().continues_expression()
            {
                return Ok(ExpressionTokenStep::Continue);
            }
            token_stream.index = saved_index;

            ast_log!("Breaking out of expression with newline");
            Ok(ExpressionTokenStep::Break)
        }

        TokenKind::Symbol(..) => {
            parse_identifier_or_call(token_stream, context, state.expression, string_table)?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::Mutable => {
            parse_mutable_receiver_expression(
                token_stream,
                context,
                state.expression,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::FloatLiteral(_)
        | TokenKind::IntLiteral(_)
        | TokenKind::StringSliceLiteral(_)
        | TokenKind::BoolLiteral(_)
        | TokenKind::CharLiteral(_)
        | TokenKind::NoneLiteral => {
            parse_literal_expression(
                token_stream,
                context,
                state.data_type,
                state.value_mode,
                state.expression,
                state.next_number_negative,
                string_table,
            )?;
            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::TemplateHead => {
            if let Some(template_expression) = parse_template_expression(
                token_stream,
                context,
                state.consume_closing_parenthesis,
                state.value_mode,
                string_table,
            )? {
                return Ok(ExpressionTokenStep::Return(Box::new(template_expression)));
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Copy => {
            let copy_location = token_stream.current_location();
            token_stream.advance();

            let copied_place = parse_copy_place_expression(token_stream, context, string_table)?;
            let copied_type = copied_place.get_expr()?.data_type;

            state.expression.push(AstNode {
                kind: NodeKind::Rvalue(Expression::copy(
                    copied_place,
                    copied_type,
                    copy_location.clone(),
                    state.value_mode.to_owned(),
                )),
                location: copy_location,
                scope: context.scope.clone(),
            });

            Ok(ExpressionTokenStep::Continue)
        }

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "expression parsing",
            )?;

            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal expression element until traits are implemented",
            ))
        }

        TokenKind::Hash => {
            if token_stream.peek_next_token() != Some(&TokenKind::TemplateHead) {
                return_type_error!(
                    "Unexpected '#' in expression. '#' is only valid before a template head.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Remove '#' or place it directly before a template expression",
                    }
                );
            }

            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Negative | TokenKind::Not => {
            let _ = parse_unary_operator(
                token_stream,
                context,
                state.expression,
                state.next_number_negative,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Add => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Add,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Subtract => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Subtract,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Multiply => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Multiply,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Divide => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Divide,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::IntDivide => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::IntDivide,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Exponent => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Exponent,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Modulus => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Modulus,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Is => match token_stream.peek_next_token() {
            Some(TokenKind::Not) => {
                token_stream.advance();
                push_operator_node(
                    state.expression,
                    context,
                    token_stream.current_location(),
                    Operator::NotEqual,
                );
                Ok(ExpressionTokenStep::Advance)
            }

            Some(TokenKind::Colon) => {
                if state.expression.len() > 1 {
                    return_type_error!(
                        format!(
                            "Match statements can only have one value to match against. Found: {}",
                            state.expression.len()
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Simplify the expression to a single value before the 'is:' match",
                        }
                    )
                }

                let value = evaluate_expression(
                    context,
                    std::mem::take(state.expression),
                    state.data_type,
                    state.value_mode,
                    string_table,
                )?;
                Ok(ExpressionTokenStep::Return(Box::new(value)))
            }

            _ => {
                push_operator_node(
                    state.expression,
                    context,
                    token_stream.current_location(),
                    Operator::Equality,
                );
                Ok(ExpressionTokenStep::Advance)
            }
        },

        TokenKind::LessThan => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::LessThan,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::LessThanOrEqual => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::LessThanOrEqual,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::GreaterThan => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::GreaterThan,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::GreaterThanOrEqual => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::GreaterThanOrEqual,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::And => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::And,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::Or => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Or,
            );
            Ok(ExpressionTokenStep::Advance)
        }
        TokenKind::ExclusiveRange => {
            push_operator_node(
                state.expression,
                context,
                token_stream.current_location(),
                Operator::Range,
            );
            Ok(ExpressionTokenStep::Advance)
        }

        TokenKind::Wildcard => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected wildcard '_' in expression. Wildcards are only valid in supported pattern positions.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Expression Parsing"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Use a concrete value/expression here, or use 'else:' for default match arms",
                ),
            );
            Err(error)
        }

        TokenKind::TypeParameterBracket => {
            if let Some(error) =
                check_expression_common_mistake(token_stream, state.expression.is_empty())
            {
                return Err(error);
            }

            let mut error = CompilerError::new_syntax_error(
                "Unexpected '|' in expression. This token is only valid in signatures/struct definitions or in loop header bindings.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Expression Parsing"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray '|' or move it into a valid declaration or loop header",
                ),
            );
            Err(error)
        }

        TokenKind::AddAssign => Ok(ExpressionTokenStep::Advance),

        _ => {
            if let Some(error) =
                check_expression_common_mistake(token_stream, state.expression.is_empty())
            {
                return Err(error);
            }

            return_syntax_error!(
                format!("Invalid token used in expression: '{:?}'", token),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Remove or replace this token with a valid expression element",
                }
            )
        }
    }
}
