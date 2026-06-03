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
    BuiltinTypeConformanceTarget(&'static str),
    Import,
    Export,
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

        TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::DatatypeChar
            if current_item_started_at_statement_boundary(token_stream) =>
        {
            if let Some(type_name) = builtin_conformance_target_name(&current_token.kind)
                && token_stream.current_token_kind() == &TokenKind::Must
            {
                return HeaderFileItem::BuiltinTypeConformanceTarget(type_name);
            }

            HeaderFileItem::StartBodyToken
        }

        TokenKind::Import => HeaderFileItem::Import,

        TokenKind::Export if current_item_started_at_statement_boundary(token_stream) => {
            HeaderFileItem::Export
        }

        TokenKind::Export => HeaderFileItem::StartBodyToken,

        TokenKind::Hash => HeaderFileItem::Hash {
            at_statement_boundary: current_item_started_at_statement_boundary(token_stream),
        },

        TokenKind::TemplateHead => HeaderFileItem::RuntimeTemplate,

        TokenKind::Must | TokenKind::TraitThis => HeaderFileItem::ReservedTraitSyntax,

        TokenKind::Eof => HeaderFileItem::Eof,

        _ => HeaderFileItem::StartBodyToken,
    }
}

fn builtin_conformance_target_name(token_kind: &TokenKind) -> Option<&'static str> {
    match token_kind {
        TokenKind::DatatypeInt => Some("Int"),
        TokenKind::DatatypeFloat => Some("Float"),
        TokenKind::DatatypeBool => Some("Bool"),
        TokenKind::DatatypeString => Some("String"),
        TokenKind::DatatypeChar => Some("Char"),
        _ => None,
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
        // `name must:` starts a trait declaration; `name must TRAIT` starts a conformance declaration.
        TokenKind::Must => true,
        // `Name of T must TRAIT` is a conformance declaration whose target is rejected later
        // as deferred specialized generic evidence.
        TokenKind::Of => starts_specialized_generic_conformance_declaration(token_stream),
        _ => false,
    }
}

/// Detect whether the current `must` token starts a trait declaration rather than conformance.
///
/// WHY: repeated `Type must TRAIT` conformance declarations reuse the target type name and do not
/// shadow it, but repeated `TRAIT must:` declarations are ordinary duplicate headers.
pub(super) fn starts_trait_declaration_after_must(token_stream: &FileTokens) -> bool {
    token_stream.current_token_kind() == &TokenKind::Must
        && matches!(token_stream.peek_next_token(), Some(TokenKind::Colon))
}

pub(super) fn starts_specialized_generic_conformance_declaration(
    token_stream: &FileTokens,
) -> bool {
    if token_stream.current_token_kind() != &TokenKind::Of {
        return false;
    }

    let mut index = token_stream.index;
    while let Some(token) = token_stream.tokens.get(index) {
        match token.kind {
            TokenKind::Must => return true,
            TokenKind::Newline | TokenKind::End | TokenKind::Eof => return false,
            _ => index += 1,
        }
    }

    false
}
