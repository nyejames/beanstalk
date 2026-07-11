//! Tests for shared HTML shell rendering.

use super::*;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::page_metadata::HtmlPageMetadata;
use crate::projects::html_project::tests::test_support::{
    assert_fragment_before_body_close, assert_has_basic_shell,
};
use std::path::Path;

fn render_shell(
    config: &HtmlDocumentConfig,
    page_metadata: &HtmlPageMetadata,
    logical_html_path: &str,
    project_name: &str,
    body_html: &str,
    script_html: &str,
) -> String {
    render_html_document_shell(
        config,
        page_metadata,
        Path::new(logical_html_path),
        project_name,
        body_html.to_owned(),
        script_html.to_owned(),
        None,
    )
}

#[test]
fn renderer_outputs_full_document_shell() {
    let html = render_shell(
        &HtmlDocumentConfig::default(),
        &HtmlPageMetadata::default(),
        "index.html",
        "",
        "<h1>Hello</h1>\n",
        "<script>start()</script>\n",
    );

    assert_has_basic_shell(&html);
    assert!(html.contains("<html lang=\"en\">"));
}

#[test]
fn renderer_applies_title_precedence_and_affixes() {
    let config = HtmlDocumentConfig {
        title_prefix: String::from("Docs | "),
        title_postfix: String::from(" | Beanstalk"),
        ..HtmlDocumentConfig::default()
    };
    let page = HtmlPageMetadata {
        title: Some(String::from("Overview")),
        ..HtmlPageMetadata::default()
    };

    let html = render_shell(&config, &page, "docs/index.html", "Project", "", "");
    assert!(html.contains("<title>Docs | Overview | Beanstalk</title>"));
}

#[test]
fn renderer_uses_route_title_fallback_before_project_name() {
    let html = render_shell(
        &HtmlDocumentConfig::default(),
        &HtmlPageMetadata::default(),
        "docs/basics/index.html",
        "Project",
        "",
        "",
    );

    assert!(html.contains("<title>Basics</title>"));
}

#[test]
fn renderer_uses_page_body_style_before_config_style() {
    let config = HtmlDocumentConfig {
        body_style: String::from("margin: 0;"),
        ..HtmlDocumentConfig::default()
    };
    let page = HtmlPageMetadata {
        body_style: Some(String::from("padding: 0;")),
        ..HtmlPageMetadata::default()
    };

    let html = render_shell(&config, &page, "index.html", "", "", "");
    assert!(html.contains("<body style=\"padding: 0;\">"));
}

#[test]
fn renderer_keeps_script_inside_body() {
    let html = render_shell(
        &HtmlDocumentConfig::default(),
        &HtmlPageMetadata::default(),
        "index.html",
        "",
        "<div>content</div>\n",
        "<script>bootstrap()</script>\n",
    );

    assert_fragment_before_body_close(&html, "<script>bootstrap()</script>");
}

#[test]
fn renderer_injects_codeblock_scroll_styles() {
    let html = render_shell(
        &HtmlDocumentConfig::default(),
        &HtmlPageMetadata::default(),
        "index.html",
        "",
        "<h1>Hello</h1>\n",
        "",
    );

    let codeblock_rule = extract_css_rule(&html, ".codeblock");

    assert!(
        codeblock_rule.contains("overflow-x: auto"),
        "expected .codeblock to set overflow-x: auto, got: {codeblock_rule}"
    );
    assert!(
        codeblock_rule.contains("white-space: pre"),
        "expected .codeblock to set white-space: pre, got: {codeblock_rule}"
    );
}

/// Extracts the first CSS rule block that starts with `selector`.
///
/// WHAT: finds the selector in the CSS and returns the text between the following `{` and the
///       matching `}`.
/// WHY: lets tests assert properties within a specific rule without being fooled by the same
///      property appearing in unrelated rules.
fn extract_css_rule<'a>(css: &'a str, selector: &str) -> &'a str {
    let selector_start = css
        .find(selector)
        .unwrap_or_else(|| panic!("expected CSS to contain selector '{selector}'"));
    let block_start = css[selector_start..]
        .find('{')
        .map(|offset| selector_start + offset)
        .expect("expected opening brace after selector");
    let block_end = css[block_start..]
        .find('}')
        .map(|offset| block_start + offset)
        .expect("expected closing brace for selector block");

    &css[block_start..=block_end]
}
