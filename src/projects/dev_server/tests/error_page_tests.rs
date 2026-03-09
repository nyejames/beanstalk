//! Tests for dev-server error page rendering helpers.

use super::{escape_html, format_compiler_messages, render_runtime_error_page};
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorLocation, ErrorMetaDataKey,
};

#[test]
fn escape_html_rewrites_special_characters() {
    let escaped = escape_html(r#"<tag attr="x">Tom & Jerry's</tag>"#);
    assert_eq!(
        escaped,
        "&lt;tag attr=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/tag&gt;"
    );
}

#[test]
fn rendered_page_includes_version_and_error_text() {
    let page = render_runtime_error_page("Title", "something broke", 14);
    assert!(page.contains("Build Version: 14"));
    assert!(page.contains("something broke"));
    assert!(page.contains("Timestamp (unix):"));
}

#[test]
fn formatted_compiler_messages_include_suggestion_metadata() {
    let mut messages = CompilerMessages::new();
    let mut error = CompilerError::new_syntax_error("bad syntax", ErrorLocation::default());
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "Function Signature Parsing",
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Add ':' after return declarations",
    );
    messages.errors.push(error);

    let formatted = format_compiler_messages(&messages);

    assert!(formatted.contains("stage: Function Signature Parsing"));
    assert!(formatted.contains("help: Add ':' after return declarations"));
}
