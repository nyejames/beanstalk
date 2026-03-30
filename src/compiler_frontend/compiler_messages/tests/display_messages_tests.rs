use super::{
    format_error_guidance_lines, format_terse_compiler_messages, relative_display_path_from_root,
    resolve_source_file_path, resolved_display_path,
};
use crate::compiler_frontend::basic_utility_functions::normalize_path;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType, SourceLocation,
};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::CharPosition;
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
    let mut error = CompilerError::new_syntax_error("bad syntax", SourceLocation::default());
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
    let error = CompilerError::new_syntax_error("bad syntax", SourceLocation::default());
    let lines = format_error_guidance_lines(&error);
    assert!(lines.is_empty());
}

#[test]
fn guidance_lines_include_replacement_and_location_variants() {
    let mut replacement_error =
        CompilerError::new_syntax_error("bad syntax", SourceLocation::default());
    replacement_error.new_metadata_entry(
        ErrorMetaDataKey::SuggestedReplacement,
        String::from("let value = 1"),
    );

    let mut location_error =
        CompilerError::new_syntax_error("bad syntax", SourceLocation::default());
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
    let error = CompilerError::new_rule_error("bad rule", SourceLocation::default());
    let messages = CompilerMessages::from_error(error, StringTable::new());

    assert_eq!(messages.errors.len(), 1);
    assert!(messages.warnings.is_empty());
    assert_eq!(messages.errors[0].msg, "bad rule");
}

#[test]
fn compiler_error_metadata_and_overrides_are_preserved() {
    let mut string_table = StringTable::new();
    let mut error = CompilerError::new_rule_error("bad rule", SourceLocation::default())
        .with_scope_path(Path::new("project/main.bst"), &mut string_table)
        .with_error_type(ErrorType::Config);
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Rename the config key"),
    );

    assert_eq!(error.error_type, ErrorType::Config);
    assert_eq!(
        error.location.scope.to_path_buf(&string_table),
        PathBuf::from("project/main.bst")
    );
    assert_eq!(
        error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion),
        Some(&String::from("Rename the config key"))
    );
}

#[test]
fn with_scope_path_preserves_existing_span_positions() {
    let mut string_table = StringTable::new();
    let mut error = CompilerError::new_rule_error(
        "bad rule",
        SourceLocation::new(
            crate::compiler_frontend::interned_path::InternedPath::from_path_buf(
                Path::new("old.bst"),
                &mut string_table,
            ),
            crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 4,
                char_column: 7,
            },
            crate::compiler_frontend::tokenizer::tokens::CharPosition {
                line_number: 4,
                char_column: 9,
            },
        ),
    );

    error = error.with_scope_path(Path::new("project/main.bst"), &mut string_table);

    assert_eq!(error.location.start_pos.line_number, 4);
    assert_eq!(error.location.start_pos.char_column, 7);
    assert_eq!(
        error.location.scope.to_path_buf(&string_table),
        PathBuf::from("project/main.bst")
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
    let normalized = normalize_path(Path::new(r"\\?\C:\workspace\main.bst"));
    assert_eq!(normalized, PathBuf::from(r"C:\workspace\main.bst"));
}

#[test]
fn resolve_source_file_path_strips_header_suffix_before_lookup() {
    let root: PathBuf = temp_dir("header_scope");
    let source_file = root.join("main.bst");
    fs::write(&source_file, "#page = []").expect("should write source file");

    let mut string_table = StringTable::new();
    let header_scope = source_file.join("title.header");
    let header_scope = crate::compiler_frontend::interned_path::InternedPath::from_path_buf(
        &header_scope,
        &mut string_table,
    );
    let resolved = resolve_source_file_path(&header_scope, &string_table);

    let expected = fs::canonicalize(&source_file).expect("should canonicalize source file");

    let normalized_resolved = normalize_path(&resolved);
    let normalized_expected = normalize_path(&expected);
    assert_eq!(normalized_resolved, normalized_expected);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn formatted_warning_uses_resolved_source_file_path_for_header_scopes() {
    let root: PathBuf = temp_dir("header_warning_scope");
    let source_file = root.join("main.bst");
    fs::write(&source_file, "#page = []").expect("should write source file");

    let mut string_table = StringTable::new();
    let header_scope = source_file.join("title.header");
    let header_scope = crate::compiler_frontend::interned_path::InternedPath::from_path_buf(
        &header_scope,
        &mut string_table,
    );

    let displayed = resolved_display_path(&header_scope, &string_table);
    assert!(displayed.ends_with("main.bst"));
    assert!(!displayed.contains(".header"));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn terse_messages_emit_single_line_records_without_ascii_formatting() {
    let root: PathBuf = temp_dir("terse_messages");
    let source_file = root.join("main.bst");
    fs::write(&source_file, "value = 1\n").expect("should write source file");

    let mut string_table = StringTable::new();
    let source_scope = InternedPath::from_path_buf(&source_file, &mut string_table);
    let error_location = SourceLocation::new(
        source_scope.clone(),
        CharPosition {
            line_number: 2,
            char_column: 3,
        },
        CharPosition {
            line_number: 2,
            char_column: 8,
        },
    );
    let warning_location = SourceLocation::new(
        source_scope,
        CharPosition {
            line_number: 4,
            char_column: 1,
        },
        CharPosition {
            line_number: 4,
            char_column: 4,
        },
    );

    let mut error = CompilerError::new("Bad\nsyntax | token", error_location, ErrorType::Syntax);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("Header\nParsing"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Remove | newline"),
    );
    let warning = CompilerWarning::new(
        "Unused\nvalue | x",
        warning_location,
        WarningKind::UnusedVariable,
    );
    let messages = CompilerMessages {
        errors: vec![error],
        warnings: vec![warning],
        string_table,
    };

    let lines = format_terse_compiler_messages(&messages);
    assert_eq!(lines.len(), 2);

    let error_line = &lines[0];
    assert!(error_line.starts_with("E|syntax|"));
    assert!(error_line.contains("main.bst"));
    assert!(error_line.contains("|3:4|Bad syntax / token|"));
    assert!(error_line.contains("|help=Remove / newline"));
    assert!(error_line.contains("|stage=Header Parsing"));
    assert!(!error_line.contains('\n'));
    assert!(!error_line.contains("🔥"));
    assert!(!error_line.contains("^"));

    let warning_line = &lines[1];
    assert!(warning_line.starts_with("W|unused_variable|"));
    assert!(warning_line.contains("main.bst"));
    assert!(warning_line.contains("|5:2|Unused value / x"));
    assert!(!warning_line.contains('\n'));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
