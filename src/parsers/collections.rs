use super::expressions::parse_expression::create_expression;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::build_ast::TokenContext;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};

// This is a dynamic array of one data type
// TODO - look through and update / test this code as a lot has changed
// TODO: string keys to make it a map
pub fn new_collection(
    x: &mut TokenContext,
    collection_type: &DataType,
    variable_declarations: &[Arg],
) -> Result<Expr, CompileError> {
    let mut items: Vec<Expr> = Vec::new();

    // Should always start with the current token being an open curly brace,
    // So skip to the first value
    x.advance();

    let mut next_item: bool = true;

    while x.index < x.length {
        match x.current_token() {
            Token::CloseCurly => {
                break;
            }

            Token::Newline => {
                x.advance();
            }

            Token::Comma => {
                if next_item {
                    return Err(CompileError {
                        msg: "Expected a collection item after the comma".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Syntax,
                    });
                }

                next_item = true;
                x.advance();
            }

            _ => {
                if !next_item {
                    return Err(CompileError {
                        msg: "Expected a comma between items in this collection".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Syntax,
                    });
                }

                let mut collection_inner_type = collection_type.to_owned();
                let item = create_expression(
                    x,
                    &mut collection_inner_type,
                    false,
                    variable_declarations,
                    &[],
                )?;

                items.push(item);

                next_item = false;
            }
        }
    }

    Ok(Expr::Collection(items, collection_type.to_owned()))
}
