//! Reserved trait syntax helpers for the frontend.
//!
//! WHAT: centralizes diagnostics for `must` and `This` while the trait system remains
//! intentionally unimplemented.
//! WHY: multiple parser stages need to reject the same reserved keywords with consistent wording
//! and metadata instead of each stage inventing its own fallback error.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::tokenizer::tokens::{SourceLocation, TokenKind};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReservedTraitKeyword {
    Must,
    This,
}

impl ReservedTraitKeyword {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReservedTraitKeyword::Must => "must",
            ReservedTraitKeyword::This => "This",
        }
    }
}

pub(crate) fn reserved_trait_keyword(token_kind: &TokenKind) -> Option<ReservedTraitKeyword> {
    match token_kind {
        TokenKind::Must => Some(ReservedTraitKeyword::Must),
        TokenKind::TraitThis => Some(ReservedTraitKeyword::This),
        _ => None,
    }
}

pub(crate) fn reserved_trait_keyword_error(
    keyword: ReservedTraitKeyword,
    location: SourceLocation,
    compilation_stage: &'static str,
    primary_suggestion: &'static str,
) -> CompilerError {
    deferred_feature_rule_error(
        format!(
            "Keyword '{}' is reserved for traits and is deferred for Alpha.",
            keyword.as_str()
        ),
        location,
        compilation_stage,
        primary_suggestion,
    )
}

pub(crate) fn reserved_trait_declaration_error(location: SourceLocation) -> CompilerError {
    deferred_feature_rule_error(
        "Trait declarations using 'must' are reserved for traits and are deferred for Alpha.",
        location,
        "Header Parsing",
        "Use a normal declaration form until trait declarations are supported.",
    )
}

pub(crate) fn reserved_trait_dispatch_mismatch_error(
    token_kind: &TokenKind,
    location: SourceLocation,
    compilation_stage: &'static str,
    parser_context: &'static str,
) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("This indicates parser dispatch drift. Please report this compiler bug."),
    );

    CompilerError {
        msg: format!(
            "Reserved trait token dispatch mismatch in {parser_context}: {token_kind:?}"
        ),
        location,
        error_type: ErrorType::Compiler,
        metadata,
    }
}
