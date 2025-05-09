use super::{ast_nodes::AstNode, expressions::parse_expression::create_expression};
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::build_ast::TokenContext;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};

// This is a dynamic array of one data type
// TODO - look through and update / test this code as a lot has changed
// TODO: string keys to make it a map
pub fn _new_collection(
    x: &mut TokenContext,
    ast: &[AstNode],
    token_positions: &[TokenPosition],
    collection_type: &mut DataType,
    variable_declarations: &mut Vec<Arg>,
) -> Result<Expr, CompileError> {
    let mut items: Vec<Expr> = Vec::new();

    // Should always start with current token being an open curly brace
    // So skip to first value
    x.index += 1;

    while x.index < x.length {
        match x.current_token() {
            Token::CloseCurly => {
                break;
            }

            _ => {
                let item =
                    create_expression(x, true, ast, collection_type, false, variable_declarations)?;

                let item_type = item.get_type();
                if item_type != *collection_type {
                    return Err(CompileError {
                        msg: format!(
                            "Type mismatch in collection. Expected type: {:?}, got type: {:?}",
                            collection_type, item_type
                        ),
                        start_pos: token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[x.index].line_number,
                            char_column: token_positions[x.index].char_column
                                + item.dimensions().char_column,
                        },
                        error_type: ErrorType::Type,
                    });
                }

                match collection_type {
                    DataType::Inferred(_) => {
                        *collection_type = item_type;
                    }
                    _ => {}
                }

                items.push(item);
            }
        }

        x.index += 1;
    }

    Ok(Expr::Collection(items, collection_type.to_owned()))
}
