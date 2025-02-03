use super::{ast_nodes::AstNode, expressions::parse_expression::create_expression};
use crate::parsers::ast_nodes::{Arg, NodeInfo, Value};
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};

// This is a dynamic array of one data type
// TODO - look through and update / test this code as a lot has changed
// TODO: string keys to make it a map
pub fn _new_collection(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_positions: &Vec<TokenPosition>,
    collection_type: &mut DataType,
    variable_declarations: &mut Vec<Arg>,
) -> Result<Value, CompileError> {
    let mut items: Vec<Value> = Vec::new();

    // Should always start with current token being an open curly brace
    // So skip to first value
    *i += 1;

    while let Some(token) = tokens.get(*i) {
        match token {
            Token::CloseCurly => {
                break;
            }

            _ => {
                let item = create_expression(
                    tokens,
                    i,
                    true,
                    ast,
                    collection_type,
                    false,
                    variable_declarations,
                    token_positions,
                )?;

                if item.get_type() != *collection_type {
                    return Err(CompileError {
                        msg: format!(
                            "Type mismatch in collection. Expected type: {:?}, got type: {:?}",
                            collection_type,
                            item.get_type()
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column
                                + item.dimensions().char_column,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                if collection_type == &DataType::Inferred {
                    *collection_type = item.get_type();
                }

                items.push(item);
            }
        }

        *i += 1;
    }

    Ok(Value::Collection(items, collection_type.to_owned()))
}
