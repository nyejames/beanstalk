use super::{
    ast_nodes::{Arg, AstNode},
    build_ast::new_ast,
    expressions::parse_expression::create_expression,
};
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::parsers::util::{find_first_missing, sort_unnamed_args_last};
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use std::path::PathBuf;

pub fn create_function(
    x: &mut TokenContext,
    name: String,
    is_exported: bool,
    ast: &[AstNode],

    // Functions don't capture the scope of the module
    // This is just for parsing the arguments
    declarations: &[Arg],
) -> Result<(AstNode, Vec<Arg>, DataType), CompileError> {
    /*
        funcName = fn(arg ~Type, arg2 Type = default_value) -> Type:
            -- Function body
            return value
        zz

        No return value

        func fn():
            -- Function body
        zz
    */

    let mut pure = true;

    // get args (tokens should currently be at the open parenthesis)
    let arg_refs = create_args(x, ast, declarations, &mut pure)?;

    x.index += 1;

    // Return type is optional (can not return anything)
    let mut return_type: DataType = parse_return_type(x)?;

    x.index += 1;

    // Should now be at the colon
    if x.current_token() != &Token::Colon {
        return Err(CompileError {
            msg: "Expected ':' to open function scope".to_string(),
            start_pos: x.token_positions[x.index].to_owned(),
            end_pos: TokenPosition {
                line_number: x.token_positions[x.index].line_number,
                char_column: x.token_positions[x.index].char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    x.index += 1;

    let function_body = new_ast(x, &arg_refs, &mut return_type, &PathBuf::new(), &mut pure)?;

    Ok((
        AstNode::Function(
            name,
            arg_refs.clone(),
            function_body,
            is_exported,
            return_type.to_owned(),
            x.current_position(),
            pure.to_owned(),
        ),
        arg_refs,
        return_type,
    ))
}

// Arg names and types are required
// Can have default values
pub fn create_args(
    x: &mut TokenContext,
    ast: &[AstNode],
    variable_declarations: &[Arg],
    pure: &mut bool,
) -> Result<Vec<Arg>, CompileError> {
    let mut args = Vec::<Arg>::new();

    // Check if there are arguments
    let mut open_parenthesis = 0;
    let mut next_in_list: bool = true;

    while x.index < x.tokens.len() {
        let token = x.tokens[x.index].to_owned();
        match token {
            Token::OpenParenthesis => {
                open_parenthesis += 1;
            }
            Token::CloseParenthesis => {
                open_parenthesis -= 1;
                if open_parenthesis < 1 {
                    break;
                }
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

                // Parse the argument
                /*
                    Arguments follow this syntax:

                    variables
                    arg_name type = default_value

                    no default value
                    arg_name type
                */

                // Make sure function arguments are not redeclared variables
                for var in variable_declarations {
                    if var.name == *arg_name {
                        return Err(CompileError {
                            msg: "Function arguments must have unique names".to_string(),
                            start_pos: x.token_positions[x.index].to_owned(),
                            end_pos: TokenPosition {
                                line_number: x.token_positions[x.index].line_number,
                                char_column: x.token_positions[x.index].char_column
                                    + arg_name.len() as i32,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }
                }

                // Check if there is a type keyword
                x.index += 1;

                let mut data_type = match &x.tokens[x.index] {
                    Token::DatatypeLiteral(data_type) => data_type.to_owned(),
                    _ => {
                        return Err(CompileError {
                            msg: "Expected type keyword after argument name".to_string(),
                            start_pos: x.token_positions[x.index].to_owned(),
                            end_pos: TokenPosition {
                                line_number: x.token_positions[x.index].line_number,
                                char_column: x.token_positions[x.index].char_column
                                    + arg_name.len() as i32,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }
                };

                if data_type.is_mutable() {
                    *pure = false;
                }

                // Check if there is a default value
                let mut default_value: Expr = Expr::None;
                if matches!(x.tokens[x.index + 1], Token::Assign) {
                    x.index += 2;
                    // Function args are similar to a struct,
                    // So create expression is told it's a struct inside brackets
                    // So it only parses up to a comma or closing parenthesis
                    default_value = create_expression(
                        x,
                        true,
                        ast,
                        &mut data_type,
                        false,
                        &mut variable_declarations.to_owned(),
                    )?;
                }

                args.push(Arg {
                    name: arg_name.to_owned(),
                    data_type: data_type.to_owned(),
                    value: default_value,
                });

                next_in_list = false;
            }

            Token::Comma => {
                next_in_list = true;
            }

            _ => {
                return Err(CompileError {
                    msg: format!("Unexpected token used in function arguments: {:?}", token),
                    start_pos: x.token_positions[x.index].to_owned(),
                    end_pos: TokenPosition {
                        line_number: x.token_positions[x.index].line_number,
                        char_column: x.token_positions[x.index].char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }

        x.index += 1;
    }

    if open_parenthesis != 0 {
        return Err(CompileError {
            msg: "Wrong number of parenthesis used when declaring function arguments".to_string(),
            start_pos: x.token_positions[x.index].to_owned(),
            end_pos: TokenPosition {
                line_number: x.token_positions[x.index].line_number,
                char_column: x.token_positions[x.index].char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    }

    Ok(args)
}

fn parse_return_type(x: &mut TokenContext) -> Result<DataType, CompileError> {
    match x.current_token() {
        Token::Arrow => {
            x.index += 1;
        }
        _ => return Ok(DataType::None),
    };

    match x.current_token() {
        Token::DatatypeLiteral(data_type) => Ok(data_type.to_owned()),
        _ => Err(CompileError {
            msg: "Expected a type keyword after the arrow operator".to_string(),
            start_pos: x.current_position(),
            end_pos: TokenPosition {
                line_number: x.current_position().line_number,
                char_column: x.current_position().char_column + 1,
            },
            error_type: ErrorType::Syntax,
        }),
    }
}

// fn parse_return_type(x: &mut TokenContext) -> Result<Vec<Arg>, CompileError> {
//     let mut return_type = Vec::<Arg>::new();
//
//     // Check if there is a return type
//     let mut open_parenthesis = 0;
//     let mut next_in_list: bool = true;
//     while x.tokens[x.index] != Token::Colon {
//         match &x.tokens[x.index] {
//             Token::OpenParenthesis => {
//                 open_parenthesis += 1;
//                 x.index += 1;
//             }
//             Token::CloseParenthesis => {
//                 open_parenthesis -= 1;
//                 x.index += 1;
//             }
//             Token::TypeKeyword(type_keyword) => {
//                 if next_in_list {
//                     return_type.push(Arg {
//                         name: "".to_string(),
//                         data_type: type_keyword.to_owned(),
//                         value: Value::None,
//                     });
//                     next_in_list = false;
//                     x.index += 1;
//                 } else {
//                     return Err(CompileError {
//                         msg: "Should have a comma to separate return types".to_string(),
//                         start_pos: x.token_positions[x.index].to_owned(),
//                         end_pos: TokenPosition {
//                             line_number: x.token_positions[x.index].line_number,
//                             char_column: x.token_positions[x.index].char_column + 1,
//                         },
//                         error_type: ErrorType::Syntax,
//                     });
//                 }
//             }
//             Token::Comma => {
//                 next_in_list = true;
//                 x.index += 1;
//             }
//             _ => {
//                 return Err(CompileError {
//                     msg: "Invalid syntax for return type".to_string(),
//                     start_pos: x.token_positions[x.index].to_owned(),
//                     end_pos: TokenPosition {
//                         line_number: x.token_positions[x.index].line_number,
//                         char_column: x.token_positions[x.index].char_column + 1,
//                     },
//                     error_type: ErrorType::Syntax,
//                 });
//             }
//         }
//     }
//
//     if open_parenthesis != 0 {
//         return Err(CompileError {
//             msg: "Wrong number of parenthesis used when declaring return type".to_string(),
//             start_pos: x.token_positions[x.index].to_owned(),
//             end_pos: TokenPosition {
//                 line_number: x.token_positions[x.index].line_number,
//                 char_column: x.token_positions[x.index].char_column + 1,
//             },
//             error_type: ErrorType::Syntax,
//         });
//     }
//
//     Ok(return_type)
// }

// For Function Calls or new instances of a predefined struct type (basically like a struct)
// Unpacks references into their values and returns them
// Give back list of args for a function call in the correct order
// Replace names with their correct index order
// Makes sure they are the correct type
// TODO: check if any of this actually works
pub fn create_func_call_args(
    value_passed_in: &Expr,
    args_required: &[Arg],
    token_position: &TokenPosition,
) -> Result<Vec<Expr>, CompileError> {
    // Create a vec of the required args values (arg.value)
    let mut indexes_filled: Vec<usize> = Vec::with_capacity(args_required.len());
    let mut sorted_values: Vec<Expr> = args_required
        .iter()
        .map(|arg| arg.value.to_owned())
        .collect();

    let args_passed_in = match value_passed_in {
        Expr::StructLiteral(args) => args,
        _ => &Vec::from([Arg {
            name: "".to_string(),
            data_type: value_passed_in.get_type(),
            value: value_passed_in.to_owned(),
        }]),
    };

    if args_passed_in.is_empty() || args_passed_in[0].value == Expr::None {
        for arg in args_required {
            // Make sure there are no required arguments left
            if arg.value != Expr::None {
                return Err(CompileError {
                    msg: format!(
                        "Missed at least one required arguments for struct or function call: {} (type: {:?})",
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

    // First we want to make sure we fill all the named arguments first
    // Then we can fill in the unnamed arguments
    // To do this we can just sort the args passed in
    let args_in_sorted = sort_unnamed_args_last(args_passed_in);

    'outer: for mut arg in args_in_sorted {
        // If argument is unnamed, find the smallest index that hasn't been filled
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

            sorted_values[min_available] = arg.value.to_owned();
            indexes_filled.push(min_available);
            continue;
        }

        for (j, arg_required) in args_required.iter().enumerate() {
            if arg_required.name == arg.name {
                sorted_values[j] = arg.value.to_owned();
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
    name: String,
    ast: &[AstNode],
    variable_declarations: &mut Vec<Arg>,
    argument_refs: &[Arg],
    return_type: &DataType,
    is_pure: bool,
) -> Result<AstNode, CompileError> {
    // Assumes starting at the first token after the name of the function call

    // Function calls MUST have parenthesis, or they are just a reference to a function
    // This is to prevent ambiguity if parenthesis are used for an unrelated expression after a function call in a scene head
    // The only way around this would be to enforce no space between function calls and the opening parenthesis (MAYBE MIGHT DO THIS, BUT PROBABLY NOT)
    // The reason for not doing this is-> func_call (1 + 1) looks really similar to func_call(1 + 1) which would be confusing.
    // By enforcing parenthesis, the compiler can be sure that the function call is not a reference to a function
    // So one small space could be a confusing error that will be hard for the compiler to understand what went wrong.
    // Functions will need to be wrapped in Lambdas if passing them as arguments

    // Make sure there are parenthesis
    let call_value = if x.tokens.get(x.index) == Some(&Token::OpenParenthesis) {
        x.index += 1;

        // Parse argument(s) passed into the function
        create_expression(
            x,
            false,
            ast,
            &mut DataType::Inferred(false),
            true,
            variable_declarations,
        )?
    } else {
        return Err(CompileError {
            msg: "Expected a parenthesis after function name".to_string(),
            start_pos: x.current_position(),
            end_pos: TokenPosition {
                line_number: x.current_position().line_number,
                char_column: x.current_position().char_column + 1,
            },
            error_type: ErrorType::Syntax,
        });
    };

    // Makes sure the call value is correct for the function call
    // If so, the function call args are sorted into their correct order (if some are named or optional)
    let args = create_func_call_args(&call_value, argument_refs, &x.current_position())?;

    // look for which arguments are being accessed from the function call
    let accessed_args = get_accessed_args(
        x,
        &name,
        &DataType::Structure(Vec::from([Arg {
            name: "".to_string(),
            data_type: return_type.to_owned(),
            value: Expr::None,
        }])),
        &mut Vec::new(),
    )?;

    // Inline this function call if it's pure and the function call is pure
    if is_pure && call_value.is_pure() {
        let original_function = variable_declarations
            .iter()
            .find(|a| a.name == *name)
            .unwrap();
        return inline_function_call(&args, &accessed_args, &original_function.value);
    }

    Ok(AstNode::FunctionCall(
        name.to_owned(),
        args,
        return_type.to_owned(),
        accessed_args,
        x.current_position(),
        is_pure,
    ))
}
