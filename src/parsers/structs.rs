#[allow(unused_imports)]
use colour::grey_ln;

use super::ast_nodes::Arg;
use crate::bs_types::DataType;
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::expressions::parse_expression::create_expression;
use crate::parsers::variables::new_arg;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token};

pub fn create_args(
    x: &mut TokenContext,
    initial_value: Expr,
    required_args: &[Arg],
    variable_declarations: &[Arg],
) -> Result<Vec<Arg>, CompileError> {
    let mut item_args = required_args.to_owned();

    let mut items: Vec<Arg> = match initial_value {
        Expr::None => Vec::new(),
        _ => {
            vec![Arg {
                name: "0".to_string(),

                // TODO: Should items be able to be declared as mutable here?
                // check for mutable token before?
                data_type: initial_value.get_type(false),
                expr: initial_value,
            }]
        }
    };

    let mut next_item: bool = true;
    let mut item_name: String = "0".to_string();

    // ASSUMES A '(' HAS JUST BEEN PASSED
    while x.index < x.tokens.len() {
        match x.current_token().to_owned() {
            Token::CloseParenthesis => {
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
                x.advance();
            }

            Token::Newline => {
                x.advance();
            }

            Token::Variable(ref name, ..) => {
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

                let item_arg = new_arg(x, name, variable_declarations)?;

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
                    create_expression(x, &mut data_type, false, variable_declarations, &[])?;

                // Get the arg of this struct item
                let item_arg = match item_args.get(items.len()) {
                    Some(arg) => arg.to_owned(),
                    None => Arg {
                        name: item_name,
                        data_type: data_type.to_owned(),
                        expr: arg_value,
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
