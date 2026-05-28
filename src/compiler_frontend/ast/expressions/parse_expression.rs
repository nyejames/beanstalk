//! AST expression parsing and expression-list helpers.
//!
//! WHAT: parses token streams into typed AST expressions before evaluation and lowering.
//! WHY: expression parsing centralizes precedence, call parsing, and place-expression rules in one pass.

use super::error::ExpressionParseError;
use super::eval_expression::evaluate_expression;
use super::expression::Expression;
use super::parse_expression_dispatch::{
    ExpressionDispatchState, ExpressionTokenStep, dispatch_expression_token,
};
use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidReturnShapeReason};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::{
    ExpectedType, parse_expectation_for_type_id,
};
use crate::compiler_frontend::value_mode::ValueMode;

/// Policy that controls how the expression parser handles trailing tokens
/// such as closing delimiters and recovery boundaries.
pub(crate) struct ExpressionTrailingPolicy {
    pub(crate) consume_closing_parenthesis: bool,
    pub(crate) skip_trailing_newlines: bool,
    /// `catch` is boundary-only. Nested expression parsers use this flag to keep
    /// function arguments, collection items, and parenthesized subexpressions
    /// from silently becoming recovery boundaries.
    pub(crate) allow_boundary_catch: bool,
}

/// Policy for parsing expressions bounded by one or more stop tokens.
struct BoundedExpressionPolicy<'a> {
    stop_tokens: &'a [TokenKind],
    allow_boundary_catch: bool,
}

// WHAT: parses a comma-separated expression list against already-known expected result types.
// WHY: function calls and multi-return contexts must preserve arity and per-slot type
//      expectations while still sharing the normal expression parser.
pub fn create_multiple_expressions(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    _context_label: &str,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, ExpressionParseError> {
    create_multiple_expressions_inner(
        token_stream,
        context,
        type_interner,
        consume_closing_parenthesis,
        string_table,
    )
}

fn create_multiple_expressions_inner(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, ExpressionParseError> {
    let mut expressions: Vec<Expression> = Vec::new();

    for (type_index, expected_type) in context.expected_result_type_ids.iter().enumerate() {
        // Pass parse-time context only for context-sensitive literals. Other
        // expressions resolve their natural type here, and callers own
        // validation or coercion after this call returns.
        let mut expression_type =
            parse_expectation_for_type_id(*expected_type, type_interner.environment());
        let expression = create_expression_with_trailing_newline_policy(
            token_stream,
            context,
            type_interner,
            &mut expression_type,
            &ValueMode::ImmutableOwned,
            ExpressionTrailingPolicy {
                consume_closing_parenthesis,
                skip_trailing_newlines: false,
                allow_boundary_catch: !consume_closing_parenthesis,
            },
            string_table,
        )?;

        expressions.push(expression);

        // Newlines are expression terminators almost everywhere else. Only normalize
        // them here when we're inside a parenthesized list so multiline calls like
        // `io(\n value\n)` leave us positioned on the comma or `)`.
        if consume_closing_parenthesis && token_stream.current_token_kind() == &TokenKind::Newline {
            token_stream.skip_newlines();
        }

        // Comma check: every slot except the last must be followed by a comma.
        if type_index + 1 < context.expected_result_type_ids.len() {
            if token_stream.current_token_kind() != &TokenKind::Comma {
                return Err(CompilerDiagnostic::invalid_return_shape(
                    InvalidReturnShapeReason::TooFewReturnValues {
                        expected_count: context.expected_result_type_ids.len(),
                        provided_count: expressions.len(),
                    },
                    token_stream.current_location(),
                )
                .into());
            }

            token_stream.advance();
        }
    }

    if consume_closing_parenthesis {
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return Err(CompilerDiagnostic::expected_token(
                TokenKind::CloseParenthesis,
                Some(token_stream.current_token_kind().to_owned()),
                token_stream.current_location(),
            )
            .into());
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
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        type_interner,
        expected_type,
        value_mode,
        ExpressionTrailingPolicy {
            consume_closing_parenthesis,
            skip_trailing_newlines: true,
            allow_boundary_catch: !consume_closing_parenthesis,
        },
        string_table,
    )
}

// WHAT: parses a nested expression while preserving ordinary expression semantics.
// WHY: `catch` is procedural recovery syntax, so nested expression positions must reject it even
// when they reuse a function/body context that would allow `catch` at the outer statement boundary.
pub(crate) fn create_expression_without_boundary_catch(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        type_interner,
        expected_type,
        value_mode,
        ExpressionTrailingPolicy {
            consume_closing_parenthesis,
            skip_trailing_newlines: true,
            allow_boundary_catch: false,
        },
        string_table,
    )
}

// WHAT: shared expression parser entry with configurable trailing-newline behavior.
// WHY: callers parsing comma-separated lists outside parentheses (for example
//      fallback/return lists) must preserve line boundaries between statements.
pub(crate) fn create_expression_with_trailing_newline_policy(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    trailing_policy: ExpressionTrailingPolicy,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    let mut expression: Vec<AstNode> = Vec::new();

    ast_log!(
        "Parsing ",
        #value_mode,
        format!("{expected_type:?}"),
        " Expression"
    );

    // Build the flat infix AST fragment first. `evaluate_expression` is the stage that turns
    // this fragment into precedence-ordered RPN, resolves the final type, and folds constants.
    let mut next_number_negative = false;
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();
        ast_log!("Parsing expression: ", #token);
        let mut dispatch_state = ExpressionDispatchState {
            expected_type,
            value_mode,
            consume_closing_parenthesis: trailing_policy.consume_closing_parenthesis,
            allow_boundary_catch: trailing_policy.allow_boundary_catch,
            expression: &mut expression,
            next_number_negative: &mut next_number_negative,
        };
        match dispatch_expression_token(
            token,
            token_stream,
            context,
            type_interner,
            &mut dispatch_state,
            string_table,
        )? {
            ExpressionTokenStep::Continue => continue,
            ExpressionTokenStep::Advance => token_stream.advance(),
            ExpressionTokenStep::Break => break,
            ExpressionTokenStep::Return(value) => return Ok(*value),
        }
    }

    if trailing_policy.skip_trailing_newlines {
        token_stream.skip_newlines();
    }

    evaluate_expression(
        context,
        expression,
        type_interner,
        expected_type,
        value_mode,
        string_table,
    )
    .map_err(ExpressionParseError::from)
}

// WHAT: parses an expression from a bounded token slice without consuming the stop token.
// WHY: some parent parsers need normal expression semantics while reserving a delimiter for the
//      surrounding grammar layer to inspect and consume itself.
pub(crate) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    create_expression_until_inner(
        token_stream,
        context,
        type_interner,
        expected_type,
        value_mode,
        BoundedExpressionPolicy {
            stop_tokens,
            allow_boundary_catch: true,
        },
        string_table,
    )
}

// WHAT: parses a bounded expression in a grammar position that is not a recovery boundary.
// WHY: loop headers and similar nested syntactic positions may need stop-token parsing while still
// rejecting procedural `catch` recovery inside the expression.
pub(crate) fn create_expression_until_without_boundary_catch(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    create_expression_until_inner(
        token_stream,
        context,
        type_interner,
        expected_type,
        value_mode,
        BoundedExpressionPolicy {
            stop_tokens,
            allow_boundary_catch: false,
        },
        string_table,
    )
}

fn create_expression_until_inner(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    policy: BoundedExpressionPolicy<'_>,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    let stop_tokens = policy.stop_tokens;
    let allow_boundary_catch = policy.allow_boundary_catch;

    // Fast path: no stop tokens means an unbounded parse with the same boundary policy.
    if stop_tokens.is_empty() {
        let trailing_policy = ExpressionTrailingPolicy {
            consume_closing_parenthesis: false,
            skip_trailing_newlines: true,
            allow_boundary_catch,
        };

        return create_expression_with_trailing_newline_policy(
            token_stream,
            context,
            type_interner,
            expected_type,
            value_mode,
            trailing_policy,
            string_table,
        );
    }

    // ------------------------
    //  Locate expression window
    // ------------------------
    let start_index = token_stream.index;
    let end_index = find_expression_end_index(&token_stream.tokens, start_index, stop_tokens);

    // ------------------------
    //  Validate window bounds
    // ------------------------
    if end_index >= token_stream.length {
        let formatted_stop_tokens: Vec<String> = stop_tokens
            .iter()
            .map(|token| format!("{token:?}"))
            .collect();
        let expected_tokens = formatted_stop_tokens.join(", ");

        let expected_delimiter = string_table.intern(&expected_tokens);
        return Err(CompilerDiagnostic::unexpected_end_of_file(
            Some(expected_delimiter),
            token_stream.current_location(),
        )
        .into());
    }

    if end_index == start_index {
        return Err(CompilerDiagnostic::unexpected_token(
            token_stream.tokens[end_index].kind.to_owned(),
            token_stream.tokens[end_index].location.clone(),
        )
        .into());
    }

    if !stop_tokens
        .iter()
        .any(|stop| token_stream.tokens[end_index].kind == *stop)
    {
        return Err(CompilerDiagnostic::unexpected_token(
            token_stream.tokens[end_index].kind.to_owned(),
            token_stream.tokens[end_index].location.clone(),
        )
        .into());
    }

    // ------------------------
    //  Parse within window
    // ------------------------
    let window_token_count = end_index.saturating_sub(start_index);
    add_ast_counter(AstCounter::BoundedExpressionTokenWindows, 1);
    add_ast_counter(
        AstCounter::BoundedExpressionTokenCopiesAvoided,
        window_token_count,
    );

    // Narrow the visible token stream so the inner parser stops at the stop token
    // without needing to copy the slice. The original length is restored after parsing.
    let original_length = token_stream.length;
    token_stream.length = end_index;

    let result = create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        type_interner,
        expected_type,
        value_mode,
        ExpressionTrailingPolicy {
            consume_closing_parenthesis: false,
            skip_trailing_newlines: true,
            allow_boundary_catch,
        },
        string_table,
    );

    // Restore the full stream and position the cursor on the stop token.
    token_stream.length = original_length;
    token_stream.index = end_index;
    result
}

#[cfg(test)]
#[path = "tests/bounded_expression_tests.rs"]
mod bounded_expression_tests;
