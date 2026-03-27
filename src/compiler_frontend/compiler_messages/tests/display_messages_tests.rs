use super::{
    format_error_guidance_lines, normalize_display_path, relative_display_path_from_root,
    resolve_source_file_path,
};
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorLocation, ErrorMetaDataKey, ErrorType,
};
use std::fs;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "beanstalk_display_messages_{name}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("should create temp test dir");
    dir
}

#[test]
fn guidance_lines_include_stage_and_suggestions_when_present() {
    let mut error = CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("Expression Parsing"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Do the thing"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::AlternativeSuggestion,
        String::from("Try another thing"),
    );
    error.new_metadata_entry(ErrorMetaDataKey::SuggestedInsertion, String::from("->"));
    error.new_metadata_entry(
        ErrorMetaDataKey::SuggestedLocation,
        String::from("after token X"),
    );

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

#[test]
fn guidance_lines_include_replacement_and_location_variants() {
    let mut replacement_error =
        CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    replacement_error.new_metadata_entry(
        ErrorMetaDataKey::SuggestedReplacement,
        String::from("let value = 1"),
    );

    let mut location_error =
        CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    location_error.new_metadata_entry(
        ErrorMetaDataKey::SuggestedLocation,
        String::from("before the closing ')'"),
    );

    let replacement_lines = format_error_guidance_lines(&replacement_error);
    let location_lines = format_error_guidance_lines(&location_error);

    assert!(
        replacement_lines
            .iter()
            .any(|line| line.contains("Suggested replacement: let value = 1"))
    );
    assert!(
        location_lines
            .iter()
            .any(|line| line.contains("Suggested location: before the closing ')'"))
    );
}

#[test]
fn compiler_messages_from_error_wraps_one_error_without_warnings() {
    let error = CompilerError::new_rule_error("bad rule", ErrorLocation::default());
    let messages = CompilerMessages::from_error(error);

    assert_eq!(messages.errors.len(), 1);
    assert!(messages.warnings.is_empty());
    assert_eq!(messages.errors[0].msg, "bad rule");
}

#[test]
fn compiler_error_metadata_and_overrides_are_preserved() {
    let mut error = CompilerError::new_rule_error("bad rule", ErrorLocation::default())
        .with_file_path(PathBuf::from("project/main.bst"))
        .with_error_type(ErrorType::Config);
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Rename the config key"),
    );

    assert_eq!(error.error_type, ErrorType::Config);
    assert_eq!(error.location.scope, PathBuf::from("project/main.bst"));
    assert_eq!(
        error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion),
        Some(&String::from("Rename the config key"))
    );
}

#[test]
fn relative_display_path_strips_root_prefix() {
    let root = Path::new("/workspace/project");
    let scope = Path::new("/workspace/project/src/main.bst");

    let relative = relative_display_path_from_root(scope, root);

    assert_eq!(relative, "src/main.bst");
}

#[test]
fn normalize_display_path_strips_windows_extended_prefix() {
    let normalized = normalize_display_path(Path::new(r"\\?\C:\workspace\main.bst"));
    assert_eq!(normalized, PathBuf::from(r"C:\workspace\main.bst"));
}

#[test]
fn resolve_source_file_path_strips_header_suffix_before_lookup() {
    let root = temp_dir("header_scope");
    let source_file = root.join("main.bst");
    fs::write(&source_file, "#page = []").expect("should write source file");

    let header_scope = source_file.join("title.header");
    let resolved = resolve_source_file_path(&header_scope);
    let expected = fs::canonicalize(&source_file).expect("should canonicalize source file");

    assert_eq!(resolved, expected);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
