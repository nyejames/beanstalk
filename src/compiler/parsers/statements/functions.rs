// use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::Ownership::ImmutableOwned;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::{HostFunctionDef, HostFunctionRegistry};
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::statements::structs::parse_multiple_args;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TextLocation, TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

// Arg names and types are required
// Can have default values
pub struct FunctionSignature {
    pub args: Vec<Arg>,
    pub returns: Vec<Arg>,
}

impl FunctionSignature {
    pub fn new(
        token_stream: &mut TokenContext,
        pure: &mut bool,
        context: &ScopeContext,
    ) -> Result<Self, CompileError> {
        let args = create_arg_constructor(token_stream, context.new_parameters(), pure)?;

        match token_stream.current_token_kind() {
            TokenKind::Arrow => {}

            // Function does not return anything
            TokenKind::Colon => {
                token_stream.advance();
                return Ok(FunctionSignature {
                    args,
                    returns: Vec::new(),
                });
            }

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Expected an arrow operator or colon after function arguments",
                )
            }
        }

        // Parse return types
        let mut returns: Vec<Arg> = Vec::new();
        let mut next_in_list: bool = true;
        let mut mutable: bool = false;

        while token_stream.index < token_stream.length {
            token_stream.advance();

            match token_stream.current_token_kind() {
                TokenKind::Mutable => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }

                    mutable = true;
                }

                TokenKind::DatatypeInt => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }
                    returns.push(Arg {
                        name: returns.len().to_string(),
                        value: Expression::int(
                            0,
                            token_stream.current_location(),
                            if mutable {
                                Ownership::MutableOwned
                            } else {
                                ImmutableOwned
                            },
                        ),
                    });
                }
                TokenKind::DatatypeFloat => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }
                    returns.push(Arg {
                        name: returns.len().to_string(),
                        value: Expression::float(
                            0.0,
                            token_stream.current_location(),
                            if mutable {
                                Ownership::MutableOwned
                            } else {
                                ImmutableOwned
                            },
                        ),
                    });
                }
                TokenKind::DatatypeBool => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }
                    returns.push(Arg {
                        name: returns.len().to_string(),
                        value: Expression::bool(
                            false,
                            token_stream.current_location(),
                            if mutable {
                                Ownership::MutableOwned
                            } else {
                                ImmutableOwned
                            },
                        ),
                    });
                }
                TokenKind::DatatypeString => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }
                    returns.push(Arg {
                        name: returns.len().to_string(),
                        value: Expression::string_slice(
                            "".to_string(),
                            token_stream.current_location(),
                            if mutable {
                                Ownership::MutableOwned
                            } else {
                                ImmutableOwned
                            },
                        ),
                    });
                }

                TokenKind::Symbol(name) => {
                    if !next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should have a comma to separate return types",
                        )
                    }

                    // Search through declarations for any data types
                    // Also search through the function parameters,
                    // as the function can return references to those parameters.
                    if let Some(possible_type) = context.get_reference(name) {
                        // Make sure this is actually a struct (Args)
                        if matches!(possible_type.value.data_type, DataType::Args(..)) {
                            returns.push(possible_type.to_owned());
                        }
                    } else if let Some(reference_return) = args.get_reference(name) {
                        returns.push(reference_return.to_owned());
                    }
                }

                TokenKind::Colon => {
                    token_stream.advance();
                    return Ok(FunctionSignature { args, returns });
                }

                TokenKind::Comma => {
                    if next_in_list {
                        return_syntax_error!(
                            token_stream.current_location(),
                            "Should not have a comma at the end of the return types",
                        )
                    }

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

/// Parse a function call, checking host function registry first
pub fn parse_function_call_with_registry(
    token_stream: &mut TokenContext,
    name: &str,
    context: &ScopeContext,
    registry: &HostFunctionRegistry,
) -> Result<AstNode, CompileError> {
    // Check if it's a host function first
    if let Some(host_func) = registry.get_function(name) {
        return parse_host_function_call(token_stream, host_func, context);
    }

    // If not a host function, we need to look up the function in the context
    if let Some(func_ref) = context.get_reference(name) {
        if let DataType::Function(required_arguments, returned_types) = &func_ref.value.data_type {
            return parse_function_call(
                token_stream,
                name,
                context,
                required_arguments,
                returned_types,
            );
        }
    }

    return_rule_error!(
        token_stream.current_location(),
        "Function '{}' is not defined. Make sure the function is declared before calling it",
        name
    );
}

/// Parse a host function call
pub fn parse_host_function_call(
    token_stream: &mut TokenContext,
    host_func: &HostFunctionDef,
    context: &ScopeContext,
) -> Result<AstNode, CompileError> {
    let location = token_stream.current_location();

    // Advance past the function name
    token_stream.advance();

    // Parse arguments using the same logic as regular function calls
    let args = parse_host_function_arguments(token_stream, host_func, context)?;

    // Validate the host function call
    validate_host_function_call(host_func, &args, &location)?;

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall(
            host_func.name.clone(),
            args,
            host_func.return_types.clone(),
            host_func.module.clone(),
            host_func.import_name.clone(),
            location.clone(),
        ),
        location,
        scope: context.scope_name.to_owned(),
    })
}

/// Validate a host function call against its signature
pub fn validate_host_function_call(
    function: &HostFunctionDef,
    args: &[Expression],
    location: &TextLocation,
) -> Result<(), CompileError> {
    // Check argument count
    if args.len() != function.parameters.len() {
        let expected = function.parameters.len();
        let got = args.len();

        if expected == 0 {
            return_type_error!(
                location.clone(),
                "Function '{}' doesn't take any arguments, but {} {} provided. Did you mean to call it without parentheses?",
                function.name,
                got,
                if got == 1 { "was" } else { "were" }
            );
        } else if got == 0 {
            return_type_error!(
                location.clone(),
                "Function '{}' expects {} argument{}, but none were provided",
                function.name,
                expected,
                if expected == 1 { "" } else { "s" }
            );
        } else {
            return_type_error!(
                location.clone(),
                "Function '{}' expects {} argument{}, got {}. {}",
                function.name,
                expected,
                if expected == 1 { "" } else { "s" },
                got,
                if got > expected {
                    "Too many arguments provided"
                } else {
                    "Not enough arguments provided"
                }
            );
        }
    }

    // Check argument types
    for (i, (arg, param)) in args.iter().zip(&function.parameters).enumerate() {
        if !types_compatible(&arg.data_type, &param.value.data_type) {
            return_type_error!(
                location.clone(),
                "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                i + 1,
                function.name,
                format_type_for_error(&param.value.data_type),
                format_type_for_error(&arg.data_type),
                get_type_conversion_hint(&arg.data_type, &param.value.data_type)
            );
        }

        // Check mutability requirements
        if param.value.ownership.is_mutable() && !arg.ownership.is_mutable() {
            return_type_error!(
                location.clone(),
                "Argument {} to function '{}' must be mutable, but an immutable {} was provided. Use '~{}' to make it mutable",
                i + 1,
                function.name,
                format_type_for_error(&arg.data_type),
                format_type_for_error(&param.value.data_type).to_lowercase()
            );
        }
    }

    Ok(())
}

/// Format a DataType for user-friendly error messages
fn format_type_for_error(data_type: &DataType) -> String {
    match data_type {
        DataType::String => "String".to_string(),
        DataType::Int => "Int".to_string(),
        DataType::Float => "Float".to_string(),
        DataType::Bool => "Bool".to_string(),
        DataType::Template => "Template".to_string(),
        DataType::Function(..) => "Function".to_string(),
        DataType::Args(..) => "Args".to_string(),
        DataType::Choices(types) => {
            let type_names: Vec<String> = types
                .iter()
                .map(|t| format_type_for_error(&t.value.data_type))
                .collect();
            format!("({})", type_names.join(" | "))
        }
        DataType::Inferred => "Inferred".to_string(),
        DataType::Range => "Range".to_string(),
        DataType::None => "None".to_string(),
        DataType::True => "True".to_string(),
        DataType::False => "False".to_string(),
        DataType::CoerceToString => "String".to_string(),
        DataType::Decimal => "Decimal".to_string(),
        DataType::Collection(inner, _) => format!("Collection<{}>", format_type_for_error(inner)),
        DataType::Struct(..) => "Struct".to_string(),
        DataType::Option(inner) => format!("Option<{}>", format_type_for_error(inner)),
        DataType::Reference(data_type, ownership) => {
            format!(
                "{} {} Reference",
                format_type_for_error(data_type),
                ownership.as_string()
            )
        }
    }
}

/// Provide helpful hints for type conversion
fn get_type_conversion_hint(from_type: &DataType, to_type: &DataType) -> String {
    match (from_type, to_type) {
        (DataType::Int, DataType::String) => {
            "Try converting the integer to a string first".to_string()
        }
        (DataType::Float, DataType::String) => {
            "Try converting the float to a string first".to_string()
        }
        (DataType::Bool, DataType::String) => {
            "Try converting the boolean to a string first".to_string()
        }
        (DataType::String, DataType::Int) => {
            "Try parsing the string as an integer first".to_string()
        }
        _ => "Check the function documentation for the expected argument types".to_string(),
    }
}

/// Check if two types are compatible for function call arguments
fn types_compatible(arg_type: &DataType, param_type: &DataType) -> bool {
    // Basic type compatibility check
    // This is a simplified version - in a full implementation, this would handle
    // more complex type relationships, ownership, mutability, etc.
    match (arg_type, param_type) {
        // Exact type matches
        (DataType::String, DataType::String) => true,
        (DataType::Int, DataType::Int) => true,
        (DataType::Float, DataType::Float) => true,
        (DataType::Bool, DataType::Bool) => true,
        (DataType::Template, DataType::Template) => true,

        // Handle inferred types - they should be compatible with their target
        (DataType::Inferred, _target) | (_target, DataType::Inferred) => {
            // For now, assume inferred types are compatible
            // In a full implementation; this would check the inferred type
            true
        }

        // Numeric type promotions (if we want to allow them)
        // (DataType::Int, DataType::Float) => true,  // Int can be promoted to Float

        // All other combinations are incompatible
        _ => false,
    }
}

/// Parse arguments for a host function call
pub fn parse_host_function_arguments(
    token_stream: &mut TokenContext,
    host_func: &HostFunctionDef,
    context: &ScopeContext,
) -> Result<Vec<Expression>, CompileError> {
    ast_log!(
        "Parsing host function call arguments for '{}'",
        host_func.name
    );

    // Make sure there is an open parenthesis
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_syntax_error!(
            token_stream.current_location(),
            "Expected a parenthesis after function call '{}'. Found '{:?}' instead.",
            host_func.name,
            token_stream.current_token_kind()
        );
    }

    token_stream.advance();

    if host_func.parameters.is_empty() {
        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                token_stream.current_location(),
                "Function '{}' does not accept any arguments, found '{:?}' instead",
                host_func.name,
                token_stream.current_token_kind()
            );
        }

        token_stream.advance();
        Ok(Vec::new())
    } else {
        let required_argument_types = host_func.parameters.to_owned();
        let call_context = context.new_child_expression(required_argument_types);

        let args = create_multiple_expressions(token_stream, &call_context, false)?;

        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                token_stream.current_location(),
                "Missing a closing parenthesis at the end of the function call '{}': found a '{:?}' instead",
                host_func.name,
                token_stream.current_token_kind()
            );
        }

        token_stream.advance();
        Ok(args)
    }
}

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut TokenContext,
    name: &str,
    context: &ScopeContext,
    required_arguments: &[Arg],
    returned_types: &[Arg],
) -> Result<AstNode, CompileError> {
    // Assumes starting at the first token after the name of the function call

    // Create expressions until hitting a closed parenthesis
    let args = create_function_call_arguments(token_stream, required_arguments, context)?;

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
            args,
            returned_types.to_owned(),
            token_stream.current_location(),
        ),
        location: token_stream.current_location(),
        scope: context.scope_name.to_owned(),
    })
}

pub fn create_function_call_arguments<'a>(
    token_stream: &mut TokenContext,
    required_arguments: &[Arg],
    context: &ScopeContext,
) -> Result<Vec<Arg>, CompileError> {
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
        let required_argument_types = required_arguments.to_owned();
        let call_context = context.new_child_expression(required_argument_types.to_owned());

        parse_multiple_args(
            token_stream,
            call_context,
            &TokenKind::CloseParenthesis,
            &mut false,
        )
    }
}

fn create_arg_constructor(
    token_stream: &mut TokenContext,
    context: ScopeContext,
    pure: &mut bool,
) -> Result<Vec<Arg>, CompileError> {
    if token_stream.current_token_kind() != &TokenKind::FuncParameterBracket {
        return_syntax_error!(
            token_stream.current_location(),
            "Expected a | after the function name",
        )
    }

    token_stream.advance();

    parse_multiple_args(
        token_stream,
        context,
        &TokenKind::FuncParameterBracket,
        pure,
    )
}
