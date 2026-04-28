//! Common language-mismatch mistakes in declaration/signature position.
//!
//! WHAT: Detects patterns like `name(a, b)` and `name(a: Int)` that users from
//! C-family languages write when declaring functions or structs in Beanstalk,
//! and misplaced `as` in variable declarations.
//!
//! WHY: Parameter and field delimiters are one of the most visually striking
//! differences between Beanstalk and other languages, so they deserve targeted
//! guidance at the point of failure. `as` is also rejected here because it is
//! called from both signature-members and body-local declaration parsers.

use super::syntax_error_with_suggestion;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

const SIGNATURE_STAGE: &str = "Signature Parsing";

/// Check for common signature-position mistakes before falling back to a generic error.
///
/// Called from declaration-shell and signature-members parsers when an
/// unexpected token appears while parsing parameter lists or struct fields.
pub(crate) fn check_signature_common_mistake(token_stream: &FileTokens) -> Option<CompilerError> {
    let current = token_stream.current_token_kind();
    let location = token_stream.current_location();

    match current {
        // `(` where `|` is expected for parameters/fields
        TokenKind::OpenParenthesis => Some(syntax_error_with_suggestion(
            "Parameters and struct fields are delimited with `|`, not `()`.",
            location,
            "Replace `(` with `|` and `)` with `|`",
            SIGNATURE_STAGE,
        )),

        // `as` is not valid in parameter/field or declaration position
        TokenKind::As => Some(syntax_error_with_suggestion(
            "`as` is not valid here. It is only supported in type aliases, import clauses, and choice payload patterns.",
            location,
            "Remove `as` or use it only in a supported renaming context",
            SIGNATURE_STAGE,
        )),

        _ => None,
    }
}
