//! Function-call parsing and result-handling suffix integration.
//!
//! WHAT: parses raw call argument syntax, resolves user/host call signatures, and applies the
//! `!` result-handling forms that can follow a call expression.
//! WHY: call parsing sits at the boundary between general expression parsing and call-specific
//! validation, so keeping that flow together makes the refactor seams easier to follow.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_host_function, expectations_from_user_parameters,
    resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::ResultCallHandling;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::result_handling::{
    ResultHandledCall, is_result_propagation_boundary, parse_named_result_handler_call,
    parse_result_fallback_values,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostFunctionDef;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause,
};
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

pub fn parse_function_call(
    token_stream: &mut FileTokens,
    id: &InternedPath,
    context: &ScopeContext,
    signature: &FunctionSignature,
    value_required: bool,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // Host calls share the same argument parser, but they reject named targets until
    // host metadata carries stable public parameter names.
    if let Some(host_func) = &context
        .host_registry
        .get_function(id.name_str(string_table).unwrap_or(""))
    {
        return parse_host_function_call(token_stream, host_func, context, string_table);
    }

    let raw_args = parse_call_arguments(token_stream, context, string_table)?;
    let args = resolve_user_function_call_arguments(
        id,
        &raw_args,
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
                    let call_name = id.name_str(string_table).unwrap_or("<function>");
                    return_type_error!(
                        format!(
                            "Mismatched propagated error type for call '{}'. {} Offending call: {}(...).",
                            call_name,
                            expected_found_clause(
                                expected_error_type,
                                error_return.data_type(),
                                string_table
                            ),
                            call_name
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Function Call Parsing",
                            ExpectedType => expected_error_type.display_with_table(string_table),
                            FoundType => error_return.data_type().display_with_table(string_table),
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
                "Function Call Parsing",
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

/// Parses the raw `(...)` argument list shared by all call-shaped syntax.
pub fn parse_call_arguments(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<CallArgument>, CompilerError> {
    ast_log!("Creating function call arguments");

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
    token_stream.skip_newlines();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        token_stream.advance();
        return Ok(Vec::new());
    }

    let mut args: Vec<CallArgument> = Vec::new();
    loop {
        token_stream.skip_newlines();
        if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
            token_stream.advance();
            break;
        }

        let argument_location = token_stream.current_location();
        let named_target = match token_stream.current_token_kind() {
            TokenKind::Mutable
                if matches!(token_stream.peek_next_token(), Some(TokenKind::Symbol(_)))
                    && token_stream
                        .tokens
                        .get(token_stream.index + 2)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::Assign) =>
            {
                return_syntax_error!(
                    "Mutable marker '~' is only allowed on the value side of a named argument",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Write named mutable arguments as 'parameter = ~value'",
                    }
                );
            }
            TokenKind::Symbol(name)
                if token_stream.peek_next_token() == Some(&TokenKind::Assign) =>
            {
                let target_location = token_stream.current_location();
                let target_name = *name;
                token_stream.advance();
                token_stream.advance();
                token_stream.skip_newlines();
                Some((target_name, target_location))
            }
            TokenKind::OpenParenthesis
                if matches!(token_stream.peek_next_token(), Some(TokenKind::Symbol(_)))
                    && token_stream
                        .tokens
                        .get(token_stream.index + 2)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::CloseParenthesis)
                    && token_stream
                        .tokens
                        .get(token_stream.index + 3)
                        .map(|token| &token.kind)
                        == Some(&TokenKind::Assign) =>
            {
                return_syntax_error!(
                    "Named argument target must be a parameter name",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Use a bare parameter name on the left side of '='",
                    }
                );
            }
            _ => None,
        };

        let access_mode = if token_stream.current_token_kind() == &TokenKind::Mutable {
            token_stream.advance();
            CallAccessMode::Mutable
        } else {
            CallAccessMode::Shared
        };

        if token_stream.current_token_kind() == &TokenKind::Comma
            || token_stream.current_token_kind() == &TokenKind::CloseParenthesis
        {
            if named_target.is_some() {
                return_syntax_error!(
                    "Expected expression after '=' in named argument",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Provide a value expression on the right side of '='",
                    }
                );
            }
            return_syntax_error!(
                "Expected expression for call argument",
                token_stream.current_location(),
                {
                    CompilationStage => "Function Call Parsing",
                    PrimarySuggestion => "Provide a value expression for this argument",
                }
            );
        }

        let value = create_expression(
            token_stream,
            context,
            &mut DataType::Inferred,
            &Ownership::ImmutableOwned,
            false,
            string_table,
        )?;

        args.push(if let Some((name, target_location)) = named_target {
            CallArgument::named(value, name, access_mode, argument_location, target_location)
        } else {
            CallArgument::positional(value, access_mode, argument_location)
        });

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                token_stream.advance();
                token_stream.skip_newlines();
            }
            TokenKind::CloseParenthesis => {
                token_stream.advance();
                break;
            }
            _ => {
                return_syntax_error!(
                    format!(
                        "Expected ',' or ')' after call argument, found '{:?}'",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Function Call Parsing",
                        PrimarySuggestion => "Separate arguments with commas and close the call with ')'",
                    }
                );
            }
        }
    }

    Ok(args)
}

fn resolve_user_function_call_arguments(
    function_name: &InternedPath,
    raw_args: &[CallArgument],
    parameters: &[Declaration],
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<Vec<CallArgument>, CompilerError> {
    let expectations = expectations_from_user_parameters(parameters);
    resolve_call_arguments(
        CallDiagnosticContext::function(
            function_name.name_str(string_table).unwrap_or("<unknown>"),
        ),
        raw_args,
        &expectations,
        location,
        string_table,
    )
}

/// Parses a host-function call using the shared argument resolver plus host-only validation.
pub fn parse_host_function_call(
    token_stream: &mut FileTokens,
    host_func: &HostFunctionDef,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();

    // Host metadata does not expose public parameter names yet, so named arguments remain
    // intentionally unsupported.
    let raw_args = parse_call_arguments(token_stream, context, string_table)?;
    if raw_args
        .iter()
        .any(|argument| argument.target_param.is_some())
    {
        return_rule_error!(
            "Named arguments are not supported for host function calls",
            location.clone(),
            {
                CompilationStage => "Function Call Validation",
                PrimarySuggestion => "Use positional arguments when calling host functions",
            }
        );
    }

    let expectations = expectations_from_host_function(host_func);
    let args = resolve_call_arguments(
        CallDiagnosticContext::host_function(host_func.name),
        &raw_args,
        &expectations,
        location.clone(),
        string_table,
    )?;
    validate_host_specific_call_rules(host_func, &args, location.clone(), string_table)?;

    let name = InternedPath::from_single_str(host_func.name, string_table);

    Ok(AstNode {
        kind: NodeKind::HostFunctionCall {
            name,
            args,
            result_types: host_func.return_data_types(),
            location: location.clone(),
        },
        location,
        scope: context.scope.clone(),
    })
}

/// Validates host-specific semantic rules that sit on top of shared call validation.
fn validate_host_specific_call_rules(
    function: &HostFunctionDef,
    args: &[CallArgument],
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if function.name == crate::compiler_frontend::host_functions::IO_FUNC_NAME {
        for (i, argument) in args.iter().enumerate() {
            if argument.value.data_type.is_result() {
                return_type_error!(
                    format!(
                        "Argument {} to function '{}' has incorrect type. Expected a renderable value, but got {}. {} Result values must be handled before reaching io(...).",
                        i + 1,
                        function.name,
                        &argument.value.data_type.display_with_table(string_table),
                        offending_value_clause(&argument.value, string_table),
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Function Call Validation",
                        PrimarySuggestion => "Handle the Result with '!' syntax before passing it to io(...)",
                    }
                );
            }

            if !matches!(
                argument.value.data_type,
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
                        "Argument {} to function '{}' has incorrect type. Expected a final scalar or textual value, but got {}. {}",
                        i + 1,
                        function.name,
                        argument.value.data_type.display_with_table(string_table),
                        offending_value_clause(&argument.value, string_table),
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

    Ok(())
}

#[cfg(test)]
#[path = "tests/function_call_tests.rs"]
mod function_call_tests;
