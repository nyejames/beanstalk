//! Common language-mismatch mistakes in statement position.
//!
//! WHAT: Detects patterns like `// comment`, `fn name(...)`, `let x = ...`,
//! `match value {`, `else if`, and `struct Name { ... }` that users from other
//! languages write when they first encounter Beanstalk.
//!
//! WHY: Statement position is where most structural syntax lives (declarations,
//! control flow, comments), so it is the richest source of language-mismatch errors.

use super::syntax_error_with_suggestion;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

const STATEMENT_STAGE: &str = "AST Construction";

/// Check for common statement-position mistakes before falling back to a generic error.
///
/// Called from the main body dispatch loop or `unexpected_function_body_token_error`
/// when a token does not match any known statement start.
pub(crate) fn check_statement_common_mistake(
    token: &TokenKind,
    token_stream: &FileTokens,
) -> Option<CompilerError> {
    let location = token_stream.current_location();

    match token {
        // `//` is integer division; comments use `--`
        TokenKind::IntDivide => Some(syntax_error_with_suggestion(
            "`//` is integer division. Comments use `--`.",
            location,
            "Replace `//` with `--` for a comment",
            STATEMENT_STAGE,
        )),

        // `!` in statement position (not result handling)
        TokenKind::Bang => Some(syntax_error_with_suggestion(
            "Beanstalk uses `not` for boolean negation, not `!`.",
            location,
            "Replace `!` with `not`",
            STATEMENT_STAGE,
        )),

        _ => None,
    }
}

/// Check if a symbol is a common mistaken keyword from another language.
///
/// WHAT: `fn`, `function`, `def`, `let`, `var`, `const`, `match`, and `struct`
/// are not Beanstalk keywords, but newcomers often type them out of habit.
///
/// Called from `parse_symbol_statement` before treating the symbol as a normal
/// variable reference, declaration, or call.
pub(crate) fn check_mistaken_keyword_symbol(
    symbol_id: StringId,
    token_stream: &FileTokens,
    string_table: &StringTable,
) -> Option<CompilerError> {
    let name = string_table.resolve(symbol_id);
    let location = token_stream.current_location();

    match name {
        "fn" | "function" | "def" => Some(syntax_error_with_suggestion(
            format!("Functions don't use a keyword prefix like '{name}' in Beanstalk."),
            location,
            "Write `name |args| -> Type:` instead",
            STATEMENT_STAGE,
        )),

        "let" | "var" => Some(syntax_error_with_suggestion(
            "Declarations don't use `let` or `var` in Beanstalk.",
            location,
            "Write `name Type = value` for an immutable binding, or `name ~Type = value` for a mutable one",
            STATEMENT_STAGE,
        )),

        "const" => Some(syntax_error_with_suggestion(
            "Constants don't use `const` in Beanstalk.",
            location,
            "Write `#name Type = value` for a module-level constant, or `name Type = value` for a local binding",
            STATEMENT_STAGE,
        )),

        "match" => Some(syntax_error_with_suggestion(
            "Use `if value is:` for pattern matching, not `match`.",
            location,
            "Replace `match value {` with `if value is:` and use `case ... =>` arms",
            STATEMENT_STAGE,
        )),

        "struct" => Some(syntax_error_with_suggestion(
            format!(
                "Structs are declared with `Name = | fields |` in Beanstalk, not with `{name}`."
            ),
            location,
            "Write `Name = | field Type, |` instead of `struct Name { ... }`",
            STATEMENT_STAGE,
        )),

        _ => None,
    }
}
