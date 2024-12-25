use super::{
    ast_nodes::{AstNode, Arg},
    build_ast::new_ast,
    expressions::parse_expression::create_expression,
};
use crate::{bs_types::DataType, CompileError, Token};
use crate::parsers::ast_nodes::Node;

pub fn create_function(
    name: String,
    tokens: &Vec<Token>,
    i: &mut usize,
    is_exported: bool,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
) -> Result<(AstNode, Vec<Arg>, Vec<Arg>), CompileError> {
    /*
        funcName fn(arg type, arg2 type = default_value) -> returnType:
            -- Function body
        end

        No return value

        func fn():
            -- Function body
        end
    */
    let start_line_number = token_line_numbers[*i];

    // get args (tokens should currently be at the open parenthesis)
    let arg_refs = create_args(tokens, i, ast, token_line_numbers, variable_declarations)?;

    *i += 1;

    // Return type is optional (can not return anything)
    let return_args: Vec<Arg> = match &tokens[*i] {
        Token::Arrow => {
            *i += 1;
            parse_return_type(tokens, i, token_line_numbers)?
        }
        _ => Vec::new()
    };

    // Should now be at the colon
    if &tokens[*i] != &Token::Colon {
        return Err(CompileError {msg: "Expected ':' to open function scope".to_string(), line_number: token_line_numbers[*i]});
    }

    *i += 1;

    variable_declarations.push(Arg {
        name: name.to_owned(),
        data_type: DataType::Function(arg_refs.clone(), return_args.clone()),
        value: AstNode::Empty(token_line_numbers[*i]),
    });

    // The function ends with the 'end' keyword
    let function_body = new_ast(
        tokens.to_vec(),
        i,
        token_line_numbers,
        &mut arg_refs.clone(),
        &return_args,
        false,
    )?
    .0;

    Ok((
        AstNode::Function(name, arg_refs.clone(), function_body, is_exported, return_args.to_owned(), start_line_number),
        arg_refs,
        return_args
    ))
}

pub fn create_args(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &Vec<Arg>,
) -> Result<Vec<Arg>, CompileError> {
    let mut args = Vec::<Arg>::new();

    // Check if there are arguments
    let mut open_parenthesis = 0;
    let mut next_in_list: bool = true;

    while *i < tokens.len() {
        match &tokens[*i] {
            Token::OpenParenthesis => {
                open_parenthesis += 1;
            }
            Token::CloseParenthesis => {
                open_parenthesis -= 1;
                if open_parenthesis < 1 {
                    break;
                }
            }
            Token::Variable(arg_name) => {
                if !next_in_list {
                    return Err(
                        CompileError {
                            msg: "Should have a comma to separate arguments".to_string(),
                            line_number: token_line_numbers[*i].to_owned(),
                        }
                    );
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
                        return Err(
                            CompileError {
                                msg: "Function arguments must have unique names".to_string(),
                                line_number: token_line_numbers[*i].to_owned(),
                            }
                        );
                    }
                }

                // Check if there is a type keyword
                *i += 1;

                let mut data_type = match &tokens[*i] {
                    Token::TypeKeyword(data_type) => data_type.to_owned(),
                    _ => {
                        return Err(CompileError {
                            msg: "Expected type keyword after argument name".to_string(),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                };

                // Check if there is a default value
                let mut default_value: AstNode = AstNode::Empty(token_line_numbers[*i]);
                if match &tokens[*i + 1] {
                    Token::Assign => true,
                    _ => false,
                } {
                    *i += 2;
                    // Function args are similar to a tuple,
                    // So create expression is told it's a tuple inside brackets
                    // So it only parses up to a comma or closing parenthesis
                    default_value = create_expression(
                        tokens,
                        i,
                        true,
                        ast,
                        &mut data_type,
                        false,
                        &mut variable_declarations.to_owned(),
                        token_line_numbers,
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
                    msg: "Invalid syntax for function arguments".to_string(),
                    line_number: token_line_numbers[*i].to_owned(),
                });
            }
        }

        *i += 1;
    }

    if open_parenthesis != 0 {
        return Err(CompileError {
            msg: "Wrong number of parenthesis used when declaring function arguments".to_string(),
            line_number: token_line_numbers[*i].to_owned(),
        });
    }

    Ok(args)
}

fn parse_return_type(tokens: &Vec<Token>, i: &mut usize, token_line_numbers: &Vec<u32>) -> Result<Vec<Arg>, CompileError> {
    let mut return_type = Vec::<Arg>::new();

    // Check if there is a return type
    let mut open_parenthesis = 0;
    let mut next_in_list: bool = true;
    while tokens[*i] != Token::Colon {
        match &tokens[*i] {
            Token::OpenParenthesis => {
                open_parenthesis += 1;
                *i += 1;
            }
            Token::CloseParenthesis => {
                open_parenthesis -= 1;
                *i += 1;
            }
            Token::TypeKeyword(type_keyword) => {
                if next_in_list {
                    return_type.push(Arg {
                        name: "".to_string(),
                        data_type: type_keyword.to_owned(),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    });
                    next_in_list = false;
                    *i += 1;
                } else {
                    return Err(CompileError {
                        msg: "Should have a comma to separate return types".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    });
                }
            }
            Token::Comma => {
                next_in_list = true;
                *i += 1;
            }
            _ => {
                return Err(
                    CompileError {
                        msg: "Invalid syntax for return type".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
                    }
                );
            }
        }
    }

    if open_parenthesis != 0 {
        return Err(CompileError {
            msg: "Wrong number of parenthesis used when declaring return type".to_string(),
            line_number: token_line_numbers[*i].to_owned(),
        });
    }

    Ok(return_type)
}

// For Function Calls
// Unpacks references into their values and returns them
// Give back list of args for a function call in the correct order
// Replace names with their correct index order
// Makes sure they are the correct type
// Return a CompileError if anything is incorrect

// node = argument passed in
// args = list of arguments needed (with possible default values)
pub fn create_func_call_args(node: &AstNode, args: &Vec<Arg>, line_number: &u32) -> Result<Vec<AstNode>, CompileError> {
    // Create a vec of the required args values (arg.value)
    let mut sorted_args: Vec<AstNode> = args
        .iter()
        .map(|arg| arg.value.to_owned())
        .collect();

    let mut args_provided = 0;
    let mut index = 0;

    match node {
        // Single argument passed in
        // Can make naming it a syntax error or ignored for now
        AstNode::Literal(_, line_number) | AstNode::ConstReference(_, _, line_number) => {
            // Make sure there is at least one argument
            if args.len() == 0 {
                return Err(CompileError {
                    msg: "No arguments should be provided to this function".to_string(),
                    line_number: line_number.to_owned(),
                });
            }

            // Make sure the arg is the correct type
            if args[0].data_type != node.get_type() {
                return Err(CompileError {
                    msg: format!("Expected type {:?}, got type {:?}", args[0].data_type, node.get_type()),
                    line_number: line_number.to_owned(),
                });
            }

            args_provided += 1;
            sorted_args[0] = node.to_owned();
        }

        // Tuples of one element SHOULD be impossible to create
        AstNode::Tuple(values, line_number) => {
            // Make sure there is at least one argument
            if args.len() == 0 {
                return Err(CompileError {
                    msg: "No arguments should be provided to this function".to_string(),
                    line_number: line_number.to_owned(),
                });
            }

            // Look through all the arguments provided
            // If not named, check their index against the args list index
            // If named, move their index to the index of the arg with the same name in the args list
            for value in values {
                // Check if the arg is named
                if value.name.is_empty() {
                    // Make sure the unnamed arg index has a corresponding arg in the args list
                    if index >= args.len() {
                        return Err(CompileError {
                            msg: format!("Missing {} arguments from function call", args.len() - index),
                            line_number: line_number.to_owned(),
                        });
                    }

                    // Make sure the unnamed arg is the correct type for that index
                    if args[index].data_type != value.data_type {
                        return Err(CompileError {
                            msg: format!("Expected type {:?}, got type {:?}", args[index].data_type, value.data_type),
                            line_number: line_number.to_owned(),
                        });
                    }

                    // This has filled one of the 'unnamed' indexes so we can increment
                    // Named arguments don't increment the index
                    index += 1;
                    continue;
                }

                // TODO - Find the index of the arg with the same name
/*                let mut arg_index = 0;
                for arg in args {
                    if arg.name == arg.name {
                        break;
                    }
                    arg_index += 1;
                }
*/
            }
        }

        // Can skip to checking if remaining args are optional
        AstNode::Empty(_) => {}

        _ => return Err(CompileError {
            msg: "Invalid argument type".to_string(),
            line_number: line_number.to_owned(),
        }),
    }

    if args_provided < args.len() {
        // Check if other args have default values (can be added to final list)
        // If not, return an error
        while index < args.len() {
            sorted_args[index] = args[index].value.to_owned();
            index += 1;
        }
    }

    Ok(sorted_args)
}