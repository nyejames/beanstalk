#[allow(unused_imports)]
use colour::grey_ln;

use super::{
    ast_nodes::{Arg, AstNode},
    variables::create_new_var_or_ref,
};
use crate::bs_types::DataType;
use crate::parsers::ast_nodes::Value;
use crate::parsers::expressions::parse_expression::create_expression;
use crate::tokenizer::TokenPosition;
use crate::{parsers::ast_nodes::NodeInfo, CompileError, ErrorType, Token};

// Assumes to have started after the open parenthesis
// Datatype must always be a struct containing the data types of the items in the struct
// Or inferred if the data type is not known
// Also modifies the data type passed into it
pub fn new_struct(
    initial_value: Value,
    tokens: &Vec<Token>,
    i: &mut usize,
    required_args: &Vec<Arg>,
    ast: &Vec<AstNode>,
    variable_declarations: &mut Vec<Arg>,
    token_positions: &Vec<TokenPosition>,
) -> Result<Vec<Arg>, CompileError> {
    let mut item_args = required_args.to_owned();

    //grey_ln!("parsing struct");

    let mut items: Vec<Arg> = match initial_value {
        Value::None => Vec::new(),
        _ => {
            vec![Arg {
                name: "0".to_string(),
                data_type: initial_value.get_type(),
                value: initial_value,
            }]
        }
    };

    let mut next_item: bool = true;
    let mut item_name: String = "0".to_string();

    // ASSUMES AN OPEN PARENTHESIS HAS JUST BEEN PASSED
    while let Some(token) = tokens.get(*i) {
        match token {
            Token::CloseParenthesis => {
                *i += 1;
                break;
            }

            Token::Comma => {
                if next_item {
                    return Err(CompileError {
                        msg: "Expected a struct item after the comma".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                next_item = true;
                *i += 1;
            }

            Token::Newline => {
                *i += 1;
            }

            Token::Variable(name) => {
                if !next_item {
                    return Err(CompileError {
                        msg: "Expected a comma between struct declarations".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + name.len() as u32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                let new_var = create_new_var_or_ref(
                    name,
                    variable_declarations,
                    tokens,
                    i,
                    false,
                    ast,
                    token_positions,
                    true,
                )?;

                let item_arg = Arg {
                    name: name.to_owned(),
                    data_type: new_var.get_type(),
                    value: new_var.get_value(),
                };

                items.push(item_arg.to_owned());
                item_args.push(item_arg);
                item_name = items.len().to_string();

                next_item = false;
            }

            _ => {
                if !next_item {
                    return Err(CompileError {
                        msg: "Expected a comma between struct items".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                next_item = false;

                let mut data_type = if required_args.len() == 0 {
                    DataType::Inferred
                } else if required_args.len() < items.len() {
                    return Err(CompileError {
                        msg: "Too many arguments provided to struct".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                } else {
                    required_args[items.len()].data_type.to_owned()
                };

                let arg_value = create_expression(
                    tokens,
                    i,
                    true,
                    ast,
                    &mut data_type,
                    false,
                    variable_declarations,
                    token_positions,
                )?;

                // Get the arg of this struct item
                let item_arg = match item_args.get(items.len()) {
                    Some(arg) => arg.to_owned(),
                    None => Arg {
                        name: item_name,
                        data_type: data_type.to_owned(),
                        value: arg_value,
                    },
                };

                items.push(item_arg.to_owned());
                item_args.push(item_arg);
                item_name = items.len().to_string();
            }
        }
    }

    Ok(items)
}

// AUTOMATICALLY TURNS STRUCTS OF ONE ITEM INTO THAT ITEM
// This is a weird/unique design choice of the language
// Every time an expression is parsed, it will turn a struct of one item into that item
pub fn struct_to_value(args: &Vec<Arg>) -> Value {
    // An empty struct is None in this language
    if args.len() < 1 {
        return Value::None;
    }

    // Automatically convert structs of one item into that item
    if args.len() == 1 {
        return args[0].value.to_owned();
    }

    Value::Structure(args.to_owned())
}
