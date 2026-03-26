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
