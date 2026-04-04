//! Reserved trait syntax helpers for the frontend.
//!
//! WHAT: centralizes diagnostics for `must` and `This` while the trait system remains
//! intentionally unimplemented.
//! WHY: multiple parser stages need to reject the same reserved keywords with consistent wording
//! and metadata instead of each stage inventing its own fallback error.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
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
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        compilation_stage.to_owned(),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        primary_suggestion.to_owned(),
    );

    CompilerError::new_rule_error_with_metadata(
        format!(
            "'{}' is reserved for traits and is not implemented yet in Alpha.",
            keyword.as_str(),
        ),
        location,
        metadata,
    )
}

pub(crate) fn reserved_trait_declaration_error(location: SourceLocation) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        String::from("Header Parsing"),
    );
    metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from(
            "Remove 'must' for now or rename the declaration until traits are implemented",
        ),
    );

    CompilerError::new_rule_error_with_metadata(
        "Trait declarations using 'must' are reserved for traits and are not implemented yet in Alpha.",
        location,
        metadata,
    )
}
