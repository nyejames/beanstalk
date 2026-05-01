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

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

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
pub(crate) fn reject_unexpected_directive_arguments(
    token_stream: &FileTokens,
    directive_name: &str,
) -> Result<(), CompilerError> {
    if directive_has_arguments(token_stream) {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            token_stream.current_location()
        );
    }
    Ok(())
}

/// Returns an error if the current token is `)`, signalling empty directive
/// parentheses.
pub(crate) fn reject_empty_directive_parens(
    token_stream: &FileTokens,
) -> Result<(), CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Directive arguments cannot be empty. Remove the parentheses or provide an argument.",
            token_stream.current_location()
        );
    }
    Ok(())
}

/// Expects the current token to be `)`. Returns a syntax error with a
/// suggestion if it is not.
pub(crate) fn expect_directive_close_paren(token_stream: &FileTokens) -> Result<(), CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Ok(());
    }

    return_syntax_error!(
        "Expected ')' after directive argument.",
        token_stream.current_location(),
        {
            SuggestedInsertion => ")",
        }
    );
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
fn parse_single_expression_in_directive_parens(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    reject_empty_directive_parens(token_stream)?;

    let mut inferred = DataType::Inferred;
    let expression = create_expression(
        token_stream,
        context,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            "Directive arguments do not support multiple values.",
            token_stream.current_location()
        );
    }

    expect_directive_close_paren(token_stream)?;

    Ok(expression)
}

/// Parses an optional parenthesized compile-time expression after a directive.
///
/// Returns `Ok(None)` if no `(` follows the directive.
/// Returns `Ok(Some(expression))` if a single expression was parsed.
pub(crate) fn parse_optional_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return Ok(None);
    }

    advance_into_directive_arguments(token_stream);
    let expression =
        parse_single_expression_in_directive_parens(token_stream, context, string_table)?;
    Ok(Some(expression))
}

/// Parses a required parenthesized compile-time expression after a directive.
///
/// Returns an error if no `(` follows the directive.
pub(crate) fn parse_required_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return_syntax_error!(
            "This directive requires a parenthesized argument.",
            token_stream.current_location()
        );
    }

    advance_into_directive_arguments(token_stream);
    parse_single_expression_in_directive_parens(token_stream, context, string_table)
}

// ----------------------------------------------------------------------------
// Slot-target argument parsers (moved from template_slots.rs)
// ----------------------------------------------------------------------------

/// Parses the optional argument to `$slot`: default (no parens), named string,
/// or positive positional integer.
pub(crate) fn parse_optional_slot_target_argument(
    token_stream: &mut FileTokens,
) -> Result<SlotKey, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return Ok(SlotKey::Default);
    }

    advance_into_directive_arguments(token_stream);

    let target = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => SlotKey::Named(*name),
        TokenKind::IntLiteral(index) => {
            if *index <= 0 {
                return_syntax_error!(
                    format!("'$slot({index})' is invalid. Positional slots start at 1."),
                    token_stream.current_location()
                );
            }
            SlotKey::Positional(*index as usize)
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$slot()' cannot use empty parentheses. Use '$slot' for default, a quoted name like '$slot(\"style\")', or a positive integer like '$slot(1)'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$slot(...)' only accepts a quoted string literal name or a positive integer.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(target)
}

/// Parses the required named target argument to `$insert("name")`.
pub(crate) fn parse_required_slot_name_argument(
    token_stream: &mut FileTokens,
) -> Result<StringId, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return_syntax_error!(
            "'$insert' requires a quoted named target like '$insert(\"style\")'.",
            token_stream.current_location()
        );
    }

    advance_into_directive_arguments(token_stream);

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::IntLiteral(_) => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$insert()' cannot use empty parentheses. Use quoted names like '$insert(\"style\")'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(slot_name)
}
