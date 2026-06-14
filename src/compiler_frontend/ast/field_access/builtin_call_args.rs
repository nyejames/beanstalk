//! Shared argument parsing for builtin receiver members.
//!
//! WHAT: validates builtin argument lists and adapts them to call-validation expectations.
//! WHY: collection and error builtins share positional-only parsing and type validation rules.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallArgumentResolutionContext, CallDiagnosticContext, ExpectedAccessMode,
    ExpectedParameterType, ParameterExpectation, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::function_calls::{
    NamedArgumentSyntax, parse_call_arguments_typed_with_expectations,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidBuiltinCallReason};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};

pub(super) fn parse_builtin_method_args_typed(
    token_stream: &mut FileTokens,
    member_name: &str,
    expected_type_ids: &[TypeId],
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    member_location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<Vec<CallArgument>, ExpressionParseError> {
    if expected_type_ids.is_empty() {
        if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
            return Err(CompilerDiagnostic::invalid_builtin_call(
                InvalidBuiltinCallReason::MissingParentheses,
                Some(string_table.intern(member_name)),
                token_stream.current_location(),
            )
            .into());
        }

        token_stream.advance();

        if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
            return Err(CompilerDiagnostic::invalid_builtin_call(
                InvalidBuiltinCallReason::TakesNoArguments,
                Some(string_table.intern(member_name)),
                token_stream.current_location(),
            )
            .into());
        }

        token_stream.advance();
        return Ok(Vec::new());
    }

    let expectations = expected_type_ids
        .iter()
        .map(|expected_type_id| ParameterExpectation {
            name: None,
            expected_type: ExpectedParameterType::Known(*expected_type_id),
            access_mode: ExpectedAccessMode::Shared,
            requires_reactive_source: false,
            default_value: None,
        })
        .collect::<Vec<_>>();

    let callee_name = string_table.intern(member_name);
    let parsed_arguments = parse_call_arguments_typed_with_expectations(
        token_stream,
        context,
        type_interner,
        string_table,
        &expectations,
        NamedArgumentSyntax::UnsupportedBuiltinMember {
            member_name: Some(callee_name),
        },
    )?;

    let type_check_context = type_interner.type_check_context();

    Ok(resolve_call_arguments(
        CallDiagnosticContext::builtin_member(member_name),
        &parsed_arguments,
        &expectations,
        member_location.to_owned(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
        },
    )?)
}
