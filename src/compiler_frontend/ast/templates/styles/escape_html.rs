//! Built-in `$escape_html` template style support.
//!
//! WHAT:
//! - Escapes HTML-sensitive characters in compile-time body string runs.
//! - Processes structured formatter input directly, preserving opaque child anchors.
//!
//! WHY:
//! - Templates often embed user-facing text into HTML output.
//! - `$escape_html` provides an explicit, lightweight escape pass without requiring markdown.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{
    Formatter, FormatterResult, TemplateFormatter,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterInputPiece, FormatterOutput, FormatterOutputPiece,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::string_interning::StringTable;
use std::sync::Arc;

#[derive(Debug)]
struct EscapeHtmlTemplateFormatter;

impl TemplateFormatter for EscapeHtmlTemplateFormatter {
    fn format(
        &self,
        input: FormatterInput,
        string_table: &mut StringTable,
    ) -> Result<FormatterResult, CompilerMessages> {
        let pieces = input
            .pieces
            .into_iter()
            .map(|piece| match piece {
                FormatterInputPiece::Text(text_piece) => {
                    let text = string_table.resolve(text_piece.text);
                    let mut escaped = String::with_capacity(text.len());

                    for ch in text.chars() {
                        match ch {
                            '&' => escaped.push_str("&amp;"),
                            '<' => escaped.push_str("&lt;"),
                            '>' => escaped.push_str("&gt;"),
                            '"' => escaped.push_str("&quot;"),
                            '\'' => escaped.push_str("&#39;"),
                            _ => escaped.push(ch),
                        }
                    }

                    FormatterOutputPiece::Text(escaped)
                }
                // Opaque anchors (child templates, dynamic expressions) pass through
                // without escaping — their content is sealed.
                FormatterInputPiece::Opaque(id) => FormatterOutputPiece::Opaque(id),
            })
            .collect();

        Ok(FormatterResult {
            output: FormatterOutput { pieces },
        })
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
    });
    template.clear_directive_validation();
}
