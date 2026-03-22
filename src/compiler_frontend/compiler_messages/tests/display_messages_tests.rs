use super::format_error_guidance_lines;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation, ErrorMetaDataKey};

#[test]
fn guidance_lines_include_stage_and_suggestions_when_present() {
    let mut error = CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    error.new_metadata_entry(ErrorMetaDataKey::CompilationStage, String::from("Expression Parsing"));
    error.new_metadata_entry(ErrorMetaDataKey::PrimarySuggestion, String::from("Do the thing"));
    error.new_metadata_entry(ErrorMetaDataKey::AlternativeSuggestion, String::from("Try another thing"));
    error.new_metadata_entry(ErrorMetaDataKey::SuggestedInsertion, String::from("->"));
    error.new_metadata_entry(ErrorMetaDataKey::SuggestedLocation, String::from("after token X"));

    let lines = format_error_guidance_lines(&error);

    assert!(
        lines
            .iter()
            .any(|line| line.contains("Stage: Expression Parsing"))
    );
    assert!(lines.iter().any(|line| line.contains("Help: Do the thing")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Alternative: Try another thing"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Suggested insertion: '->' after token X"))
    );
}

#[test]
fn guidance_lines_are_empty_when_metadata_is_missing() {
    let error = CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    let lines = format_error_guidance_lines(&error);
    assert!(lines.is_empty());
}
