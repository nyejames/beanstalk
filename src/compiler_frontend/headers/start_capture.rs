//! Implicit entry-start body capture.
//!
//! WHAT: collects non-header top-level tokens into the module entry file's implicit `start` body.
//! WHY: only the entry file executes top-level runtime code; non-entry executable code must be
//! rejected before AST lowering.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token};

pub(super) fn push_runtime_template_tokens_to_start_function(
    opening_template_token: Token,
    token_stream: &mut FileTokens,
    start_function_body: &mut Vec<Token>,
) -> Result<(), CompilerError> {
    start_function_body.push(opening_template_token);

    crate::compiler_frontend::token_scan::consume_balanced_template_region(
        token_stream,
        |token, _token_kind| {
            start_function_body.push(token);
        },
        |location| {
            CompilerError::new_rule_error(
                "Unexpected end of file while parsing top-level runtime template. Missing ']' to close the template.",
                location,
            )
        },
    )
    .map_err(|mut error| {
        error.new_metadata_entry(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
            String::from("Close the template with ']'"),
        );
        error.new_metadata_entry(
            crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::SuggestedInsertion,
            String::from("]"),
        );
        error
    })
}
