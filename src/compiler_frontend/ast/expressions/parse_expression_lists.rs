//! Expression-list and bounded-expression parsing helpers.
//!
//! WHAT: parses expression lists and bounded sub-expressions.
//! WHY: list parsing and stop-token parsing are entrypoint variants, not general token-dispatch responsibilities.

use super::expression::Expression;
use super::parse_expression::{create_expression, create_expression_with_trailing_newline_policy};
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::{return_syntax_error, return_type_error};

// WHAT: parses a comma-separated expression list against already-known expected result types.
// WHY: function calls and multi-return contexts must preserve arity and per-slot type
//      expectations while still sharing the normal expression parser.
pub(super) fn create_multiple_expressions(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    context_label: &str,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let mut expressions: Vec<Expression> = Vec::new();
    for (type_index, expected_type) in context.expected_result_types.iter().enumerate() {
        // Pass Inferred for concrete scalar/composite types so that eval_expression stays
        // strict (Exact context); callers own their own coercion or validation after this
        // call returns. Pass the expected type through only for Option variants so that
        // `none` literals can resolve their inner type from the surrounding context.
        let mut expr_type = parse_expectation_for_target_type(expected_type);
        let expression = create_expression_with_trailing_newline_policy(
            token_stream,
            context,
            &mut expr_type,
            &Ownership::ImmutableOwned,
            false,
            consume_closing_parenthesis,
            string_table,
        )?;

        expressions.push(expression);

        // Newlines are expression terminators almost everywhere else. Only normalize
        // them here when we're inside a parenthesized list so multiline calls like
        // `io(\n value\n)` leave us positioned on the comma or `)`.
        if consume_closing_parenthesis && token_stream.current_token_kind() == &TokenKind::Newline {
            token_stream.skip_newlines();
        }

        if type_index + 1 < context.expected_result_types.len() {
            if token_stream.current_token_kind() != &TokenKind::Comma {
                return_type_error!(
                    format!(
                        "Too few {} provided. Expected {}, provided {}.",
                        context_label,
                        context.expected_result_types.len(),
                        expressions.len()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Add missing values to match the expected count",
                    }
                )
            }

            token_stream.advance();
        }
    }

    if consume_closing_parenthesis {
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "Expected closing parenthesis after arguments, found '{:?}'",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Add ')' after the final argument",
                    SuggestedInsertion => ")",
                }
            );
        }

        token_stream.advance();
    }

    Ok(expressions)
}

// WHAT: parses an expression from a bounded token slice without consuming the stop token.
// WHY: some parent parsers need normal expression semantics while reserving a delimiter for the
//      surrounding grammar layer to inspect and consume itself.
pub(super) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if stop_tokens.is_empty() {
        return create_expression(
            token_stream,
            context,
            data_type,
            ownership,
            false,
            string_table,
        );
    }

    let start_index = token_stream.index;
    let end_index = find_expression_end_index(&token_stream.tokens, start_index, stop_tokens);

    if end_index >= token_stream.length {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression, but reached end of file",
                expected_tokens
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Complete the expression and add the required delimiter token",
            }
        )
    }

    if end_index == start_index {
        return_syntax_error!(
            "Expected an expression before this delimiter",
            token_stream.tokens[end_index]
                .location.clone()
                ,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add a valid expression before this token",
            }
        )
    }

    if !stop_tokens
        .iter()
        .any(|stop| token_stream.tokens[end_index].kind == *stop)
    {
        let expected_tokens = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect::<Vec<_>>()
            .join(", ");

        return_syntax_error!(
            format!(
                "Expected one of [{}] to end this expression",
                expected_tokens
            ),
            token_stream.tokens[end_index]
                .location.clone()
                ,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Add the required delimiter token after this expression",
            }
        )
    }

    let mut expression_tokens = token_stream.tokens[start_index..end_index].to_vec();
    expression_tokens.push(Token::new(
        TokenKind::Eof,
        token_stream.tokens[end_index].location.clone(),
    ));

    let mut scoped_stream = FileTokens::new(token_stream.src_path.clone(), expression_tokens);
    let expression = create_expression(
        &mut scoped_stream,
        context,
        data_type,
        ownership,
        false,
        string_table,
    )?;

    token_stream.index = end_index;
    Ok(expression)
}
