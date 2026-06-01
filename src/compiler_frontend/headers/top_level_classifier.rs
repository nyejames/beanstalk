//! Shallow top-level token classification for header file parsing.
//!
//! WHAT: classifies the already-read token at a file boundary into the next header-parser action.
//! WHY: declaration parsing, import parsing, and runtime-body validation have separate owners; this
//! module only answers which branch the per-file parser should try next.

use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::line_scanning::find_top_level_fat_arrow_on_line;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};

pub(super) enum HeaderFileItem {
    Symbol(StringId),
    Import,
    Hash { at_statement_boundary: bool },
    RuntimeTemplate,
    ReservedTraitSyntax,
    Eof,
    StartBodyToken,
}

pub(super) fn classify_current_item(
    token_stream: &FileTokens,
    current_token: &Token,
) -> HeaderFileItem {
    match current_token.kind {
        TokenKind::Symbol(name_id) if current_item_started_at_statement_boundary(token_stream) => {
            HeaderFileItem::Symbol(name_id)
        }

        TokenKind::Symbol(_) => HeaderFileItem::StartBodyToken,

        TokenKind::Import => HeaderFileItem::Import,

        TokenKind::Hash => HeaderFileItem::Hash {
            at_statement_boundary: current_item_started_at_statement_boundary(token_stream),
        },

        TokenKind::TemplateHead => HeaderFileItem::RuntimeTemplate,

        TokenKind::Must | TokenKind::TraitThis => HeaderFileItem::ReservedTraitSyntax,

        TokenKind::Eof => HeaderFileItem::Eof,

        _ => HeaderFileItem::StartBodyToken,
    }
}

fn current_item_started_at_statement_boundary(token_stream: &FileTokens) -> bool {
    token_stream
        .tokens
        .get(token_stream.index.saturating_sub(2))
        .map(|previous_token| {
            matches!(
                previous_token.kind,
                TokenKind::ModuleStart | TokenKind::Newline | TokenKind::End
            )
        })
        .unwrap_or(true)
}

/// Detect whether a repeated top-level symbol is starting another header declaration.
/// Already in the context of parsing a variable name that exists in this scope.
///
/// WHAT: peeks at the token sequence immediately after an already-seen symbol name.
/// WHY: duplicate header declarations must fail during header parsing instead of being
///      misclassified as references inside the implicit start function.
pub(super) fn starts_duplicate_top_level_header_declaration(token_stream: &FileTokens) -> bool {
    // Qualified match arms such as `Status::Ready => ...` are executable start-body
    // syntax, not a second top-level `Status :: ...` declaration. Header splitting
    // only needs to keep these tokens with the implicit start body; AST owns the
    // actual match-pattern validation.
    if token_stream.current_token_kind() == &TokenKind::DoubleColon
        && find_top_level_fat_arrow_on_line(token_stream, token_stream.index).is_some()
    {
        return false;
    }

    match token_stream.current_token_kind() {
        // `name |...|` starts a function signature.
        TokenKind::TypeParameterBracket => true,
        // `name type T ...` starts a generic function/struct/choice declaration.
        TokenKind::Type => true,
        // `name = |...|` starts a struct declaration.
        TokenKind::Assign => matches!(
            token_stream.peek_next_token(),
            Some(TokenKind::TypeParameterBracket)
        ),
        // `name :: ...` starts a choice declaration.
        TokenKind::DoubleColon => true,
        // `name as ...` starts a type alias declaration.
        TokenKind::As => true,
        // `name #= ...` or `name #Type = ...` starts a compile-time constant declaration.
        TokenKind::Hash => true,
        _ => false,
    }
}
