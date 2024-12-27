use super::{
    ast_nodes::{Arg, AstNode},
    variables::create_new_var_or_ref,
};
use crate::bs_types::DataType;
use crate::parsers::ast_nodes::Value;
use crate::parsers::expressions::parse_expression::create_expression;
use crate::{parsers::ast_nodes::NodeInfo, CompileError, Token};

// Assumes to have started after the open parenthesis
// Datatype must always be a tuple containing the data types of the items in the tuple
// Or inferred if the data type is not known
// Also modifies the data type passed into it
// If there is only one item in the tuple, it just returns that item
pub fn new_tuple(
    initial_value: Value,
    tokens: &Vec<Token>,
    i: &mut usize,
    required_args: &Vec<Arg>,
    ast: &Vec<AstNode>,
    variable_declarations: &mut Vec<Arg>,
    token_line_numbers: &Vec<u32>,
) -> Result<Vec<Arg>, CompileError> {
    let mut item_args = required_args.to_owned();

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

    while let Some(token) = tokens.get(*i) {
        match token {
            Token::CloseParenthesis => {
                *i += 1;
                break;
            }

            Token::Comma => {
                if next_item {
                    return Err(CompileError {
                        msg: "Expected a tuple item after the comma".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
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
                        msg: "Expected a comma between tuple declarations".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                let new_var = create_new_var_or_ref(
                    name,
                    variable_declarations,
                    tokens,
                    i,
                    false,
                    ast,
                    token_line_numbers,
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
                        msg: "Expected a comma between tuple items".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }

                next_item = false;

                let mut data_type = if required_args.len() == 0 {
                    DataType::Inferred
                } else if required_args.len() < items.len() {
                    return Err(CompileError {
                        msg: "Too many arguments provided to tuple".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
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
                    token_line_numbers,
                )?;

                // Get the arg of this tuple item
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
