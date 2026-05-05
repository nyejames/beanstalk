//! AST expression parsing and expression-list helpers.
//!
//! WHAT: parses token streams into typed AST expressions before evaluation and lowering.
//! WHY: expression parsing centralizes precedence, call parsing, and place-expression rules in one pass.

use super::eval_expression::evaluate_expression;
use super::expression::Expression;
use super::parse_expression_dispatch::{
    ExpressionDispatchState, ExpressionTokenStep, dispatch_expression_token,
};
use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_syntax_error, return_type_error};

// WHAT: parses a comma-separated expression list against already-known expected result types.
// WHY: function calls and multi-return contexts must preserve arity and per-slot type
//      expectations while still sharing the normal expression parser.
pub fn create_multiple_expressions(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    context_label: &str,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let mut expressions: Vec<Expression> = Vec::new();
    for (type_index, expected_type) in context.expected_result_types.iter().enumerate() {
        // Pass parse-time context only for context-sensitive literals. Other
        // expressions resolve their natural type here, and callers own
        // validation or coercion after this call returns.
        let mut expr_type = parse_expectation_for_target_type(expected_type);
        let expression = create_expression_with_trailing_newline_policy(
            token_stream,
            context,
            &mut expr_type,
            &ValueMode::ImmutableOwned,
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

// WHAT: parses one expression and evaluates the AST fragment into a typed expression node.
// WHY: expression parsing is the choke point where token structure, place rules, and expected
//      type information meet before later lowering stages.
pub fn create_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    value_mode: &ValueMode,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        data_type,
        value_mode,
        consume_closing_parenthesis,
        true,
        string_table,
    )
}

// WHAT: shared expression parser entry with configurable trailing-newline behavior.
// WHY: callers parsing comma-separated lists outside parentheses (for example
//      fallback/return lists) must preserve line boundaries between statements.
pub(crate) fn create_expression_with_trailing_newline_policy(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    value_mode: &ValueMode,
    consume_closing_parenthesis: bool,
    skip_trailing_newlines: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut expression: Vec<AstNode> = Vec::new();

    ast_log!(
        "Parsing ",
        #value_mode,
        data_type.display_with_table(string_table),
        " Expression"
    );

    // Build the flat infix AST fragment first. `evaluate_expression` is the stage that turns
    // this fragment into precedence-ordered RPN, resolves the final type, and folds constants.
    let mut next_number_negative = false;
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();
        ast_log!("Parsing expression: ", #token);
        let mut dispatch_state = ExpressionDispatchState {
            data_type,
            value_mode,
            consume_closing_parenthesis,
            expression: &mut expression,
            next_number_negative: &mut next_number_negative,
        };
        match dispatch_expression_token(
            token,
            token_stream,
            context,
            &mut dispatch_state,
            string_table,
        )? {
            ExpressionTokenStep::Continue => continue,
            ExpressionTokenStep::Advance => token_stream.advance(),
            ExpressionTokenStep::Break => break,
            ExpressionTokenStep::Return(value) => return Ok(*value),
        }
    }

    if skip_trailing_newlines {
        token_stream.skip_newlines();
    }

    evaluate_expression(context, expression, data_type, value_mode, string_table)
}

// WHAT: parses an expression from a bounded token slice without consuming the stop token.
// WHY: some parent parsers need normal expression semantics while reserving a delimiter for the
//      surrounding grammar layer to inspect and consume itself.
pub(crate) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    value_mode: &ValueMode,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if stop_tokens.is_empty() {
        return create_expression(
            token_stream,
            context,
            data_type,
            value_mode,
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

    let window_token_count = end_index.saturating_sub(start_index);
    add_ast_counter(AstCounter::BoundedExpressionTokenWindows, 1);
    add_ast_counter(
        AstCounter::BoundedExpressionTokenCopiesAvoided,
        window_token_count,
    );

    let original_length = token_stream.length;
    token_stream.length = end_index;

    let result = create_expression(
        token_stream,
        context,
        data_type,
        value_mode,
        false,
        string_table,
    );

    token_stream.length = original_length;
    token_stream.index = end_index;
    result
}

#[cfg(test)]
#[path = "tests/bounded_expression_tests.rs"]
mod bounded_expression_tests;
