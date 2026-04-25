//! Helpful syntax-error detection for common language-mismatch mistakes.
//!
//! WHAT: Detects patterns that new Beanstalk users write when bringing habits from
//! C, Rust, Python, JavaScript, and other languages. These are not generic syntax
//! errors — they are specific, actionable mistakes that deserve targeted guidance.
//!
//! WHY: Centralizing this logic keeps the parser dispatch loops focused on grammar
//! and makes it easy to add new "did you mean" hints without bloating every parse
//! file. Each position (expression, statement, signature) gets its own module so
//! ownership and context requirements stay explicit.

pub(crate) mod expression_position;
pub(crate) mod signature_position;
pub(crate) mod statement_position;

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[cfg(test)]
mod tests;

/// Build a syntax error with a primary suggestion and compilation stage metadata.
///
/// WHAT: small shared helper so every mistake detector doesn't repeat the same
/// `new_metadata_entry` boilerplate.
/// WHY: keeps error construction readable and consistent.
pub(crate) fn syntax_error_with_suggestion(
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: impl Into<String>,
    stage: &str,
) -> CompilerError {
    let mut error = CompilerError::new_syntax_error(message, location);
    error.new_metadata_entry(ErrorMetaDataKey::CompilationStage, String::from(stage));
    error.new_metadata_entry(ErrorMetaDataKey::PrimarySuggestion, suggestion.into());
    error
}
