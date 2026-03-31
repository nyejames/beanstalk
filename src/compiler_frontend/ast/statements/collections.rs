//! Collection literal parsing helpers.
//!
//! WHAT: parses `{...}` literals into `ExpressionKind::Collection` during AST construction.
//! WHY: collection parsing must share the normal expression parser for item type-checking.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership::MutableOwned;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_syntax_error;

/// Parse a collection literal with homogeneous item expressions.
pub fn new_collection(
    token_stream: &mut FileTokens,
    collection_type: &DataType,
    context: &ScopeContext,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut items: Vec<Expression> = Vec::new();
    let mut consumed_close_curly = false;

    // Should always start with the current token being an open curly brace,
    // So skip to the first value
    token_stream.advance();

    let mut next_item: bool = true;

    while token_stream.index < token_stream.length {
        match token_stream.current_token_kind() {
            TokenKind::CloseCurly => {
                consumed_close_curly = true;
                break;
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Comma => {
                if next_item {
                    return_syntax_error!(
                        "Expected a collection item after the comma",
                        token_stream.current_location()
                    )
                }

                next_item = true;
                token_stream.advance();
            }

            _ => {
                if !next_item {
                    return_syntax_error!(
                        "Expected a collection item after the comma",
                        token_stream.current_location()
                    )
                }

                let mut collection_inner_type = collection_type.to_owned();
                let item = create_expression(
                    token_stream,
                    context,
                    &mut collection_inner_type,
                    &MutableOwned,
                    false,
                    string_table,
                )?;

                items.push(item);

                next_item = false;
            }
        }
    }

    if !consumed_close_curly {
        return_syntax_error!(
            "Unterminated collection literal. Expected '}' before end of expression.",
            token_stream.current_location()
        )
    }

    Ok(Expression::collection(
        items,
        token_stream.current_location(),
        ownership.to_owned(),
    ))
}

#[cfg(test)]
#[path = "tests/collections_tests.rs"]
mod collections_tests;
