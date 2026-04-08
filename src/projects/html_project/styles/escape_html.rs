//! HTML-project-owned `$escape_html` formatter.
//!
//! WHAT:
//! - Escapes HTML-sensitive characters in compile-time body string runs.
//! - Preserves opaque child anchors so frontend composition semantics remain unchanged.
//!
//! WHY:
//! - HTML escaping is output-policy behavior owned by the HTML project builder, not a core
//!   language directive.

use crate::compiler_frontend::ast::templates::template::{
    Formatter, FormatterResult, TemplateFormatter,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterInputPiece, FormatterOutput, FormatterOutputPiece,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveArgumentValue;
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
            warnings: Vec::new(),
        })
    }
}

pub(crate) fn escape_html_formatter() -> Formatter {
    Formatter {
        pre_format_whitespace_passes: Vec::new(),
        formatter: Arc::new(EscapeHtmlTemplateFormatter),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub(crate) fn escape_html_formatter_factory(
    argument: Option<&StyleDirectiveArgumentValue>,
) -> Result<Option<Formatter>, String> {
    if argument.is_some() {
        return Err("'$escape_html' does not accept arguments.".to_string());
    }

    Ok(Some(escape_html_formatter()))
}
