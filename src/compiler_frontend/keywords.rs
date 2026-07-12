//! Frontend keyword and identifier policy.
//!
//! WHAT: owns the exact keyword-to-token mapping used by lexing and the identifier
//! validation helpers shared with path/import parsing.
//! WHY: keyword policy is user-visible and must not drift between the tokenizer,
//! import alias validation, and reserved-name diagnostics.

use crate::compiler_frontend::tokenizer::tokens::TokenKind;

/// Keywords that may not be shadowed by identifiers after case folding and
/// stripping leading underscores.
pub(crate) const RESERVED_KEYWORD_SHADOWS: [&str; 37] = [
    "import", "export", "if", "return", "yield", "else", "block", "checked", "async", "cast", "as",
    "copy", "type", "of", "must", "this", "catch", "then", "loop", "to", "by", "break", "continue",
    "is", "not", "and", "or", "true", "false", "none", "fn", "float", "int", "string", "bool",
    "char", "assert",
];

/// Returns the tokenizer token kind for an exact source keyword spelling.
pub(crate) fn keyword_token_kind(text: &str) -> Option<TokenKind> {
    match text {
        "import" => Some(TokenKind::Import),
        // Module-root API marker for the strict `export:` block; exposes declarations through the
        // module's public export surface.
        "export" => Some(TokenKind::Export),

        // Control flow
        "if" => Some(TokenKind::If),
        "return" => Some(TokenKind::Return),
        "catch" => Some(TokenKind::Catch),
        "then" => Some(TokenKind::Then),
        "else" => Some(TokenKind::Else),
        "block" => Some(TokenKind::Block),
        "checked" => Some(TokenKind::Checked),
        "cast" => Some(TokenKind::Cast),
        "as" => Some(TokenKind::As),
        "type" => Some(TokenKind::Type),
        "of" => Some(TokenKind::Of),

        // Reserved trait and receiver syntax
        "must" => Some(TokenKind::Must),
        "this" => Some(TokenKind::This),
        "This" => Some(TokenKind::TraitThis),

        // Deferred async syntax
        "async" => Some(TokenKind::Async),
        "yield" => Some(TokenKind::Yield),

        // Loops
        "loop" => Some(TokenKind::Loop),
        "to" => Some(TokenKind::ExclusiveRange),
        "by" => Some(TokenKind::By),
        "break" => Some(TokenKind::Break),
        "continue" => Some(TokenKind::Continue),

        // Logical operators
        "is" => Some(TokenKind::Is),
        "not" => Some(TokenKind::Not),
        "and" => Some(TokenKind::And),
        "or" => Some(TokenKind::Or),

        // Literals and builtin type spellings
        "true" => Some(TokenKind::BoolLiteral(true)),
        "True" => Some(TokenKind::DatatypeTrue),
        "false" => Some(TokenKind::BoolLiteral(false)),
        "False" => Some(TokenKind::DatatypeFalse),
        "Float" => Some(TokenKind::DatatypeFloat),
        "Int" => Some(TokenKind::DatatypeInt),
        "String" => Some(TokenKind::DatatypeString),
        "Bool" => Some(TokenKind::DatatypeBool),
        "Char" => Some(TokenKind::DatatypeChar),
        "None" => Some(TokenKind::DatatypeNone),
        "none" => Some(TokenKind::NoneLiteral),

        // Memory/access syntax
        "copy" => Some(TokenKind::Copy),

        // Assertion statement intrinsic
        "assert" => Some(TokenKind::Assert),

        _ => None,
    }
}

/// Returns the compound token for keyword forms that require an attached `!`.
///
/// WHAT: `return!` and `cast!` are lexical forms, not a keyword followed by a
///       whitespace-sensitive postfix operator.
/// WHY: keeping attachment in tokenization prevents AST parsing from having to
///      reconstruct source adjacency from locations.
pub(crate) fn attached_bang_keyword_token_kind(text: &str) -> Option<TokenKind> {
    match text {
        "return" => Some(TokenKind::ReturnBang),
        "cast" => Some(TokenKind::CastBang),
        _ => None,
    }
}

/// True when `text` is an exact keyword spelling that lexes to a dedicated token.
pub(crate) fn is_keyword(text: &str) -> bool {
    keyword_token_kind(text).is_some()
}

/// True when a character can appear after the first character of an identifier.
pub(crate) fn is_identifier_continue(char: char) -> bool {
    char.is_alphanumeric() || char == '_'
}

/// True when a string is a source-level identifier spelling.
pub(crate) fn is_valid_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|char| char.is_alphabetic() || char == '_')
        && text.chars().all(is_identifier_continue)
}
