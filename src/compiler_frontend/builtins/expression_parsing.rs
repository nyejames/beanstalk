//! Builtin expression parsing helpers.
//!
//! WHAT: parses compiler-owned expression forms such as numeric casts and collection literals.
//! WHY: builtin parsing logic should live with builtin metadata so extending language-owned
//! surfaces does not keep bloating the generic expression parser.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::{
    ExpressionTrailingPolicy, create_expression_with_trailing_newline_policy,
};
use crate::compiler_frontend::ast::statements::collections::new_collection;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type_typed;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidBuiltinCallReason, TypeMismatchContext,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

/// Parses compiler-owned numeric cast forms (`Int(...)`, `Float(...)`).
///
/// WHAT: validates builtin cast syntax and lowers directly into builtin cast expressions.
/// WHY: these casts are language-owned and should not route through user call resolution.
pub(crate) fn parse_builtin_cast_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    let cast_location = token_stream.current_location();
    let cast_kind = token_stream.current_token_kind().to_owned();
    let cast_name = builtin_cast_name(&cast_kind, string_table);
    token_stream.advance();

    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::CastMissingParentheses,
            cast_name,
            token_stream.current_location(),
        )
        .into());
    }

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::CastMissingArgument,
            cast_name,
            token_stream.current_location(),
        )
        .into());
    }

    let mut inferred_type = ExpectedType::Infer;
    let value = create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        type_interner,
        &mut inferred_type,
        value_mode,
        ExpressionTrailingPolicy {
            consume_closing_parenthesis: false,
            skip_trailing_newlines: false,
            allow_boundary_catch: false,
        },
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::CastTooManyArguments,
            cast_name,
            token_stream.current_location(),
        )
        .into());
    }

    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::CastMissingClosingParenthesis,
            cast_name,
            token_stream.current_location(),
        )
        .into());
    }

    token_stream.advance();

    let error_type = resolve_builtin_error_type_typed(context, &cast_location, string_table)?;

    match cast_kind {
        TokenKind::DatatypeInt => Ok(Expression::builtin_int_cast(
            value,
            error_type.type_id,
            type_interner.environment_mut_for_derived_types(),
            cast_location,
        )),
        TokenKind::DatatypeFloat => Ok(Expression::builtin_float_cast(
            value,
            error_type.type_id,
            type_interner.environment_mut_for_derived_types(),
            cast_location,
        )),
        other => Err(CompilerError::compiler_error(format!(
            "Builtin cast parser dispatch mismatch: expected Int/Float token, got '{other:?}'."
        ))
        .into()),
    }
}

fn builtin_cast_name(cast_kind: &TokenKind, string_table: &mut StringTable) -> Option<StringId> {
    match cast_kind {
        TokenKind::DatatypeInt => Some(string_table.intern("Int")),
        TokenKind::DatatypeFloat => Some(string_table.intern("Float")),
        _ => None,
    }
}

/// Parses collection literal expressions (`{...}`) for declared and inferred collection types.
///
/// WHAT: validates that collection literals are used with a compatible expected type.
/// WHY: collection literals are compiler-owned syntax and should be centralized with builtin
/// parsing helpers for consistency and future extension.
pub(crate) fn parse_collection_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &ExpectedType,
    value_mode: &ValueMode,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), ExpressionParseError> {
    let element_type_id = match expected_type {
        ExpectedType::Known(type_id) => {
            let type_environment = type_interner.environment();
            let Some(inner_type_id) = type_environment.collection_element_type(*type_id) else {
                return Err(CompilerDiagnostic::type_mismatch(
                    *type_id,
                    type_environment.builtins().string,
                    TypeMismatchContext::General,
                    token_stream.current_location(),
                )
                .into());
            };
            Some(inner_type_id)
        }

        ExpectedType::Infer => None,
    };

    expression.push(AstNode {
        kind: NodeKind::Rvalue(new_collection(
            token_stream,
            element_type_id,
            context,
            type_interner,
            value_mode,
            string_table,
        )?),
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    });
    Ok(())
}
