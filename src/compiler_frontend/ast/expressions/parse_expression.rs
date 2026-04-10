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
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

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
    super::parse_expression_lists::create_multiple_expressions(
        token_stream,
        context,
        context_label,
        consume_closing_parenthesis,
        string_table,
    )
}

// WHAT: parses one expression and evaluates the AST fragment into a typed expression node.
// WHY: expression parsing is the choke point where token structure, place rules, and expected
//      type information meet before later lowering stages.
pub fn create_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    consume_closing_parenthesis: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        data_type,
        ownership,
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
    ownership: &Ownership,
    consume_closing_parenthesis: bool,
    skip_trailing_newlines: bool,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut expression: Vec<AstNode> = Vec::new();

    ast_log!(
        "Parsing ",
        #ownership,
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
            ownership,
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

    evaluate_expression(context, expression, data_type, ownership, string_table)
}

// WHAT: parses an expression from a bounded token slice without consuming the stop token.
// WHY: some parent parsers need normal expression semantics while reserving a delimiter for the
//      surrounding grammar layer to inspect and consume itself.
pub(crate) fn create_expression_until(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &mut DataType,
    ownership: &Ownership,
    stop_tokens: &[TokenKind],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    super::parse_expression_lists::create_expression_until(
        token_stream,
        context,
        data_type,
        ownership,
        stop_tokens,
        string_table,
    )
}
