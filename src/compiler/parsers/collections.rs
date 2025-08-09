use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::return_syntax_error;

// This is a dynamic array of one data type
// TODO - look through and update / test this code as a lot has changed
// TODO: string keys to make it a map
pub fn new_collection(
    token_stream: &mut TokenContext,
    collection_type: &DataType,
    context: &ScopeContext,
) -> Result<Expression, CompileError> {
    let mut items: Vec<Expression> = Vec::new();

    // Should always start with the current token being an open curly brace,
    // So skip to the first value
    token_stream.advance();

    let mut next_item: bool = true;

    while token_stream.index < token_stream.length {
        match token_stream.current_token_kind() {
            TokenKind::CloseCurly => {
                break;
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Comma => {
                if next_item {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Expected a collection item after the comma"
                    )
                }

                next_item = true;
                token_stream.advance();
            }

            _ => {
                if !next_item {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Expected a collection item after the comma"
                    )
                }

                let mut collection_inner_type = collection_type.to_owned();
                let item =
                    create_expression(token_stream, context, &mut collection_inner_type, false)?;

                items.push(item);

                next_item = false;
            }
        }
    }

    Ok(Expression::collection(
        items,
        token_stream.current_location(),
    ))
}
