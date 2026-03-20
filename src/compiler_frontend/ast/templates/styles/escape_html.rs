//! Built-in `$escape_html` template style support.
//!
//! WHAT:
//! - Escapes HTML-sensitive characters in compile-time body string runs.
//!
//! WHY:
//! - Templates often embed user-facing text into HTML output.
//! - `$escape_html` provides an explicit, lightweight escape pass without requiring markdown.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{Formatter, TemplateFormatter};
use std::sync::Arc;

#[derive(Debug)]
struct EscapeHtmlTemplateFormatter;

impl TemplateFormatter for EscapeHtmlTemplateFormatter {
    fn format(&self, content: &mut String) {
        let mut escaped = String::with_capacity(content.len());

        for ch in content.chars() {
            match ch {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&#39;"),
                _ => escaped.push(ch),
            }
        }

        *content = escaped;
    }
}

pub(crate) fn escape_html_formatter() -> Formatter {
    Formatter {
        id: "escape_html",
        skip_if_already_formatted: false,
        pre_format_whitespace_passes: Vec::new(),
        formatter: Arc::new(EscapeHtmlTemplateFormatter),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub(crate) fn configure_escape_html_style(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.id = "escape_html";
        style.formatter = Some(escape_html_formatter());
        style.formatter_precedence = 0;
        style.css_mode = None;
        style.html_mode = false;
    });
}
