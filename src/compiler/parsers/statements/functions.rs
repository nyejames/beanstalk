// use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::Ownership::ImmutableOwned;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::HostFunctionDef;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::structs::{create_struct_definition, parse_parameters};
use crate::compiler::string_interning::{StringId, StringTable};

use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::{ast_log, return_syntax_error, return_type_error};

// Arg names and types are required
// Can have default values
#[derive(Clone, Debug)]
pub struct FunctionSignature {
    pub parameters: Vec<Arg>,
    pub returns: Vec<Arg>,
}

impl FunctionSignature {
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        string_table: &mut StringTable,
    ) -> Result<Self, CompilerError> {
        // Should start at the Colon
        // Need to skip it,
        token_stream.advance();

        let parameters = parse_parameters(token_stream, context, &mut true, string_table, false)?;

        // create_struct_definition already advances past the closing |,
        // So we're now at the Arrow or Colon token
        match token_stream.current_token_kind() {
            TokenKind::Arrow => {}

            // Function does not return anything
            TokenKind::Colon => {
                token_stream.advance();
                return Ok(FunctionSignature {
                    parameters,
                    returns: Vec::new(),
                });
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Expected an arrow operator or colon after function arguments. Found {:?} instead.",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Function Signature Parsing",
                        PrimarySuggestion => "Use '->' for functions with return values or ':' for functions without return values",
                    }
                )
            }
        }

        // Parse return types
        let mut returns: Vec<Arg> = Vec::new();
        let mut next_in_list: bool = true;
        let mut mutable: bool = false;

        loop {
            token_stream.advance();

            match token_stream.current_token_kind() {
                TokenKind::Mutable => {
                    if !next_in_list {
                        return_syntax_error!(
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }

                    mutable = true;
                }

                TokenKind::DatatypeInt => {
                    if !next_in_list {
                        return_syntax_error!(
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }
                    let return_name = format!("return_{}", returns.len());
                    returns.push(Arg {
                        id: string_table.intern(&return_name),
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
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }
                    let return_name = format!("return_{}", returns.len());
                    returns.push(Arg {
                        id: string_table.intern(&return_name),
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
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }
                    let return_name = format!("return_{}", returns.len());
                    returns.push(Arg {
                        id: string_table.intern(&return_name),
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
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }
                    let return_name = format!("return_{}", returns.len());
                    returns.push(Arg {
                        id: string_table.intern(&return_name),
                        value: Expression::string_slice(
                            string_table.intern(""), // Empty string
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
                            "Should have a comma to separate return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Add ',' between return types",
                                SuggestedInsertion => ",",
                            }
                        )
                    }

                    // TODO: This needs to be updated to resolve interned strings
                    // when string table integration is complete
                    // For now, we'll skip this symbol resolution
                    //
                    // Search through declarations for any data types
                    // Also search through the function parameters,
                    // as the function can return references to those parameters.
                    // if let Some(possible_type) = context.get_reference(name) {
                    //     // Make sure this is actually a struct (Args)
                    //     if matches!(possible_type.value.data_type, DataType::Parameters(..)) {
                    //         returns.push(possible_type.to_owned());
                    //     }
                    // } else if let Some(reference_return) = parameters.get_reference(name) {
                    //     returns.push(reference_return.to_owned());
                    // }
                }

                TokenKind::Colon => {
                    token_stream.advance();
                    return Ok(FunctionSignature {
                        parameters,
                        returns,
                    });
                }

                TokenKind::Comma => {
                    if next_in_list {
                        return_syntax_error!(
                            "Should not have a comma at the end of the return types",
                            token_stream.current_location().to_error_location(&string_table),
                            {
                                CompilationStage => "Function Signature Parsing",
                                PrimarySuggestion => "Remove the trailing comma or add another return type",
                            }
                        )
                    }

                    next_in_list = true;
                }

                _ => {
                    return_syntax_error!(
                        "Expected a type keyword after the arrow operator",
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Function Signature Parsing",
                            PrimarySuggestion => "Use a valid type (Int, String, Float, Bool) for the return type",
                        }
                    )
                }
            }
        }
    }

    pub fn default() -> Self {
        Self {
            parameters: Vec::new(),
            returns: Vec::new(),
        }
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

/// Format a DataType for user-friendly error messages
/// Note: This function provides basic type names without resolving interned strings.
/// For full type information with resolved strings, use DataType::display_with_table().
fn format_type_for_error(data_type: &DataType) -> String {
    match data_type {
        DataType::String => "String".to_string(),
        DataType::Int => "Int".to_string(),
        DataType::Float => "Float".to_string(),
        DataType::Bool => "Bool".to_string(),
        DataType::Template => "Template".to_string(),
        DataType::Function(..) => "Function".to_string(),
        DataType::Parameters(..) => "Args".to_string(),
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

/// Format a DataType for user-friendly error messages with proper string resolution
fn format_type_for_error_with_table(
    data_type: &DataType,
    string_table: &crate::compiler::string_interning::StringTable,
) -> String {
    // Use the display_with_table method for proper string resolution
    data_type.display_with_table(string_table)
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
        // CoerceToString accepts any type - this is the key for io() function
        (_, DataType::CoerceToString) => true,

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

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut FileTokens,
    id: &StringId,
    context: &ScopeContext,
    signature: &FunctionSignature,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Assumes we're starting at the first token after the name of the function call
    // Check if it's a host function first
    if let Some(host_func) = &context.host_registry.get_function(id) {
        return parse_host_function_call(token_stream, host_func, context, string_table);
    }

    // Create expressions until hitting a closed parenthesis
    let args =
        create_function_call_arguments(token_stream, &signature.parameters, context, string_table)?;

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

    let interned_name = *id;

    Ok(AstNode {
        kind: NodeKind::FunctionCall(
            interned_name,
            args,
            signature.returns.to_owned(),
            token_stream.current_location(),
        ),
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    })
}

pub fn create_function_call_arguments(
    token_stream: &mut FileTokens,
    required_arguments: &[Arg],
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    // Starts at the first token after the function name
    ast_log!("Creating function call arguments");

    // make sure there is an open parenthesis
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_syntax_error!(
            format!(
                "Expected a parenthesis after function call. Found '{:?}' instead.",
                token_stream.current_token_kind()
            ),
            token_stream.current_location().to_error_location(&string_table),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Add '(' after the function name",
                SuggestedInsertion => "(",
            }
        )
    }

    token_stream.advance();

    if required_arguments.is_empty() {
        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "This function does not accept any arguments, found '{:?}' instead",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Function Call Parsing",
                    PrimarySuggestion => "Remove the arguments or check the function signature",
                }
            )
        }

        // Advance past the closing parenthesis
        token_stream.advance();

        Ok(Vec::new())
    } else {
        let required_argument_types = required_arguments.to_owned();
        let call_context = context.new_child_expression(required_argument_types.to_owned());

        create_multiple_expressions(token_stream, &call_context, true, string_table)
    }
}

/// Coerce an expression to a string at compile time if possible
/// This handles compile-time constant folding for CoerceToString parameters
fn coerce_to_string_at_compile_time(
    expr: &Expression,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    match &expr.kind {
        // String literals pass through unchanged (optimization: no conversion needed)
        ExpressionKind::StringSlice(_) => Ok(expr.clone()),

        // Integer literals: fold to string at compile time (42 → "42")
        ExpressionKind::Int(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Float literals: fold to string at compile time (3.14 → "3.14")
        ExpressionKind::Float(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Boolean literals: fold to string at compile time (true → "true", false → "false")
        ExpressionKind::Bool(value) => {
            let string_value = value.to_string();
            let interned = string_table.get_or_intern(string_value);
            Ok(Expression::string_slice(
                interned,
                expr.location.clone(),
                Ownership::ImmutableOwned,
            ))
        }

        // Template literals: evaluate to string at compile time if possible
        ExpressionKind::Template(template) => {
            // Try to fold the template to a string
            if let Ok(folded_string) = template.fold(&None, string_table) {
                Ok(Expression::string_slice(
                    folded_string,
                    expr.location.clone(),
                    Ownership::ImmutableOwned,
                ))
            } else {
                // Template contains runtime expressions, can't fold at compile time
                // Return as-is for runtime conversion
                Ok(expr.clone())
            }
        }

        // Runtime expressions, variable references, function calls, etc.
        // These cannot be folded at compile time and will need runtime conversion
        _ => Ok(expr.clone()),
    }
}

/// Parse a host function call
pub fn parse_host_function_call(
    token_stream: &mut FileTokens,
    host_func: &HostFunctionDef,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();

    let params_as_args = host_func.params_to_signature(string_table);

    // Parse arguments using the same logic as regular function calls
    let mut args = create_function_call_arguments(
        token_stream,
        &params_as_args.parameters,
        context,
        string_table,
    )?;

    // Apply CoerceToString compile-time folding for constant arguments
    for (i, param) in host_func.parameters.iter().enumerate() {
        if matches!(param.data_type, DataType::CoerceToString) && i < args.len() {
            args[i] = coerce_to_string_at_compile_time(&args[i], string_table)?;
        }
    }

    // Validate the host function call
    validate_host_function_call(host_func, &args, location.clone(), &string_table)?;

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall(
            host_func.name,
            args,
            host_func.return_types.clone(),
            host_func.module,
            host_func.import_name,
            location.clone(),
        ),
        location,
        scope: context.scope.clone(),
    })
}

/// Validate a host function call against its signature
pub fn validate_host_function_call(
    function: &HostFunctionDef,
    args: &[Expression],
    location: TextLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Check argument count
    if args.len() != function.parameters.len() {
        let expected = function.parameters.len();
        let got = args.len();

        if expected == 0 {
            return_type_error!(
                format!(
                    "Function '{}' doesn't take any arguments, but {} {} provided. Did you mean to call it without parentheses?",
                    function.name,
                    got,
                    if got == 1 { "was" } else { "were" }
                ),
                location.to_error_location(&string_table),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Remove the parentheses and arguments",
                }
            );
        } else if got == 0 {
            return_type_error!(
                format!(
                    "Function '{}' expects {} argument{}, but none were provided",
                    function.name,
                    expected,
                    if expected == 1 { "" } else { "s" }
                ),
                location.to_error_location(&string_table),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Add the required arguments to the function call",
                }
            );
        } else {
            return_type_error!(
                format!(
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
                ),
                location.to_error_location(&string_table),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => if got > expected {
                        "Remove extra arguments"
                    } else {
                        "Add missing arguments"
                    },
                }
            );
        }
    }

    // Check argument types
    for (i, (expression, param)) in args.iter().zip(&function.parameters).enumerate() {
        if !types_compatible(&expression.data_type, &param.data_type) {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                    i + 1,
                    function.name,
                    format_type_for_error(&param.data_type),
                    format_type_for_error(&expression.data_type),
                    get_type_conversion_hint(&expression.data_type, &param.data_type)
                ),
                location.to_error_location(&string_table),
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }

        // Check mutability requirements
        if param.ownership.is_mutable() && !expression.ownership.is_mutable() {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' must be mutable, but an immutable {} was provided. Use '~{}' to make it mutable",
                    i + 1,
                    function.name,
                    format_type_for_error(&expression.data_type),
                    format_type_for_error(&param.data_type).to_lowercase()
                ),
                location.to_error_location(&string_table),
                {
                    BorrowKind => "Mutable",
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Add '~' before the argument to make it mutable",
                }
            );
        }
    }

    Ok(())
}
