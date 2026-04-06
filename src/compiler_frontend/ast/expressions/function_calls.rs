use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::expressions::parse_expression::create_multiple_expressions;
use crate::compiler_frontend::ast::module_ast::ScopeContext;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::result_handling::{
    ResultHandledCall, is_result_propagation_boundary, parse_named_result_handler_call,
    parse_result_fallback_values,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::host_functions::HostFunctionDef;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};
use crate::compiler_frontend::display_messages::get_type_conversion_hint;

// Built-in functions will do their own thing
pub fn parse_function_call(
    token_stream: &mut FileTokens,
    id: &InternedPath,
    context: &ScopeContext,
    signature: &FunctionSignature,
    value_required: bool,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Assumes we're starting at the first token after the name of the function call
    // Check if it's a host function first
    if let Some(host_func) = &context
        .host_registry
        .get_function(id.name_str(string_table).unwrap_or(""))
    {
        return parse_host_function_call(token_stream, host_func, context, string_table);
    }

    // Create expressions until hitting a closed parenthesis
    let args =
        create_function_call_arguments(token_stream, &signature.parameters, context, string_table)?;
    validate_user_function_argument_types(
        id,
        &args,
        &signature.parameters,
        token_stream.current_location(),
        string_table,
    )?;

    let call = ResultHandledCall {
        name: id.to_owned(),
        args,
        result_types: signature.return_data_types(),
        call_location: token_stream.current_location(),
    };

    if let Some(error_return) = signature.error_return() {
        if token_stream.current_token_kind() == &TokenKind::Bang {
            token_stream.advance();

            if is_result_propagation_boundary(token_stream.current_token_kind()) {
                let Some(expected_error_type) = context.expected_error_type.as_ref() else {
                    return_rule_error!(
                        "This call uses '!' propagation, but the surrounding function does not declare an error return slot",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Call Parsing",
                            PrimarySuggestion => "Declare a matching error slot in the surrounding function signature",
                        }
                    );
                };

                if expected_error_type != error_return.data_type() {
                    return_type_error!(
                        format!(
                            "Mismatched propagated error type. Called function returns '{}', but current function expects '{}'.",
                            error_return.data_type().display_with_table(string_table),
                            expected_error_type.display_with_table(string_table)
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Call Parsing",
                            PrimarySuggestion => "Use a function with the same error type or change the surrounding function error slot type",
                        }
                    );
                }

                return Ok(AstNode {
                    kind: NodeKind::ResultHandledFunctionCall {
                        name: call.name,
                        args: call.args,
                        result_types: call.result_types,
                        handling: ResultCallHandling::Propagate,
                        location: call.call_location,
                    },
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            if call.result_types.is_empty() {
                return_rule_error!(
                    "This function has no success return values, so fallback values cannot be provided here",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Use plain propagation syntax 'call(...)!' for error-only functions",
                    }
                );
            }

            let fallback_values = parse_result_fallback_values(
                token_stream,
                context,
                &call.result_types,
                "Fallback values",
                string_table,
            )?;

            return Ok(call.into_ast_node(
                ResultCallHandling::Fallback(fallback_values),
                token_stream.current_location(),
                &context.scope,
            ));
        }

        if matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang)
        {
            return parse_named_result_handler_call(
                token_stream,
                context,
                call,
                error_return.data_type(),
                value_required,
                warnings,
                string_table,
            );
        }

        return_rule_error!(
            "Calls to error-returning functions must be explicitly handled with '!' syntax",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Use 'call(...)!' to propagate or 'call(...) ! fallback' to provide fallback values",
            }
        );
    } else if token_stream.current_token_kind() == &TokenKind::Bang {
        return_rule_error!(
            "The '!' call-handling suffix is only valid for functions that declare an error return slot",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Remove '!' from this call or add an error slot to the called function",
            }
        );
    }

    Ok(AstNode {
        kind: NodeKind::FunctionCall {
            name: call.name,
            args: call.args,
            result_types: call.result_types,
            location: call.call_location,
        },
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    })
}

pub fn create_function_call_arguments(
    token_stream: &mut FileTokens,
    required_arguments: &[Declaration],
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
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Add '(' after the function name",
                SuggestedInsertion => "(",
            }
        )
    }

    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        let missing_required = required_arguments
            .iter()
            .filter(|argument| matches!(argument.value.kind, ExpressionKind::NoValue))
            .count();

        if missing_required > 0 {
            return_syntax_error!(
                format!(
                    "This function requires {missing_required} argument(s) without defaults, but none were provided.",
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Function Call Parsing",
                    PrimarySuggestion => "Provide the required arguments or add defaults in the declaration",
                }
            )
        }

        token_stream.advance();
        return Ok(Vec::new());
    }

    if required_arguments.is_empty() {
        // Make sure there is a closing parenthesis
        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_syntax_error!(
                format!(
                    "This function does not accept any arguments, found '{:?}' instead",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
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
        let required_argument_types: Vec<DataType> = required_arguments
            .iter()
            .map(|argument| match &argument.value.data_type {
                // WHAT: keep immutable-collection argument parsing permissive.
                // WHY: call compatibility allows mutable collections for immutable parameters.
                // Parsing should defer this ownership-specific check to function call validation.
                DataType::Collection(_, ownership) if !ownership.is_mutable() => DataType::Inferred,
                _ => argument.value.data_type.to_owned(),
            })
            .collect();

        let call_context = context.new_child_expression(required_argument_types.to_owned());

        create_multiple_expressions(token_stream, &call_context, true, string_table)
    }
}

fn validate_user_function_argument_types(
    function_name: &InternedPath,
    args: &[Expression],
    parameters: &[Declaration],
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for (index, (expression, parameter)) in args.iter().zip(parameters.iter()).enumerate() {
        if !&expression.data_type.accepts_value_type(&parameter.value.data_type) {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                    index + 1,
                    function_name.name_str(string_table).unwrap_or("<unknown>"),
                    &parameter.value.data_type.display_with_table(string_table),
                    &expression.data_type.display_with_table(string_table),
                    get_type_conversion_hint(&expression.data_type, &parameter.value.data_type)
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }
    }

    Ok(())
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
    let args = create_function_call_arguments(
        token_stream,
        &params_as_args.parameters,
        context,
        string_table,
    )?;

    // Validate the host function call
    validate_host_function_call(host_func, &args, location.clone(), string_table)?;

    // Create an interned path name from the name
    let name = InternedPath::from_single_str(host_func.name, string_table);

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall {
            name,
            args,
            result_types: params_as_args.return_data_types(),
            location: location.clone(),
        },
        location,
        scope: context.scope.clone(),
    })
}

/// Validate a host function call against its signature
pub fn validate_host_function_call(
    function: &HostFunctionDef,
    args: &[Expression],
    location: SourceLocation,
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
                location,
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
                location,
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
                location,
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

    if function.name == crate::compiler_frontend::host_functions::IO_FUNC_NAME {
        for (i, expression) in args.iter().enumerate() {
            if expression.data_type.is_result() {
                return_type_error!(
                    format!(
                        "Argument {} to function '{}' has incorrect type. Expected a renderable value, but got {}. Result values must be handled before reaching io(...).",
                        i + 1,
                        function.name,
                        &expression.data_type.display_with_table(string_table)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Handle the Result with '!' syntax before passing it to io(...)",
                    }
                );
            }

            if !matches!(
                expression.data_type,
                DataType::StringSlice
                    | DataType::Template
                    | DataType::TemplateWrapper
                    | DataType::Int
                    | DataType::Float
                    | DataType::Bool
                    | DataType::Char
                    | DataType::Path(_)
            ) {
                return_type_error!(
                    format!(
                        "Argument {} to function '{}' has incorrect type. Expected a final scalar or textual value, but got {}.",
                        i + 1,
                        function.name,
                        expression.data_type.display_with_table(string_table)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Render collections/structs/templates earlier or pass a scalar/textual value to io(...)",
                    }
                );
            }
        }

        return Ok(());
    }

    for (i, (expression, param)) in args.iter().zip(&function.parameters).enumerate() {
        if !param
            .language_type
            .accepts_value_type(&expression.data_type)
        {
            return_type_error!(
                format!(
                    "Argument {} to function '{}' has incorrect type. Expected {}, but got {}. {}",
                    i + 1,
                    function.name,
                    &param.language_type.display_with_table(string_table),
                    &expression.data_type.display_with_table(string_table),
                    get_type_conversion_hint(&expression.data_type, &param.language_type)
                ),
                location,
                {
                    CompilationStage => "Function Call Validation",
                    PrimarySuggestion => "Convert the argument to the expected type",
                }
            );
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/function_call_tests.rs"]
mod function_call_tests;
