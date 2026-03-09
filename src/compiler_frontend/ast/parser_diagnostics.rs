use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{TextLocation, TokenKind};

pub(crate) struct SyntaxDiagnosticConfig {
    pub stage: &'static str,
    pub primary_suggestion: &'static str,
    pub alternative_suggestion: Option<&'static str>,
    pub suggested_insertion: Option<&'static str>,
    pub suggested_location: Option<&'static str>,
}

impl SyntaxDiagnosticConfig {
    pub(crate) fn new(stage: &'static str, primary_suggestion: &'static str) -> Self {
        Self {
            stage,
            primary_suggestion,
            alternative_suggestion: None,
            suggested_insertion: None,
            suggested_location: None,
        }
    }
}

pub(crate) fn syntax_error(
    message: impl Into<String>,
    location: TextLocation,
    config: SyntaxDiagnosticConfig,
    string_table: &StringTable,
) -> CompilerError {
    let mut error =
        CompilerError::new_syntax_error(message, location.to_error_location(string_table));
    error.new_metadata_entry(ErrorMetaDataKey::CompilationStage, config.stage);
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        config.primary_suggestion,
    );

    if let Some(alternative) = config.alternative_suggestion {
        error.new_metadata_entry(ErrorMetaDataKey::AlternativeSuggestion, alternative);
    }

    if let Some(insertion) = config.suggested_insertion {
        error.new_metadata_entry(ErrorMetaDataKey::SuggestedInsertion, insertion);
    }

    if let Some(suggested_location) = config.suggested_location {
        error.new_metadata_entry(ErrorMetaDataKey::SuggestedLocation, suggested_location);
    }

    error
}

pub(crate) fn unexpected_token(
    token: &TokenKind,
    context: &str,
    location: TextLocation,
    config: SyntaxDiagnosticConfig,
    string_table: &StringTable,
) -> CompilerError {
    syntax_error(
        format!("Unexpected token '{token:?}' in {context}."),
        location,
        config,
        string_table,
    )
}
