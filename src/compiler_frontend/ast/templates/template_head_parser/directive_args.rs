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
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateDirectiveReason,
};
use crate::compiler_frontend::numeric_text::parse::materialize_i32;
use crate::compiler_frontend::numeric_text::token::NumericLiteralKind;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

/// Boxed diagnostic result shared by directive-argument parsing helpers.
type DirectiveArgsResult<T> = Result<T, Box<CompilerDiagnostic>>;

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
    directive_name: StringId,
    token_stream: &FileTokens,
) -> DirectiveArgsResult<()> {
    if directive_has_arguments(token_stream) {
        return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
            Some(directive_name),
            InvalidTemplateDirectiveReason::UnexpectedArguments,
            token_stream.current_location(),
        )));
    }
    Ok(())
}

/// Returns an error if the current token is `)`, signalling empty directive
/// parentheses.
pub(crate) fn reject_empty_directive_parens(
    directive_name: StringId,
    token_stream: &FileTokens,
) -> DirectiveArgsResult<()> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
            Some(directive_name),
            InvalidTemplateDirectiveReason::EmptyArguments,
            token_stream.current_location(),
        )));
    }
    Ok(())
}

/// Expects the current token to be `)`. Returns a syntax error with a
/// suggestion if it is not.
pub(crate) fn expect_directive_close_paren(token_stream: &FileTokens) -> DirectiveArgsResult<()> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Ok(());
    }

    let found = token_stream.current_token_kind().to_owned();
    Err(Box::new(CompilerDiagnostic::expected_token(
        TokenKind::CloseParenthesis,
        Some(found),
        token_stream.current_location(),
    )))
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
    directive_name: StringId,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> DirectiveArgsResult<Expression> {
    reject_empty_directive_parens(directive_name, token_stream)?;

    let mut inferred = ExpectedType::Infer;
    let expression = create_expression(
        token_stream,
        context,
        type_interner,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )
    .map_err(CompilerDiagnostic::from)?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return Err(Box::new(CompilerDiagnostic::unexpected_token(
            TokenKind::Comma,
            token_stream.current_location(),
        )));
    }

    expect_directive_close_paren(token_stream)?;

    Ok(expression)
}

/// Parses an optional parenthesized compile-time expression after a directive.
///
/// Returns `Ok(None)` if no `(` follows the directive.
/// Returns `Ok(Some(expression))` if a single expression was parsed.
pub(crate) fn parse_optional_parenthesized_expression(
    directive_name: StringId,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> DirectiveArgsResult<Option<Expression>> {
    if !directive_has_arguments(token_stream) {
        return Ok(None);
    }

    advance_into_directive_arguments(token_stream);
    let expression = parse_single_expression_in_directive_parens(
        directive_name,
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
pub(crate) fn parse_required_parenthesized_expression(
    directive_name: StringId,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> DirectiveArgsResult<Expression> {
    if !directive_has_arguments(token_stream) {
        return Err(Box::new(CompilerDiagnostic::expected_token(
            TokenKind::OpenParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        )));
    }

    advance_into_directive_arguments(token_stream);
    parse_single_expression_in_directive_parens(
        directive_name,
        token_stream,
        context,
        type_interner,
        string_table,
    )
}

// ----------------------------------------------------------------------------
// Slot-target argument parsers (moved from template_slots.rs)
// ----------------------------------------------------------------------------

/// Parses the optional argument to `$slot`: default (no parens), named string,
/// or positive positional integer.
pub(crate) fn parse_optional_slot_target_argument(
    directive_name: StringId,
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> DirectiveArgsResult<SlotKey> {
    if !directive_has_arguments(token_stream) {
        return Ok(SlotKey::Default);
    }

    advance_into_directive_arguments(token_stream);

    let target = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => SlotKey::Named(*name),
        TokenKind::NumericLiteral(token) => {
            if token.kind != NumericLiteralKind::WholeNumber {
                return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                    Some(directive_name),
                    InvalidTemplateDirectiveReason::InvalidSlotTarget,
                    token_stream.current_location(),
                )));
            }

            let index = materialize_i32(token, string_table).map_err(|reason| {
                CompilerDiagnostic::invalid_number_literal(
                    token.source_text,
                    reason,
                    token_stream.current_location(),
                )
            })?;

            if index <= 0 {
                return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                    Some(directive_name),
                    InvalidTemplateDirectiveReason::InvalidSlotTarget,
                    token_stream.current_location(),
                )));
            }

            SlotKey::Positional(index as usize)
        }
        TokenKind::CloseParenthesis => {
            return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                Some(directive_name),
                InvalidTemplateDirectiveReason::EmptyArguments,
                token_stream.current_location(),
            )));
        }
        _ => {
            return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                Some(directive_name),
                InvalidTemplateDirectiveReason::InvalidSlotTarget,
                token_stream.current_location(),
            )));
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(target)
}

/// Parses the required named target argument to `$insert("name")`.
pub(crate) fn parse_required_slot_name_argument(
    directive_name: StringId,
    token_stream: &mut FileTokens,
) -> DirectiveArgsResult<StringId> {
    if !directive_has_arguments(token_stream) {
        return Err(Box::new(CompilerDiagnostic::expected_token(
            TokenKind::OpenParenthesis,
            Some(token_stream.current_token_kind().to_owned()),
            token_stream.current_location(),
        )));
    }

    advance_into_directive_arguments(token_stream);

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::NumericLiteral(_) => {
            return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                Some(directive_name),
                InvalidTemplateDirectiveReason::InvalidInsertTarget,
                token_stream.current_location(),
            )));
        }
        TokenKind::CloseParenthesis => {
            return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                Some(directive_name),
                InvalidTemplateDirectiveReason::EmptyArguments,
                token_stream.current_location(),
            )));
        }
        _ => {
            return Err(Box::new(CompilerDiagnostic::invalid_template_directive(
                Some(directive_name),
                InvalidTemplateDirectiveReason::InvalidInsertTarget,
                token_stream.current_location(),
            )));
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(slot_name)
}
