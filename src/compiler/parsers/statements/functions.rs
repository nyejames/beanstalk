use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

// use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind, VarVisibility};
use crate::{ast_log, return_syntax_error};

// Arg names and types are required
// Can have default values
pub fn create_function_signature(
    token_stream: &mut TokenContext,
    pure: &mut bool,
    context: &ScopeContext,
) -> Result<(Vec<Arg>, Vec<DataType>), CompileError> {
    let args = create_arg_constructor(token_stream, context, pure)?;

    match token_stream.current_token_kind() {
        TokenKind::Arrow => {
            token_stream.advance();
        }

        // Function does not return anything
        TokenKind::Colon => {
            token_stream.advance();
            return Ok((args, Vec::new()));
        }

        _ => {
            return_syntax_error!(
                token_stream.current_location(),
                "Expected an arrow operator or colon after function arguments",
            )
        }
    }

    // Parse return types
    let mut return_types = Vec::new();
    let mut next_in_list: bool = true;
    let mut mutable: bool = false;

    while token_stream.index < token_stream.length {
        match token_stream.current_token_kind() {
            TokenKind::Mutable => {
                if !next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should have a comma to separate return types",
                    )
                }

                token_stream.advance();
                mutable = true;
            }

            TokenKind::DatatypeLiteral(data_type) => {
                if !next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should have a comma to separate return types",
                    )
                }

                return_types.push(data_type.to_owned());
                token_stream.advance();
            }

            TokenKind::Symbol(name) => {
                if !next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should have a comma to separate return types",
                    )
                }

                return_types.push(DataType::UnknownReference(
                    name.to_owned(),
                    mutable.to_owned(),
                ));
                token_stream.advance();
            }

            TokenKind::Colon => {
                token_stream.advance();
                return Ok((args, return_types));
            }

            TokenKind::Comma => {
                if next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should not have a comma at the end of the return types",
                    )
                }

                token_stream.advance();
                next_in_list = true;
            }

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Expected a type keyword after the arrow operator",
                )
            }
        }
    }

    return_syntax_error!(
        token_stream.current_location(),
        "Expected a colon after the return types",
    )
}

// For Function Calls or new instances of a predefined struct type (basically like a struct)
// Unpacks references into their values and returns them
// Give back list of args for a function call in the correct order
// Replace names with their correct index order
// Makes sure they are the correct type
// TODO: check if any of this actually works
// pub fn create_func_call_args(
//     args_passed_in: &[Arg],
//     args_required: &[Arg],
//     token_position: &TextLocation,
// ) -> Result<Vec<ExpressionKind>, CompileError> {
//     // Create a vec of the required args values (arg.value)
//     let mut indexes_filled: Vec<usize> = Vec::with_capacity(args_required.len());
//     let mut sorted_values: Vec<ExpressionKind> = args_required
//         .iter()
//         .map(|arg| arg.value.to_owned())
//         .collect();
//
//     if args_passed_in.is_empty() || args_passed_in[0].value == ExpressionKind::None {
//         for arg in args_required {
//             // Make sure there are no required arguments left
//             if arg.value != ExpressionKind::None {
//                 return Err(CompileError {
//                     msg: format!(
//                         "Missed at least one required argument for struct or function call: {} (type: {:?})",
//                         arg.name, arg.data_type
//                     ),
//                     start_pos: token_position.to_owned(),
//                     end_pos: TextLocation {
//                         line_number: token_position.line_number,
//                         char_column: token_position.char_column + 1,
//                     },
//                     error_type: ErrorType::Syntax,
//                 });
//             }
//         }
//
//         // Since all sorted args have values, they can just be passed back
//         // As the default values
//         return Ok(sorted_values);
//     }
//
//     // Should just be a literal value (one arg passed in)
//     // Probably won't allow names or anything here so can just type check it with the sorted args
//     // And return the value
//     if sorted_values.is_empty() {
//         return Err(CompileError {
//             msg: format!(
//                 "Function call does not accept any arguments. Value passed in: {:?}",
//                 args_passed_in
//             ),
//             start_pos: token_position.to_owned(),
//             end_pos: TextLocation {
//                 line_number: token_position.line_number,
//                 char_column: token_position.char_column + 1,
//             },
//             error_type: ErrorType::Syntax,
//         });
//     }
//
//     // First, we want to make sure we fill all the named arguments first.
//     // Then we can fill in the unnamed arguments
//     // To do this, we can just sort the args passed in
//     let args_in_sorted = sort_unnamed_args_last(args_passed_in);
//
//     'outer: for mut arg in args_in_sorted {
//         // If the argument is unnamed, find the smallest index that hasn't been filled
//         if arg.name.is_empty() {
//             let min_available = find_first_missing(&indexes_filled);
//
//             // Make sure the type is correct
//             if args_required[min_available]
//                 .data_type
//                 .is_valid_type(&mut arg.data_type)
//             {
//                 return Err(CompileError {
//                     msg: format!(
//                         "Argument '{}' is of type {:?}, but used in an argument of type: {:?}",
//                         arg.name, arg.data_type, args_required[min_available].data_type
//                     ),
//                     start_pos: token_position.to_owned(),
//                     end_pos: TextLocation {
//                         line_number: token_position.line_number,
//                         char_column: token_position.char_column + 1,
//                     },
//                     error_type: ErrorType::Syntax,
//                 });
//             }
//
//             if sorted_values.len() <= min_available {
//                 return Err(CompileError {
//                     msg: format!(
//                         "Too many arguments passed into function call. Expected: {:?}, Passed in: {:?}",
//                         args_required, args_passed_in
//                     ),
//                     start_pos: token_position.to_owned(),
//                     end_pos: TextLocation {
//                         line_number: token_position.line_number,
//                         char_column: token_position.char_column + 1,
//                     },
//                     error_type: ErrorType::Syntax,
//                 });
//             }
//
//             sorted_values[min_available] = arg.value.to_owned();
//             indexes_filled.push(min_available);
//             continue;
//         }
//
//         for (j, arg_required) in args_required.iter().enumerate() {
//             if arg_required.name == arg.name {
//                 sorted_values[j] = arg.value.to_owned();
//                 indexes_filled.push(j);
//                 continue 'outer;
//             }
//         }
//
//         return Err(CompileError {
//             msg: format!(
//                 "Argument '{}' not found in function call. Expected: {:?}, Passed in: {:?}",
//                 arg.name, args_required, arg
//             ),
//             start_pos: token_position.to_owned(),
//             end_pos: TextLocation {
//                 line_number: token_position.line_number,
//                 char_column: token_position.char_column + 1,
//             },
//             error_type: ErrorType::Syntax,
//         });
//     }
//
//     // Check if the sorted args contains any None values
//     // If it does, there are missing arguments (error)
//     for (i, value) in sorted_values.iter().enumerate() {
//         if value == &ExpressionKind::None {
//             return Err(CompileError {
//                 msg: format!(
//                     "Required argument missing from struct/function call: {:?} (type: {:?})",
//                     args_required[i].name, args_required[i].data_type
//                 ),
//                 start_pos: token_position.to_owned(),
//                 end_pos: TextLocation {
//                     line_number: token_position.line_number,
//                     char_column: token_position.char_column + 1,
//                 },
//                 error_type: ErrorType::Syntax,
//             });
//         }
//     }
//
//     Ok(sorted_values)
// }

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut TokenContext,
    name: &str,
    context: &ScopeContext,
    required_arguments: &[Arg],
    returned_types: &[DataType],
) -> Result<AstNode, CompileError> {
    // Assumes starting at the first token after the name of the function call

    // Create expressions until hitting a closed parenthesis
    let expressions = create_function_call_arguments(token_stream, required_arguments, context)?;

    // Make sure there is a closing parenthesis
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            token_stream.current_location(),
            "Missing a closing parenthesis at the end of the function call: found a '{:?}' instead",
            token_stream.current_token_kind()
        )
    }

    token_stream.advance();

    // TODO
    // Makes sure the call value is correct for the function call
    // If so, the function call args are sorted into their correct order (if some are named or optional)
    // Once this is always working then default args can be removed from the JS output
    // let args = create_func_call_args(&expressions, argument_refs, &x.current_position())?;

    // look for which arguments are being accessed from the function call
    // let return_type = create_args_from_types(returns);
    // let accessed_args = get_accessed_args(x, name, &DataType::Object(return_type), &mut Vec::new(), captured_declarations)?;

    // Inline this function call if it's pure and the function call is pure
    // if is_pure && call_value.is_pure() {
    //     let original_function = variable_declarations
    //         .iter()
    //         .find(|a| a.name == *name)
    //         .unwrap();
    //     return inline_function_call(&args, &accessed_args, &original_function.value);
    // }

    Ok(AstNode {
        kind: NodeKind::FunctionCall(
            name.to_owned(),
            expressions,
            returned_types.to_owned(),
            token_stream.current_location(),
        ),
        location: token_stream.current_location(),
        scope: context.scope_name.to_owned(),
        lifetime: context.owned_lifetimes,
    })
}

pub fn create_function_call_arguments(
    token_stream: &mut TokenContext,
    required_arguments: &[Arg],
    context: &ScopeContext,
) -> Result<Vec<Expression>, CompileError> {
    // Starts at the first token after the function name
    ast_log!("Creating function call arguments");

    // make sure there is an open parenthesis
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_syntax_error!(
            token_stream.current_location(),
            "Expected a parenthesis after function call. Found '{:?}' instead.",
            token_stream.current_token_kind()
        )
    }

    token_stream.advance();

    if required_arguments.is_empty() {
        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                token_stream.current_location(),
                "This function does not accept any arguments, found '{:?}' instead",
                token_stream.current_token_kind()
            )
        }

        Ok(Vec::new())
    } else {
        let required_argument_types = required_arguments
            .iter()
            .map(|arg| arg.value.data_type.to_owned())
            .collect::<Vec<DataType>>();

        let call_context = context.new_child_expression(required_argument_types);

        create_multiple_expressions(token_stream, &call_context, false)
    }
}

fn create_arg_constructor(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    pure: &mut bool,
) -> Result<Vec<Arg>, CompileError> {
    let mut args = Vec::<Arg>::new();
    let mut next_in_list: bool = true;

    if token_stream.current_token_kind() != &TokenKind::StructDefinition {
        return_syntax_error!(
            token_stream.current_location(),
            "Expected a | after the function name",
        )
    }

    token_stream.advance();

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::StructDefinition => {
                token_stream.advance();
                return Ok(args);
            }

            TokenKind::Symbol(arg_name, ..) => {
                if !next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should have a comma to separate arguments",
                    )
                }

                // Create a new variable
                let argument = new_arg(
                    token_stream,
                    &arg_name,
                    &context,
                    &mut VarVisibility::Private,
                )?;

                if argument.value.data_type.is_mutable() {
                    *pure = false;
                }

                args.push(argument);

                next_in_list = false;
            }

            TokenKind::Comma => {
                token_stream.advance();
                next_in_list = true;
            }

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Unexpected token used in function arguments: {:?}",
                    token_stream.current_token_kind()
                )
            }
        }
    }

    Ok(args)
}
