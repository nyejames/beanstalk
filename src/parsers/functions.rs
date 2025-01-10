use super::{
    ast_nodes::{Arg, AstNode},
    build_ast::new_ast,
    expressions::parse_expression::create_expression,
};
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::expressions::parse_expression::get_accessed_args;
use crate::{bs_types::DataType, CompileError, Token};

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
        _ => Vec::new(),
    };

    // Should now be at the colon
    if &tokens[*i] != &Token::Colon {
        return Err(CompileError {
            msg: "Expected ':' to open function scope".to_string(),
            line_number: token_line_numbers[*i],
        });
    }

    *i += 1;

    variable_declarations.push(Arg {
        name: name.to_owned(),
        data_type: DataType::Function(arg_refs.clone(), return_args.clone()),
        value: Value::None,
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
        AstNode::Function(
            name,
            arg_refs.clone(),
            function_body,
            is_exported,
            return_args.to_owned(),
            start_line_number,
        ),
        arg_refs,
        return_args,
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
                    return Err(CompileError {
                        msg: "Should have a comma to separate arguments".to_string(),
                        line_number: token_line_numbers[*i].to_owned(),
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
                            line_number: token_line_numbers[*i].to_owned(),
                        });
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
                let mut default_value: Value = Value::None;
                if match &tokens[*i + 1] {
                    Token::Assign => true,
                    _ => false,
                } {
                    *i += 2;
                    // Function args are similar to a struct,
                    // So create expression is told it's a struct inside brackets
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
                    value: default_value.get_value(),
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

fn parse_return_type(
    tokens: &Vec<Token>,
    i: &mut usize,
    token_line_numbers: &Vec<u32>,
) -> Result<Vec<Arg>, CompileError> {
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
                        value: Value::None,
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
                return Err(CompileError {
                    msg: "Invalid syntax for return type".to_string(),
                    line_number: token_line_numbers[*i].to_owned(),
                });
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

// For Function Calls or new instances of a predefined struct type (basically like a struct)
// Unpacks references into their values and returns them
// Give back list of args for a function call in the correct order
// Replace names with their correct index order
// Makes sure they are the correct type
// Return a CompileError if anything is incorrect

// TODO: check if any of this actually works
pub fn create_structure_args(
    value_passed_in: &Value,
    args_required: &Vec<Arg>,
    line_number: &u32,
) -> Result<Vec<Value>, CompileError> {
    // Create a vec of the required args values (arg.vaalue)
    let mut sorted_values: Vec<Value> = args_required
        .iter()
        .map(|arg| arg.value.to_owned())
        .collect();

    // If args required contains a struct of one datatype,
    // Then any number of arguments of that datatype can be passed in
    match value_passed_in {
        // These are the only way values can be named
        Value::Structure(inner_args) => {
            let inner_args_length = inner_args.len();
            if inner_args_length == 0 {
                // This is equivalent to None
                // This should probably be throwing an error here as an empty struct should not be possible to pass as an arg
                return Err( CompileError {
                    msg: "Compiler Problem: Empty struct passed into a struct arg: this should not be possible. This is equivalent to None.".to_string(),
                    line_number: line_number.to_owned()
                });
            }

            // This should also not be possible, as a struct of a single item will automatically become a type of that item
            if inner_args_length == 1 {
                return Err( CompileError {
                    msg: "Compiler Problem: A spooky struct of one item has somehow got this far is the compile stage. This should just be the item itself".to_string(),
                    line_number: line_number.to_owned()
                });
            }

            // TODO: if the accepted argument is a struct of only one item, any number of values of that argument's type can be passed in
            // TODO: Match all the args in the struct to the required args
        }

        // This should only be in the case of function calls that don't need any arguments passed in
        Value::None => {
            for arg in args_required {
                // Make sure there are no required arguments left
                if arg.value != Value::None {
                    return Err(CompileError {
                        msg: format!("Missed at least one required arguments for struct or function call: {} (type: {:?})", arg.name, arg.data_type),
                        line_number: line_number.to_owned()
                    });
                }
            }

            return Ok(Vec::new());
        }

        // Should just be a literal value (one arg passed in)
        // Probably won't allow names or anything here so can just type check it with the sorted args
        // And return the value
        _ => {
            if sorted_values.is_empty() {
                return Err(CompileError {
                    msg: format!("This struct or function call does not accept any arguments. Value passed in: {:?}", value_passed_in),
                    line_number: line_number.to_owned()
                });
            }

            // Fill in the first required value (first none) in the required args
            // Then make sure there are no more required args
            let replacement_value = value_passed_in.to_owned();
            for (i, value) in sorted_values.iter().enumerate() {
                match value {
                    Value::None => {
                        // Make sure this is the correct type
                        let value_being_replaced = &mut sorted_values[i];
                        let value_being_replaced_type = value_being_replaced.get_type();
                        let passed_value_type = value_passed_in.get_type();

                        if value_being_replaced_type != passed_value_type {
                            return Err(CompileError {
                                msg: format!("Type error: argument of type {:?} was passed into an argument of type: {:?}", passed_value_type, value_being_replaced_type),
                                line_number: line_number.to_owned()
                            });
                        }

                        *value_being_replaced = replacement_value;
                        return Ok(sorted_values);
                    }
                    _ => {}
                }
            }

            // If there are no None values, this automatically becomes the first one
            sorted_values[0] = replacement_value;
            return Ok(sorted_values);
        }
    }

    // Check if the sorted args contains any None values
    // If it does, there are missing arguments (error)
    for (i, value) in sorted_values.iter().enumerate() {
        match value {
            Value::None => {
                return Err(CompileError {
                    msg: format!(
                        "Required argument missing from struct/function call: {:?} (type: {:?})",
                        args_required[i].name, args_required[i].data_type
                    ),
                    line_number: line_number.to_owned(),
                })
            }
            _ => {}
        }
    }

    Ok(sorted_values)
}

// Built-in functions will do their own thing
pub fn parse_function_call(
    name: &String,
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
    argument_refs: &Vec<Arg>,
    return_args: &Vec<Arg>,
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
    let call_value = if tokens.get(*i) == Some(&Token::OpenParenthesis) {
        *i += 1;

        // Parse argument(s) passed into the function
        create_expression(
            tokens,
            i,
            false,
            ast,
            &mut DataType::Structure(argument_refs.to_owned()),
            false,
            variable_declarations,
            token_line_numbers,
        )?
    } else {
        return Err(CompileError {
            msg: "Expected a parenthesis after function name".to_string(),
            line_number: token_line_numbers[*i].to_owned(),
        });
    };

    // Makes sure the call value is correct for the function call
    // If so, the function call args are sorted into their correct order (if some are named or optional)
    let args = create_structure_args(&call_value, &argument_refs, &token_line_numbers[*i])?;

    // look for which arguments are being accessed from the function call
    let accessed_args = get_accessed_args(
        &name,
        tokens,
        &mut *i,
        &DataType::Structure(return_args.to_owned()),
        token_line_numbers,
        &mut Vec::new(),
    )?;

    Ok(AstNode::FunctionCall(
        name.to_owned(),
        args,
        return_args.to_owned(),
        accessed_args,
        token_line_numbers[*i],
    ))
}
