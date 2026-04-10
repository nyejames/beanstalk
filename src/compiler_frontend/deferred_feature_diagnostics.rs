//! Shared diagnostics for deferred and intentionally unsupported user-facing features.
//!
//! WHAT: provides one helper pattern for deferred language-surface failures and unsupported
//! style-directive failures.
//! WHY: parser/tokenizer callsites should not hand-roll metadata keys and wording patterns.

use crate::compiler_frontend::compiler_errors::{
    CompilerError, ErrorMetaDataKey, ErrorType, SourceLocation,
};
use std::collections::HashMap;

fn standard_metadata(
    compilation_stage: impl Into<String>,
    primary_suggestion: impl Into<String>,
) -> HashMap<ErrorMetaDataKey, String> {
    let mut metadata = HashMap::new();
    metadata.insert(ErrorMetaDataKey::CompilationStage, compilation_stage.into());
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        primary_suggestion.into(),
    );
    metadata
}

/// Build a rule diagnostic for deferred language surfaces.
///
/// WHAT: returns a rule error with standardized metadata keys.
/// WHY: deferred language features should read consistently regardless of parsing stage.
pub(crate) fn deferred_feature_rule_error(
    message: impl Into<String>,
    location: SourceLocation,
    compilation_stage: impl Into<String>,
    primary_suggestion: impl Into<String>,
) -> CompilerError {
    CompilerError::new_rule_error_with_metadata(
        message,
        location,
        standard_metadata(compilation_stage, primary_suggestion),
    )
}

/// Build a syntax diagnostic for unsupported style directives.
///
/// WHAT: reports unknown/unsupported `$directive` usage with deterministic wording.
/// WHY: tokenizer and template-head fallback parsing should produce matching diagnostics.
pub(crate) fn unsupported_style_directive_syntax_error(
    directive_name: &str,
    supported_directives: &str,
    location: SourceLocation,
    compilation_stage: impl Into<String>,
) -> CompilerError {
    CompilerError {
        msg: format!(
            "Style directive '${directive_name}' is unsupported here. Registered directives are {supported_directives}.",
        ),
        location,
        error_type: ErrorType::Syntax,
        metadata: standard_metadata(
            compilation_stage,
            "Use a registered style directive here or register this directive in the active project builder.",
        ),
    }
}
