//! Assert statement parsing.
//!
//! WHAT: parses `assert(condition)` and `assert(condition, "message")` as a language-owned
//!       statement intrinsic.
//! WHY: keeping assert out of the ordinary symbol/expression path prevents shadowing,
//!      named arguments, mutable markers, fallible suffixes, and expression-position use.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AssertMessage, AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_until;
use crate::compiler_frontend::ast::expressions::parse_expression_input::{
    ExpressionParseInput, ExpressionParseResources,
};
use crate::compiler_frontend::ast::statements::condition_validation::ensure_boolean_condition;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidBuiltinCallReason, InvalidResultHandlingReason,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::CastTargetContext;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

#[allow(clippy::result_large_err)]
pub(crate) fn parse_assert_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let assert_location = token_stream.current_location();
    let assert_name = string_table.intern("assert");

    token_stream.advance(); // past `assert`

    // Require `(` immediately.
    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::MissingParentheses,
            Some(assert_name),
            token_stream.current_location(),
        ));
    }
    token_stream.advance(); // past `(`

    // Reject `assert()`.
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::MissingArgument,
            Some(assert_name),
            token_stream.current_location(),
        ));
    }
    reject_unsupported_assert_argument_prefix(token_stream, assert_name)?;

    // Parse the condition expression, stopping at the top-level comma or close paren.
    let bool_type_id = type_interner.environment().builtins().bool;
    let mut expected_bool = ExpectedType::Known(bool_type_id);
    let mut cast_target_context = CastTargetContext::None;
    let input = ExpressionParseInput::until(ExpressionParseResources {
        token_stream,
        scope_context: context,
        type_interner,
        expected_type: &mut expected_bool,
        cast_target_context: &mut cast_target_context,
        value_mode: &ValueMode::ImmutableOwned,
        string_table,
    });
    let condition =
        create_expression_until(input, &[TokenKind::Comma, TokenKind::CloseParenthesis])
            .map_err(CompilerDiagnostic::from)?;

    // Validate the condition is Bool using the shared condition diagnostic path.
    ensure_boolean_condition(&condition, &condition.location, type_interner.environment())?;

    // Optional message argument.
    let message = if token_stream.current_token_kind() == &TokenKind::Comma {
        token_stream.advance(); // past `,`

        // Reject trailing comma with no message.
        if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
            return Err(CompilerDiagnostic::invalid_builtin_call(
                InvalidBuiltinCallReason::MissingArgument,
                Some(assert_name),
                token_stream.current_location(),
            ));
        }
        reject_unsupported_assert_argument_prefix(token_stream, assert_name)?;

        // For Alpha, only string slice literals are accepted as messages.
        match token_stream.current_token_kind() {
            TokenKind::StringSliceLiteral(text) => {
                let msg = AssertMessage {
                    text: *text,
                    location: token_stream.current_location(),
                };
                token_stream.advance();
                Some(msg)
            }
            _ => {
                return Err(CompilerDiagnostic::invalid_builtin_call(
                    InvalidBuiltinCallReason::RuntimeMessageExpressionDeferred,
                    Some(assert_name),
                    token_stream.current_location(),
                ));
            }
        }
    } else {
        None
    };

    // Reject extra arguments such as `assert(true, "a", "b")`.
    if token_stream.current_token_kind() == &TokenKind::Comma {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::TooManyArguments,
            Some(assert_name),
            token_stream.current_location(),
        ));
    }

    // Require closing `)`.
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::CloseParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        ));
    }
    token_stream.advance(); // past `)`

    // Reject `assert(...)!` — assert is not a fallible expression.
    if token_stream.current_token_kind() == &TokenKind::Bang {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::NotResultExpression,
            token_stream.current_location(),
        ));
    }

    // Reject `assert(...) catch ...` — assert is not a fallible expression.
    if token_stream.current_token_kind() == &TokenKind::Catch {
        return Err(CompilerDiagnostic::invalid_result_handling(
            InvalidResultHandlingReason::NotResultExpression,
            token_stream.current_location(),
        ));
    }

    ast.push(AstNode {
        kind: NodeKind::Assert { condition, message },
        location: assert_location,
        scope: context.scope.clone(),
    });

    Ok(())
}

#[allow(clippy::result_large_err)]
fn reject_unsupported_assert_argument_prefix(
    token_stream: &FileTokens,
    assert_name: StringId,
) -> Result<(), CompilerDiagnostic> {
    match token_stream.current_token_kind() {
        TokenKind::Mutable => Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::DoesNotAcceptMutableAccess,
            Some(assert_name),
            token_stream.current_location(),
        )),

        TokenKind::Symbol(_) if token_stream.peek_next_token() == Some(&TokenKind::Assign) => {
            Err(CompilerDiagnostic::invalid_builtin_call(
                InvalidBuiltinCallReason::NamedArgumentsNotSupported,
                Some(assert_name),
                token_stream.current_location(),
            ))
        }

        _ => Ok(()),
    }
}
