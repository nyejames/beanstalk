//! Shared HTML document shell rendering.
//!
//! WHAT: merges builder config, page metadata, body HTML, and runtime script HTML into one final
//!       HTML document.
//! WHY: JS-only and HTML+Wasm outputs must share one shell policy so they cannot drift.

use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::page_metadata::HtmlPageMetadata;
use std::fmt::Write as _;
use std::path::Path;

const CORE_CSS: &str = include_str!("bs-css-core.css");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedHtmlDocument {
    pub lang: String,
    pub title: String,
    pub description: Option<String>,
    pub favicon: Option<String>,
    pub inject_charset: bool,
    pub inject_viewport: bool,
    pub inject_color_scheme: bool,
    pub head_html: String,
    pub body_style: String,
    pub body_html: String,
    pub script_html: String,
    pub core_css: Option<String>,
}

pub(crate) fn render_html_document_shell(
    config: &HtmlDocumentConfig,
    page_metadata: &HtmlPageMetadata,
    logical_html_path: &Path,
    project_name: &str,
    body_html: String,
    script_html: String,
) -> String {
    let resolved = resolve_html_document(
        config,
        page_metadata,
        logical_html_path,
        project_name,
        body_html,
        script_html,
    );

    render_resolved_document(&resolved)
}

fn resolve_html_document(
    config: &HtmlDocumentConfig,
    page_metadata: &HtmlPageMetadata,
    logical_html_path: &Path,
    project_name: &str,
    body_html: String,
    script_html: String,
) -> ResolvedHtmlDocument {
    let base_title = page_metadata
        .title
        .clone()
        .or_else(|| route_title_fallback(logical_html_path))
        .or_else(|| (!project_name.is_empty()).then(|| project_name.to_string()))
        .unwrap_or_default();

    ResolvedHtmlDocument {
        lang: page_metadata
            .lang
            .clone()
            .unwrap_or_else(|| config.lang.clone()),
        title: format!(
            "{}{}{}",
            config.title_prefix, base_title, config.title_postfix
        ),
        description: page_metadata.description.clone(),
        favicon: page_metadata
            .favicon
            .clone()
            .or_else(|| config.favicon.clone()),
        inject_charset: config.inject_charset,
        inject_viewport: config.inject_viewport,
        inject_color_scheme: config.inject_color_scheme,
        head_html: page_metadata.head_html.clone().unwrap_or_default(),
        body_style: page_metadata
            .body_style
            .clone()
            .unwrap_or_else(|| config.body_style.clone()),
        body_html,
        script_html,
        core_css: config.inject_core_css.then(|| CORE_CSS.to_string()),
    }
}

fn render_resolved_document(document: &ResolvedHtmlDocument) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n");
    let _ = writeln!(
        html,
        "<html lang=\"{}\">",
        escape_html_attribute(&document.lang)
    );
    html.push_str("  <head>\n");
    if document.inject_charset {
        html.push_str("    <meta charset=\"UTF-8\">\n");
    }
    if document.inject_viewport {
        html.push_str(
            "    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
        );
    }
    if document.inject_color_scheme {
        html.push_str("    <meta name=\"color-scheme\" content=\"light dark\">\n");
    }
    let _ = writeln!(
        html,
        "    <title>{}</title>",
        escape_html_text(&document.title)
    );

    if let Some(description) = &document.description {
        let _ = writeln!(
            html,
            "    <meta name=\"description\" content=\"{}\">",
            escape_html_attribute(description)
        );
    }

    if let Some(favicon) = &document.favicon {
        let _ = writeln!(
            html,
            "    <link rel=\"icon\" href=\"{}\">",
            escape_html_attribute(favicon)
        );
    }

    if let Some(core_css) = &document.core_css {
        html.push_str("    <style>\n");
        html.push_str(core_css);
        if !core_css.ends_with('\n') {
            html.push('\n');
        }
        html.push_str("    </style>\n");
    }

    if !document.head_html.is_empty() {
        html.push_str(&indent_html_block(&document.head_html, "    "));
        if !document.head_html.ends_with('\n') {
            html.push('\n');
        }
    }

    html.push_str("  </head>\n");
    let _ = writeln!(
        html,
        "  <body style=\"{}\">",
        escape_html_attribute(&document.body_style)
    );
    if !document.body_html.is_empty() {
        html.push_str(&indent_html_block(&document.body_html, "    "));
    }
    if !document.script_html.is_empty() {
        if !html.ends_with('\n') {
            html.push('\n');
        }
        html.push_str(&indent_html_block(&document.script_html, "    "));
    }
    if !html.ends_with('\n') {
        html.push('\n');
    }
    html.push_str("  </body>\n");
    html.push_str("</html>\n");

    html
}

fn route_title_fallback(logical_html_path: &Path) -> Option<String> {
    let route_segment =
        if logical_html_path.file_name().and_then(|name| name.to_str()) == Some("index.html") {
            logical_html_path
                .parent()?
                .file_name()?
                .to_str()?
                .to_string()
        } else {
            logical_html_path.file_stem()?.to_str()?.to_string()
        };

    if route_segment.is_empty() {
        return None;
    }

    let mut formatted = String::with_capacity(route_segment.len());
    let mut uppercase_next = true;
    for ch in route_segment.chars() {
        if matches!(ch, '-' | '_' | '/') {
            formatted.push(' ');
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            for upper in ch.to_uppercase() {
                formatted.push(upper);
            }
            uppercase_next = false;
        } else {
            formatted.push(ch);
        }
    }

    Some(formatted)
}

fn indent_html_block(input: &str, indent: &str) -> String {
    let mut output = String::new();
    for line in input.lines() {
        if line.is_empty() {
            output.push('\n');
            continue;
        }
        output.push_str(indent);
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_html_attribute(value: &str) -> String {
    escape_html_text(value).replace('"', "&quot;")
}

#[cfg(test)]
#[path = "tests/document_shell_tests.rs"]
mod tests;
