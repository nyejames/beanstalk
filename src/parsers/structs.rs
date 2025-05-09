#[allow(unused_imports)]
use colour::grey_ln;

use super::{
    ast_nodes::{Arg, AstNode},
    variables::create_new_var_or_ref,
};
use crate::bs_types::DataType;
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::expressions::parse_expression::create_expression;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token};

// Assumes to have started after the open curly
// Datatype must always be a struct containing the data types of the items in the struct
// Or inferred if the data type is not known
// Also modifies the data type passed into it

// This can be created using curly brackets or parenthesis depending on context (function calls)
pub fn new_fixed_collection(
    x: &mut TokenContext,
    initial_value: Expr,
    required_args: &[Arg],
    ast: &[AstNode],
    variable_declarations: &mut Vec<Arg>,
) -> Result<Vec<Arg>, CompileError> {
    let mut item_args = required_args.to_owned();

    let mut items: Vec<Arg> = match initial_value {
        Expr::None => Vec::new(),
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

    // ASSUMES AN OPEN CURLY HAS JUST BEEN PASSED
    while x.index < x.tokens.len() {
        match x.current_token().to_owned() {
            Token::CloseCurly => {
                x.index += 1;
                break;
            }

            Token::Comma => {
                if next_item {
                    return Err(CompileError {
                        msg: "Expected a collection item after the comma".to_string(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                next_item = true;
                x.index += 1;
            }

            Token::Newline => {
                x.index += 1;
            }

            Token::Variable(name, is_public) => {
                if !next_item {
                    return Err(CompileError {
                        msg: "Expected a comma between items in this collection".to_string(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                let new_var = create_new_var_or_ref(
                    x,
                    name.to_owned(),
                    variable_declarations,
                    is_public,
                    ast,
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
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                next_item = false;

                let mut data_type = if required_args.is_empty() {
                    DataType::Inferred(false)
                } else if required_args.len() < items.len() {
                    return Err(CompileError {
                        msg: "Too many arguments provided to struct".to_string(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                } else {
                    required_args[items.len()].data_type.to_owned()
                };

                let arg_value =
                    create_expression(x, true, ast, &mut data_type, false, variable_declarations)?;

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
