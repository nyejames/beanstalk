//! Terse rendering for `CompilerDiagnostic`.
//!
//! WHAT: produces single-line machine-friendly diagnostic records.
//! WHY: CI, test runners, and IDEs often prefer compact output without ASCII art or colours.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, display_column_number, display_line_number,
    relative_display_path_from_root, render_payload, resolve_source_file_path,
};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticSeverity};
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[cfg(test)]
pub(crate) fn format_terse_diagnostics(
    diagnostics: &[CompilerDiagnostic],
    string_table: &StringTable,
) -> Vec<String> {
    let context = DiagnosticRenderContext::new(string_table);
    format_terse_diagnostics_with_context(diagnostics, context)
}

#[cfg(test)]
pub(crate) fn format_terse_diagnostics_with_context(
    diagnostics: &[CompilerDiagnostic],
    context: DiagnosticRenderContext<'_>,
) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| format_terse_diagnostic_with_context(d, context))
        .collect()
}

pub(crate) fn format_terse_compiler_messages(messages: &CompilerMessages) -> Vec<String> {
    messages
        .diagnostic_slice()
        .iter()
        .enumerate()
        .map(|(diagnostic_index, diagnostic)| {
            format_terse_diagnostic_with_context(
                diagnostic,
                messages.diagnostic_render_context(diagnostic_index),
            )
        })
        .collect()
}

pub(crate) fn format_terse_diagnostic_with_context(
    diagnostic: &CompilerDiagnostic,
    context: DiagnosticRenderContext<'_>,
) -> String {
    let string_table = context.string_table;
    let descriptor = diagnostic.kind.descriptor();
    let severity_char = match diagnostic.severity {
        DiagnosticSeverity::Error => 'E',
        DiagnosticSeverity::Warning => 'W',
        DiagnosticSeverity::Note => 'N',
    };

    let display_path = relative_display_path_from_root(
        &resolve_source_file_path(&diagnostic.primary_location.scope, string_table),
        &std::env::current_dir().unwrap_or_default(),
    );
    let sanitized_path = sanitize_terse_field(&display_path);
    let line = display_line_number(diagnostic.primary_location.start_pos.line_number);
    let column = display_column_number(diagnostic.primary_location.start_pos.char_column);

    let rendered_payload = render_payload(&diagnostic.payload, context);

    format!(
        "{severity_char}|{}|{sanitized_path}|{line}:{column}|{}",
        descriptor.code,
        sanitize_terse_field(&rendered_payload.message)
    )
}

fn sanitize_terse_field(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
}
