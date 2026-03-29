//! Tests for dev-server error page rendering helpers.

use super::{
    escape_html, format_compiler_messages, render_compiler_error_page, render_runtime_error_page,
};
use crate::compiler_frontend::basic_utility_functions::file_url_from_path;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorLocation, ErrorMetaDataKey,
};
use crate::compiler_frontend::tokenizer::tokens::CharPosition;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_error_page_{prefix}_{unique}"))
}

#[test]
fn escape_html_rewrites_special_characters() {
    let escaped = escape_html(r#"<tag attr="x">Tom & Jerry's</tag>"#);
    assert_eq!(
        escaped,
        "&lt;tag attr=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/tag&gt;"
    );
}

#[test]
fn rendered_runtime_page_includes_version_error_text_and_dark_mode() {
    let page = render_runtime_error_page("Title", "something broke", 14);
    assert!(page.contains("Build Version: 14"));
    assert!(page.contains("something broke"));
    assert!(page.contains("Timestamp (unix):"));
    assert!(page.contains("color-scheme: dark"));
}

#[test]
fn formatted_compiler_messages_include_suggestion_metadata() {
    let mut messages = CompilerMessages::new();
    let mut error = CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("Function Signature Parsing"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Add ':' after return declarations"),
    );
    messages.errors.push(error);

    let formatted = format_compiler_messages(&messages);

    assert!(formatted.contains("stage: Function Signature Parsing"));
    assert!(formatted.contains("help: Add ':' after return declarations"));
}

#[test]
fn compiler_error_page_links_to_project_relative_resolved_source_path() {
    let root = temp_dir("relative_path");
    let source_file = root.join("src/docs/guide.bst");
    fs::create_dir_all(
        source_file
            .parent()
            .expect("source file should have a parent directory"),
    )
    .expect("should create source dir");
    fs::write(&source_file, "broken()\n").expect("should write source file");

    let header_scope = source_file.join("start.header");
    let mut messages = CompilerMessages::new();
    let mut error = CompilerError::new_syntax_error(
        "bad syntax",
        ErrorLocation::new(
            header_scope,
            CharPosition {
                line_number: 1,
                char_column: 4,
            },
            CharPosition {
                line_number: 1,
                char_column: 7,
            },
        ),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("Function Signature Parsing"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Add ':' after return declarations"),
    );
    messages.errors.push(error);

    let page = render_compiler_error_page(&messages, &root, 7);
    let resolved_source_file = fs::canonicalize(&source_file).expect("source file should resolve");
    let expected_href = file_url_from_path(&resolved_source_file, false);

    assert!(page.contains("color-scheme: dark"));
    assert!(page.contains("guide.bst<"));
    println!("EXPECTED! {}", expected_href);
    assert!(page.contains(&format!("href=\"{expected_href}\"")));
    assert!(page.contains("line 2, col 5"));
    assert!(page.contains("Stage: Function Signature Parsing"));
    assert!(page.contains("Help: Add"));
    assert!(page.contains("return declarations"));
    assert!(!page.contains("start.header"));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
