//! Dev-server HTML rendering for `CompilerDiagnostic`.
//!
//! WHAT: converts structured diagnostics into escaped HTML cards for the dev-server error page.
//! WHY: the dev-server needs clickable source links and readable diagnostic output.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, display_column_number, display_line_number, render_payload,
    resolve_source_file_path,
};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticSeverity};
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::path::Path;

#[cfg(test)]
#[allow(dead_code)] // Test suites call the context-aware variant directly in targeted cases only.
pub(crate) fn render_diagnostics_html(
    diagnostics: &[CompilerDiagnostic],
    project_root: &Path,
    string_table: &StringTable,
) -> String {
    let context = DiagnosticRenderContext::new(string_table);
    render_diagnostics_html_with_context(diagnostics, project_root, context)
}

#[cfg(test)]
#[allow(dead_code)] // Kept for renderer-focused unit tests that bypass CompilerMessages.
pub(crate) fn render_diagnostics_html_with_context(
    diagnostics: &[CompilerDiagnostic],
    project_root: &Path,
    context: DiagnosticRenderContext<'_>,
) -> String {
    if diagnostics.is_empty() {
        return String::from("<p>No compiler diagnostics available.</p>");
    }

    diagnostics
        .iter()
        .map(|d| render_diagnostic_card(d, project_root, context))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_compiler_messages_html(
    messages: &CompilerMessages,
    project_root: &Path,
) -> String {
    if messages.diagnostic_slice().is_empty() {
        return String::from("<p>No compiler diagnostics available.</p>");
    }

    messages
        .diagnostic_slice()
        .iter()
        .enumerate()
        .map(|(diagnostic_index, diagnostic)| {
            render_diagnostic_card(
                diagnostic,
                project_root,
                messages.diagnostic_render_context(diagnostic_index),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_diagnostic_card(
    diagnostic: &CompilerDiagnostic,
    project_root: &Path,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let descriptor = diagnostic.kind.descriptor();
    let severity_badge = match diagnostic.severity {
        DiagnosticSeverity::Error => "ERROR",
        DiagnosticSeverity::Warning => "WARNING",
        DiagnosticSeverity::Note => "NOTE",
    };
    let badge_class = match diagnostic.severity {
        DiagnosticSeverity::Error => "badge",
        DiagnosticSeverity::Warning => "badge warning",
        DiagnosticSeverity::Note => "badge info",
    };

    let mut details = String::from("<ul class=\"detail-list\">");

    let resolved_path = resolve_source_file_path(&diagnostic.primary_location.scope, string_table);
    let display_root = match std::fs::canonicalize(project_root) {
        Ok(canonical_root) => canonical_root,
        Err(_) => project_root.to_path_buf(),
    };
    let display_label =
        crate::compiler_frontend::compiler_messages::render::relative_display_path_from_root(
            &resolved_path,
            &display_root,
        );
    let line = display_line_number(diagnostic.primary_location.start_pos.line_number);
    let column = display_column_number(diagnostic.primary_location.start_pos.char_column);

    details.push_str(&format!(
        "<li><a href=\"file://{}\">{}</a> - line {}, col {}</li>",
        escape_html(&resolved_path.to_string_lossy()),
        escape_html(&display_label),
        line,
        column
    ));

    let rendered_payload = render_payload(&diagnostic.payload, context);
    if !rendered_payload.message.is_empty() {
        details.push_str(&format!(
            "<li>{}</li>",
            escape_html(&rendered_payload.message)
        ));
    }
    for guidance in rendered_payload.guidance {
        details.push_str(&format!("<li>{}</li>", escape_html(&guidance)));
    }

    details.push_str("</ul>");

    format!(
        "<article class=\"diagnostic\">\
         <header><span class=\"{badge_class}\">{severity_badge}</span> \
         <code>{code}</code> {title}</header>\
         {details}\
         </article>",
        code = escape_html(descriptor.code),
        title = escape_html(descriptor.title),
    )
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
