//! Token-level `if` header classification for value-producing control flow.
//!
//! WHAT: inspects tokens after `if` to decide whether the header introduces a
//! full match, an inline single-predicate match, or a bool condition.
//! WHY: AST expression parser still rejects bare `if`; classification must happen
//! at the receiver site before routing to the appropriate parser.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::compiler_messages::InvalidControlFlowStatementReason;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::utilities::token_scan::find_expression_end_index;

/// Classified shape of an `if` header at a value-producing receiver.
///
/// WHAT: distinguishes the three syntactic routes after `if`.
/// WHY: loose booleans would require the caller to know the precedence rules
/// between full-match, inline-predicate, and bool-condition detection.
pub(super) enum ValueIfHeaderKind {
    FullMatch,
    InlineSinglePredicate,
    BoolCondition,
}

/// Classifies the token stream immediately after the `if` keyword.
pub(super) fn classify_value_if_header(token_stream: &FileTokens) -> ValueIfHeaderKind {
    if current_if_header_is_full_match(token_stream) {
        return ValueIfHeaderKind::FullMatch;
    }

    if current_if_header_is_inline_single_predicate(token_stream) {
        return ValueIfHeaderKind::InlineSinglePredicate;
    }

    ValueIfHeaderKind::BoolCondition
}

/// Returns `true` when the token stream after `if` is a full value-match header:
/// `if <expr> is:` (colon on the same logical line after `is`).
pub(in crate::compiler_frontend::ast::statements::value_production) fn current_if_header_is_full_match(
    token_stream: &FileTokens,
) -> bool {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length {
        return false;
    }
    if token_stream.tokens[is_index].kind != TokenKind::Is {
        return false;
    }

    token_stream
        .tokens
        .iter()
        .skip(is_index + 1)
        .find(|token| token.kind != TokenKind::Newline)
        .is_some_and(|token| token.kind == TokenKind::Colon)
}

/// Returns `true` when the token stream after `if` is an inline single-predicate
/// match: `if <expr> is <pattern> then ...`.
fn current_if_header_is_inline_single_predicate(token_stream: &FileTokens) -> bool {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length || token_stream.tokens[is_index].kind != TokenKind::Is {
        return false;
    }

    let Some(pattern_index) = next_non_newline_index(token_stream, is_index + 1) else {
        return false;
    };
    if !matches!(
        token_stream.tokens[pattern_index].kind,
        TokenKind::Symbol(_) | TokenKind::TypeParameterBracket
    ) {
        return false;
    }

    token_stream
        .tokens
        .iter()
        .skip(pattern_index + 1)
        .take_while(|token| {
            !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::End | TokenKind::Eof | TokenKind::Colon
            )
        })
        .any(|token| token.kind == TokenKind::Then)
}

/// Detects unsupported optional single-predicate forms and returns the reason.
///
/// WHAT: rejects `if maybe is none then ...` and literal predicates on optionals
/// because inline optional recovery must use present capture (`|value|`).
/// WHY: these diagnostics prevent accidental misuse before parsing proceeds.
pub(super) fn unsupported_optional_single_predicate_reason(
    token_stream: &FileTokens,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
) -> Option<InvalidControlFlowStatementReason> {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length || token_stream.tokens[is_index].kind != TokenKind::Is {
        return None;
    }

    let TokenKind::Symbol(scrutinee_name) = token_stream.current_token_kind() else {
        return None;
    };
    if token_stream.index + 1 != is_index {
        return None;
    }

    let scrutinee_type_id = context.get_reference(scrutinee_name)?.value.type_id;
    type_environment.option_inner_type(scrutinee_type_id)?;

    let pattern_index = next_non_newline_index(token_stream, is_index + 1)?;
    let pattern_token = &token_stream.tokens[pattern_index].kind;

    if matches!(pattern_token, TokenKind::NoneLiteral) {
        return Some(InvalidControlFlowStatementReason::ValueIfOptionNonePredicate);
    }

    if token_is_literal_pattern(pattern_token)
        && header_has_inline_then_after(token_stream, pattern_index + 1)
    {
        return Some(InvalidControlFlowStatementReason::ValueIfOptionLiteralPredicate);
    }

    None
}

/// Finds the index of the next token that is not a newline.
pub(super) fn next_non_newline_index(
    token_stream: &FileTokens,
    start_index: usize,
) -> Option<usize> {
    token_stream
        .tokens
        .iter()
        .enumerate()
        .skip(start_index)
        .find(|(_, token)| token.kind != TokenKind::Newline)
        .map(|(index, _)| index)
}

/// Returns `true` for token kinds that can serve as literal match patterns.
fn token_is_literal_pattern(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::BoolLiteral(_)
    )
}

/// Returns `true` when a `then` token appears on the same logical line after
/// `start_index`, stopping at newline/End/Eof.
fn header_has_inline_then_after(token_stream: &FileTokens, start_index: usize) -> bool {
    token_stream
        .tokens
        .iter()
        .skip(start_index)
        .take_while(|token| {
            !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::End | TokenKind::Eof
            )
        })
        .any(|token| token.kind == TokenKind::Then)
}
