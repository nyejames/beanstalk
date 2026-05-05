//! Shared argument parsing for builtin receiver members.
//!
//! WHAT: validates builtin argument lists and adapts them to call-validation expectations.
//! WHY: collection and error builtins share positional-only parsing and type validation rules.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, ExpectedAccessMode, ParameterExpectation, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;

/// Parses builtin receiver-method arguments using positional-only policy.
pub(super) fn parse_builtin_method_args(
    token_stream: &mut FileTokens,
    member_name: &str,
    expected_types: &[DataType],
    context: &ScopeContext,
    member_location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<Vec<CallArgument>, CompilerError> {
    if expected_types.is_empty() {
        if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
            return_rule_error!(
                "Builtin method call is missing '(' before the argument list.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call the method with parentheses, for example '.length()'",
                }
            );
        }

        token_stream.advance();

        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return_rule_error!(
                "This builtin method takes no arguments.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove the extra argument",
                }
            );
        }

        token_stream.advance();
        return Ok(Vec::new());
    }

    let expectations = expected_types
        .iter()
        .map(|expected_type| ParameterExpectation {
            name: None,
            data_type: expected_type.to_owned(),
            access_mode: ExpectedAccessMode::Shared,
            default_value: None,
        })
        .collect::<Vec<_>>();

    let parsed_arguments = parse_call_arguments(token_stream, context, string_table)?;
    if parsed_arguments
        .iter()
        .any(|argument| argument.target_param.is_some())
    {
        return_rule_error!(
            "Named arguments are not supported for builtin member calls",
            member_location.to_owned(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use positional arguments for builtin member calls",
            }
        );
    }

    resolve_call_arguments(
        CallDiagnosticContext::builtin_member(member_name),
        &parsed_arguments,
        &expectations,
        member_location.to_owned(),
        string_table,
    )
}
