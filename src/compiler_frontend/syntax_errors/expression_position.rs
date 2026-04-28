//! Common language-mismatch mistakes in expression position.
//!
//! WHAT: Detects patterns like `==`, `!=`, `&&`, `||`, `!expr`, `&expr` that
//! users from C-family languages write when they first encounter Beanstalk.
//!
//! WHY: These are unambiguous syntax errors at the token level. Catching them
//! early with specific guidance prevents confusion before generic "invalid token"
//! messages waste the user's time.

use super::syntax_error_with_suggestion;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

const EXPRESSION_STAGE: &str = "Expression Parsing";

/// Check for common expression-position mistakes before falling back to a generic error.
///
/// WHAT: inspects the current token (and sometimes the next token) for patterns
/// that are valid in other languages but not in Beanstalk.
///
/// Returns `Some(error)` when a known mistake is detected, `None` otherwise.
pub(crate) fn check_expression_common_mistake(
    token_stream: &FileTokens,
    expression_is_empty: bool,
) -> Option<CompilerError> {
    let current = token_stream.current_token_kind();
    let next = token_stream.peek_next_token();
    let location = token_stream.current_location();

    match current {
        // `==`  →  `is`
        TokenKind::Assign if next == Some(&TokenKind::Assign) => {
            Some(syntax_error_with_suggestion(
                "Beanstalk uses `is` for equality, not `==`.",
                location,
                "Replace `==` with `is`",
                EXPRESSION_STAGE,
            ))
        }

        // `!=`  →  `is not`
        TokenKind::Bang if next == Some(&TokenKind::Assign) => Some(syntax_error_with_suggestion(
            "Beanstalk uses `is not` for inequality, not `!=`.",
            location,
            "Replace `!=` with `is not`",
            EXPRESSION_STAGE,
        )),

        // `&&`  →  `and`
        TokenKind::Ampersand if next == Some(&TokenKind::Ampersand) => {
            Some(syntax_error_with_suggestion(
                "Beanstalk uses `and` for logical conjunction, not `&&`.",
                location,
                "Replace `&&` with `and`",
                EXPRESSION_STAGE,
            ))
        }

        // `||`  →  `or`
        TokenKind::TypeParameterBracket if next == Some(&TokenKind::TypeParameterBracket) => {
            Some(syntax_error_with_suggestion(
                "Beanstalk uses `or` for logical disjunction, not `||`.",
                location,
                "Replace `||` with `or`",
                EXPRESSION_STAGE,
            ))
        }

        // `!` used as boolean negation (not result handling)
        // Result handling `!` is parsed as a postfix suffix after the primary expression,
        // so encountering `Bang` at the start of an operand or after an operator means
        // the user is trying to use it as unary negation.
        TokenKind::Bang if expression_is_empty => Some(syntax_error_with_suggestion(
            "Beanstalk uses `not` for boolean negation, not `!`.",
            location,
            "Replace `!` with `not`",
            EXPRESSION_STAGE,
        )),

        // Single `=` in expression position where it is not valid.
        // `=` is only valid in declarations and assignments (statement position).
        TokenKind::Assign => Some(syntax_error_with_suggestion(
            "Use `is` for comparison. `=` is for declarations and assignments.",
            location,
            "Replace `=` with `is` for equality, or move the assignment to statement position",
            EXPRESSION_STAGE,
        )),

        // Single `&` in expression position (likely Rust borrow attempt)
        TokenKind::Ampersand if expression_is_empty => Some(syntax_error_with_suggestion(
            "`&` marks inclusive ranges in Beanstalk. Borrowing is implicit; use `~` at call sites for mutation.",
            location,
            "Remove `&` — shared borrows are automatic. For mutation, prefix the place with `~` at the call site.",
            EXPRESSION_STAGE,
        )),

        // `as` outside its three supported domains
        TokenKind::As => Some(syntax_error_with_suggestion(
            "`as` is not a cast operator. It is only valid in type aliases, import clauses, and choice payload patterns.",
            location,
            "Use builtin casts such as Int(value) where supported, or use `as` only in a supported renaming context",
            EXPRESSION_STAGE,
        )),

        _ => None,
    }
}
