use super::{
    ast_nodes::{Arg, AstNode},
};
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::TokenContext;
// use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::parsers::expressions::parse_expression::{create_multiple_expressions, get_accessed_args, create_args_from_types};
use crate::parsers::util::{find_first_missing, sort_unnamed_args_last};
use crate::parsers::variables::new_arg;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};

// Arg names and types are required
// Can have default values
pub fn create_block_signature(
    x: &mut TokenContext,
    pure: &mut bool,
    variable_declarations: &[Arg],
) -> Result<(Vec<Arg>, Vec<DataType>), CompileError> {
    let args = create_arg_constructor(x, variable_declarations, pure)?;

    match x.current_token() {
        Token::Arrow => {
            x.advance();
        }

        // Function does not return anything
        Token::Colon => {
            x.advance();
            return Ok((args, Vec::new()));
        }

        _ => {
            return Err(CompileError {
                msg: "Expected an arrow operator or colon after function arguments".to_string(),
                start_pos: x.token_start_position(),
                end_pos: x.token_start_position(),
                error_type: ErrorType::Syntax,
            });
        }
    }

    // Parse return types
    let mut return_types = Vec::new();
    let mut next_in_list: bool = true;
    while x.index < x.length {
        match x.current_token() {
            Token::DatatypeLiteral(data_type) => {
                if !next_in_list {
                    return Err(CompileError {
                        msg: "Should have a comma to separate return types".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: TokenPosition {
                            line_number: x.token_start_position().line_number,
                            char_column: x.token_start_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                return_types.push(data_type.to_owned());
                x.advance();
            }

            Token::Variable(name, ..) => {
                if !next_in_list {
                    return Err(CompileError {
                        msg: "Should have a comma to separate return types".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: TokenPosition {
                            line_number: x.token_start_position().line_number,
                            char_column: x.token_start_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                return_types.push(DataType::Pointer(name.to_owned()));
                x.advance();
            }

            Token::Colon => {
                x.advance();
                return Ok((args, return_types));
            }

            Token::Comma => {
                if next_in_list {
                    return Err(CompileError {
                        msg: "Should only have 1 comma separating return types".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: TokenPosition {
                            line_number: x.token_start_position().line_number,
                            char_column: x.token_start_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                x.advance();
                next_in_list = true;
            }

            _ => {
                return Err(CompileError {
                    msg: "Expected a type keyword after the arrow operator".to_string(),
                    start_pos: x.token_start_position(),
                    end_pos: TokenPosition {
                        line_number: x.token_start_position().line_number,
                        char_column: x.token_start_position().char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    Err(CompileError {
        msg: "Expected a colon after the type definitions".to_string(),
        start_pos: x.token_start_position(),
        end_pos: TokenPosition {
            line_number: x.token_start_position().line_number,
            char_column: x.token_start_position().char_column + 1,
        },
        error_type: ErrorType::Syntax,
    })
}

// For Function Calls or new instances of a predefined struct type (basically like a struct)
// Unpacks references into their values and returns them
// Give back list of args for a function call in the correct order
// Replace names with their correct index order
// Makes sure they are the correct type
// TODO: check if any of this actually works
pub fn _create_func_call_args(
    args_passed_in: &[Arg],
    args_required: &[Arg],
    token_position: &TokenPosition,
) -> Result<Vec<Expr>, CompileError> {
    // Create a vec of the required args values (arg.value)
    let mut indexes_filled: Vec<usize> = Vec::with_capacity(args_required.len());
    let mut sorted_values: Vec<Expr> = args_required
        .iter()
        .map(|arg| arg.expr.to_owned())
        .collect();

    if args_passed_in.is_empty() || args_passed_in[0].expr == Expr::None {
        for arg in args_required {
            // Make sure there are no required arguments left
            if arg.expr != Expr::None {
                return Err(CompileError {
                    msg: format!(
                        "Missed at least one required argument for struct or function call: {} (type: {:?})",
                        arg.name, arg.data_type
                    ),
                    start_pos: token_position.to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_position.line_number,
                        char_column: token_position.char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }

        // Since all sorted args have values, they can just be passed back
        // As the default values
        return Ok(sorted_values);
    }

    // Should just be a literal value (one arg passed in)
    // Probably won't allow names or anything here so can just type check it with the sorted args
    // And return the value
    if sorted_values.is_empty() {
        return Err(CompileError {
            msg: format!(
                "Function call does not accept any arguments. Value passed in: {:?}",
                args_passed_in
            ),
            start_pos: token_position.to_owned(),
            end_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    // First, we want to make sure we fill all the named arguments first.
    // Then we can fill in the unnamed arguments
    // To do this, we can just sort the args passed in
    let args_in_sorted = sort_unnamed_args_last(args_passed_in);

    'outer: for mut arg in args_in_sorted {
        // If the argument is unnamed, find the smallest index that hasn't been filled
        if arg.name.is_empty() {
            let min_available = find_first_missing(&indexes_filled);

            // Make sure the type is correct
            if args_required[min_available]
                .data_type
                .is_valid_type(&mut arg.data_type)
            {
                return Err(CompileError {
                    msg: format!(
                        "Argument '{}' is of type {:?}, but used in an argument of type: {:?}",
                        arg.name, arg.data_type, args_required[min_available].data_type
                    ),
                    start_pos: token_position.to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_position.line_number,
                        char_column: token_position.char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            if sorted_values.len() <= min_available {
                return Err(CompileError {
                    msg: format!(
                        "Too many arguments passed into function call. Expected: {:?}, Passed in: {:?}",
                        args_required, args_passed_in
                    ),
                    start_pos: token_position.to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_position.line_number,
                        char_column: token_position.char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            sorted_values[min_available] = arg.expr.to_owned();
            indexes_filled.push(min_available);
            continue;
        }

        for (j, arg_required) in args_required.iter().enumerate() {
            if arg_required.name == arg.name {
                sorted_values[j] = arg.expr.to_owned();
                indexes_filled.push(j);
                continue 'outer;
            }
        }

        return Err(CompileError {
            msg: format!(
                "Argument '{}' not found in function call. Expected: {:?}, Passed in: {:?}",
                arg.name, args_required, arg
            ),
            start_pos: token_position.to_owned(),
            end_pos: TokenPosition {
                line_number: token_position.line_number,
                char_column: token_position.char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    // Check if the sorted args contains any None values
    // If it does, there are missing arguments (error)
    for (i, value) in sorted_values.iter().enumerate() {
        if value == &Expr::None {
            return Err(CompileError {
                msg: format!(
                    "Required argument missing from struct/function call: {:?} (type: {:?})",
                    args_required[i].name, args_required[i].data_type
                ),
                start_pos: token_position.to_owned(),
                end_pos: TokenPosition {
                    line_number: token_position.line_number,
                    char_column: token_position.char_column + 1,
                },
                error_type: ErrorType::Syntax,
            });
        }
    }

    Ok(sorted_values)
}

// Built-in functions will do their own thing
pub fn parse_function_call(
    x: &mut TokenContext,
    name: &str,
    variable_declarations: &[Arg],
    argument_refs: &[Arg],
    returns: &[DataType],
) -> Result<AstNode, CompileError> {
    // Assumes starting at the first token after the name of the function call
    // let mut is_pure = true;

    // make sure there is an open parenthesis
    if x.current_token() != &Token::OpenParenthesis {
        return Err(CompileError {
            msg: format!(
                "Expected a parenthesis after function call. Found {:?} instead.",
                x.current_token()
            ),
            start_pos: x.token_start_position(),
            end_pos: TokenPosition {
                line_number: x.token_start_position().line_number,
                char_column: x.token_start_position().char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    x.advance();

    // Create expressions until hitting a closed parenthesis
    // TODO: named arguments
    let required_argument_types = argument_refs
        .iter()
        .map(|arg| arg.data_type.to_owned())
        .collect::<Vec<DataType>>();

    let expressions = if required_argument_types.is_empty() {
        // make sure there is a closing parenthesis
        if x.current_token() != &Token::CloseParenthesis {
            return Err(CompileError {
                msg: "This function doesn't accept any arguments. Close it right away with a closing parenthesis instead.".to_string(),
                start_pos: x.token_start_position(),
                end_pos: TokenPosition {
                    line_number: x.token_start_position().line_number,
                    char_column: x.token_start_position().char_column + 1,
                },
                error_type: ErrorType::Syntax,
            });
        }

        Vec::new()
    } else {
        create_multiple_expressions(x, &required_argument_types, variable_declarations)?
    };

    // Make sure there is a closing parenthesis
    if x.current_token() != &Token::CloseParenthesis {
        return Err(CompileError {
            msg: format!(
                "Missing a closing parenthesis at the end of the function call, found a '{:?}' instead",
                x.current_token()
            ),
            start_pos: x.token_start_position(),
            end_pos: TokenPosition {
                line_number: x.token_start_position().line_number,
                char_column: x.token_start_position().char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    x.advance();

    // TODO
    // Makes sure the call value is correct for the function call
    // If so, the function call args are sorted into their correct order (if some are named or optional)
    // Once this is always working then default args can be removed from the JS output
    // let args = create_func_call_args(&expressions, argument_refs, &x.current_position())?;

    // look for which arguments are being accessed from the function call
    let return_type = create_args_from_types(returns);
    let accessed_args = get_accessed_args(x, name, &DataType::Object(return_type), &mut Vec::new())?;

    // Inline this function call if it's pure and the function call is pure
    // if is_pure && call_value.is_pure() {
    //     let original_function = variable_declarations
    //         .iter()
    //         .find(|a| a.name == *name)
    //         .unwrap();
    //     return inline_function_call(&args, &accessed_args, &original_function.value);
    // }

    Ok(AstNode::FunctionCall(
        name.to_owned(),
        expressions,
        returns.to_owned(),
        accessed_args,
        x.token_start_position(),
    ))
}

fn create_arg_constructor(
    x: &mut TokenContext,
    variable_declarations: &[Arg],
    pure: &mut bool,
) -> Result<Vec<Arg>, CompileError> {
    let mut args = Vec::<Arg>::new();
    let mut next_in_list: bool = true;

    if x.current_token() != &Token::ArgConstructor {
        return Err(CompileError {
            msg: "Expected a | after the function name".to_string(),
            start_pos: x.token_start_position(),
            end_pos: TokenPosition {
                line_number: x.token_start_position().line_number,
                char_column: x.token_start_position().char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    x.advance();

    while x.index < x.tokens.len() {
        match x.current_token().to_owned() {
            Token::ArgConstructor => {
                x.advance();
                return Ok(args);
            }

            Token::Variable(arg_name, ..) => {
                if !next_in_list {
                    return Err(CompileError {
                        msg: "Should have a comma to separate arguments".to_string(),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column
                                + arg_name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                // Create a new variable
                let argument = new_arg(x, &arg_name, variable_declarations)?;

                if argument.data_type.is_mutable() {
                    *pure = false;
                }

                args.push(argument);

                next_in_list = false;
            }

            Token::Comma => {
                x.advance();
                next_in_list = true;
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Unexpected token used in function arguments: {:?}",
                        x.current_token()
                    ),
                    start_pos: x.token_positions[x.index].to_owned(),
                    end_pos: TokenPosition {
                        line_number: x.token_positions[x.index].line_number,
                        char_column: x.token_positions[x.index].char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    Ok(args)
}
