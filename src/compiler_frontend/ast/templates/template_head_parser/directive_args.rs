//! Centralized template-head directive argument parsing.
//!
//! WHAT:
//! - Shared helpers for parsing parenthesized arguments after `$directive` tokens.
//! - Common validation for empty parens, missing close-paren, extra commas, and
//!   compile-time constness.
//!
//! WHY:
//! - Directive argument syntax was duplicated across `$children`, handler styles,
//!   `$slot`, `$insert`, and `$code`. Centralizing it eliminates drift and makes
//!   new directives easier to add correctly.
//!
//! ## Ownership boundary
//!
//! - **This module** owns token-level syntax: `(` detection, paren balancing,
//!   single-expression parsing, string-literal extraction.
//! - **Directive modules** own semantic validation: type checking, compile-time
//!   restrictions, and normalization into template/style state.
//! - **Slot modules** own slot schema and composition; they do not parse tokens.

#![allow(clippy::result_large_err)]

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

/// Returns true if the next token after the current directive is `(`.
pub(crate) fn directive_has_arguments(token_stream: &FileTokens) -> bool {
    token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
}

/// Advances the token stream from the directive token past `(` into the
/// first argument position.
///
/// Precondition: `directive_has_arguments` returned `true`.
pub(crate) fn advance_into_directive_arguments(token_stream: &mut FileTokens) {
    token_stream.advance(); // past directive token
    token_stream.advance(); // past '('
}

/// Rejects parenthesized arguments for directives that do not accept them.
#[allow(clippy::needless_return)]
pub(crate) fn reject_unexpected_directive_arguments(
    token_stream: &FileTokens,
) -> Result<(), CompilerDiagnostic> {
    if directive_has_arguments(token_stream) {
        return Err(CompilerDiagnostic::unexpected_token(
            TokenKind::OpenParenthesis,
            token_stream.current_location(),
        ));
    }
    Ok(())
}

/// Returns an error if the current token is `)`, signalling empty directive
/// parentheses.
#[allow(clippy::needless_return)]
pub(crate) fn reject_empty_directive_parens(
    token_stream: &FileTokens,
) -> Result<(), CompilerDiagnostic> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Err(CompilerDiagnostic::unexpected_token(
            TokenKind::CloseParenthesis,
            token_stream.current_location(),
        ));
    }
    Ok(())
}

/// Expects the current token to be `)`. Returns a syntax error with a
/// suggestion if it is not.
#[allow(clippy::needless_return)]
pub(crate) fn expect_directive_close_paren(
    token_stream: &FileTokens,
) -> Result<(), CompilerDiagnostic> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Ok(());
    }

    let found = token_stream.current_token_kind().to_owned();
    return Err(CompilerDiagnostic::expected_token(
        TokenKind::CloseParenthesis,
        Some(found),
        token_stream.current_location(),
    ));
}

/// Parses a single compile-time expression inside already-opened directive
/// parentheses.
///
/// The caller must have already advanced past `(` (e.g. via
/// `advance_into_directive_arguments`). On success the parser is positioned
/// at the closing `)` token.
///
/// Performs generic validation:
/// - rejects empty parentheses
/// - rejects extra comma-separated arguments
/// - expects `)` after the expression
#[allow(clippy::needless_return)]
fn parse_single_expression_in_directive_parens(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    reject_empty_directive_parens(token_stream)?;

    let mut inferred = ExpectedType::Infer;
    let expression = create_expression(
        token_stream,
        context,
        type_interner,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return Err(CompilerDiagnostic::unexpected_token(
            TokenKind::Comma,
            token_stream.current_location(),
        ));
    }

    expect_directive_close_paren(token_stream)?;

    Ok(expression)
}

/// Parses an optional parenthesized compile-time expression after a directive.
///
/// Returns `Ok(None)` if no `(` follows the directive.
/// Returns `Ok(Some(expression))` if a single expression was parsed.
#[allow(clippy::needless_return)]
pub(crate) fn parse_optional_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerDiagnostic> {
    if !directive_has_arguments(token_stream) {
        return Ok(None);
    }

    advance_into_directive_arguments(token_stream);
    let expression = parse_single_expression_in_directive_parens(
        token_stream,
        context,
        type_interner,
        string_table,
    )?;
    Ok(Some(expression))
}

/// Parses a required parenthesized compile-time expression after a directive.
///
/// Returns an error if no `(` follows the directive.
#[allow(clippy::needless_return)]
pub(crate) fn parse_required_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    if !directive_has_arguments(token_stream) {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::OpenParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        ));
    }

    advance_into_directive_arguments(token_stream);
    parse_single_expression_in_directive_parens(token_stream, context, type_interner, string_table)
}

// ----------------------------------------------------------------------------
// Slot-target argument parsers (moved from template_slots.rs)
// ----------------------------------------------------------------------------

/// Parses the optional argument to `$slot`: default (no parens), named string,
/// or positive positional integer.
#[allow(clippy::needless_return)]
pub(crate) fn parse_optional_slot_target_argument(
    token_stream: &mut FileTokens,
) -> Result<SlotKey, CompilerDiagnostic> {
    if !directive_has_arguments(token_stream) {
        return Ok(SlotKey::Default);
    }

    advance_into_directive_arguments(token_stream);

    let target = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => SlotKey::Named(*name),
        TokenKind::IntLiteral(index) => {
            if *index <= 0 {
                return Err(CompilerDiagnostic::unexpected_token(
                    token_stream.current_token_kind().to_owned(),
                    token_stream.current_location(),
                ));
            }
            SlotKey::Positional(*index as usize)
        }
        TokenKind::CloseParenthesis => {
            return Err(CompilerDiagnostic::unexpected_token(
                TokenKind::CloseParenthesis,
                token_stream.current_location(),
            ));
        }
        _ => {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            ));
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(target)
}

/// Parses the required named target argument to `$insert("name")`.
#[allow(clippy::needless_return)]
pub(crate) fn parse_required_slot_name_argument(
    token_stream: &mut FileTokens,
) -> Result<StringId, CompilerDiagnostic> {
    if !directive_has_arguments(token_stream) {
        return Err(CompilerDiagnostic::expected_token(
            TokenKind::OpenParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        ));
    }

    advance_into_directive_arguments(token_stream);

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::IntLiteral(_) => {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            ));
        }
        TokenKind::CloseParenthesis => {
            return Err(CompilerDiagnostic::unexpected_token(
                TokenKind::CloseParenthesis,
                token_stream.current_location(),
            ));
        }
        _ => {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            ));
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(slot_name)
}
