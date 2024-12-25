use super::{ast_nodes::AstNode, expressions::parse_expression::create_expression};
use crate::{bs_types::DataType, CompileError, Token};
use crate::parsers::ast_nodes::{Arg, Node};

// This is a dynamic array of one data type
// TODO: string keys to make it a map
pub fn new_collection(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    collection_type: &mut DataType,
    variable_declarations: &mut Vec<Arg>,
) -> Result<AstNode, CompileError> {
    let mut items: Vec<AstNode> = Vec::new();

    // Should always start with current token being an open scope
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
                    token_line_numbers,
                )?;
                
                if item.get_type() != *collection_type {
                    return Err(CompileError {
                        msg: format!("Type mismatch in collection. Expected type: {:?}, got type: {:?}", collection_type, item.get_type()),
                        line_number: token_line_numbers[*i].to_owned(),
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

    Ok(AstNode::Collection(
        items,
        DataType::Collection(Box::new(collection_type.to_owned())),
        token_line_numbers[*i])
    )
}
