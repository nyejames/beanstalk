//! `$html` formatter/validator implementation shared by build-system style registration.
//!
//! WHAT:
//! - provides a formatter that preserves HTML text while emitting sanitation warnings.
//! - keeps diagnostics source-mapped by translating flattened offsets back to text spans.
//!
//! WHY:
//! - validation ownership belongs to formatter execution, not parser post-passes.

use crate::compiler_frontend::ast::templates::template::{
    Formatter, FormatterResult, TemplateFormatter,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterInputPiece, FormatterOutput, FormatterOutputPiece,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveArgumentValue;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, TextLocation};
use std::sync::Arc;

#[derive(Debug)]
struct HtmlValidationTemplateFormatter;

pub(crate) fn html_validation_formatter() -> Formatter {
    Formatter {
        id: "html",
        skip_if_already_formatted: false,
        pre_format_whitespace_passes: Vec::new(),
        formatter: Arc::new(HtmlValidationTemplateFormatter),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub(crate) fn html_formatter_factory(
    argument: Option<&StyleDirectiveArgumentValue>,
) -> Result<Option<Formatter>, String> {
    if argument.is_some() {
        return Err("'$html' does not accept arguments.".to_string());
    }
    Ok(Some(html_validation_formatter()))
}

impl TemplateFormatter for HtmlValidationTemplateFormatter {
    fn format(
        &self,
        input: FormatterInput,
        string_table: &mut StringTable,
    ) -> Result<FormatterResult, CompilerMessages> {
        let mut output_pieces = Vec::with_capacity(input.pieces.len());
        let mut spans: Vec<BodySourceSpan> = Vec::new();
        let mut flattened_source = String::new();
        let mut offset = 0usize;

        for piece in input.pieces {
            match piece {
                FormatterInputPiece::Text(text_piece) => {
                    let text = string_table.resolve(text_piece.text).to_owned();
                    let char_len = text.chars().count();
                    if char_len > 0 {
                        spans.push(BodySourceSpan {
                            start_offset: offset,
                            end_offset: offset + char_len,
                            text: text.clone(),
                            location: text_piece.location,
                        });
                        offset += char_len;
                    }
                    flattened_source.push_str(&text);
                    output_pieces.push(FormatterOutputPiece::Text(text));
                }
                FormatterInputPiece::Opaque(anchor) => {
                    output_pieces.push(FormatterOutputPiece::Opaque(anchor));
                }
            }
        }

        let warnings = emit_html_formatter_warnings(&spans, &flattened_source, string_table);
        Ok(FormatterResult {
            output: FormatterOutput {
                pieces: output_pieces,
            },
            warnings,
        })
    }
}

#[derive(Clone, Debug)]
struct HtmlSourceWarning {
    message: String,
    start_offset: usize,
    end_offset: usize, // exclusive
}

#[derive(Clone, Debug)]
struct BodySourceSpan {
    start_offset: usize,
    end_offset: usize, // exclusive
    text: String,
    location: TextLocation,
}

fn emit_html_formatter_warnings(
    spans: &[BodySourceSpan],
    source: &str,
    string_table: &StringTable,
) -> Vec<CompilerWarning> {
    if spans.is_empty() || source.trim().is_empty() {
        return Vec::new();
    }

    validate_html_source(source)
        .into_iter()
        .filter_map(|warning| {
            map_warning_span_to_text_location(spans, &warning).map(|location| {
                let file_path = location.scope.to_path_buf(string_table);
                CompilerWarning::new(
                    &warning.message,
                    location.to_error_location(string_table),
                    WarningKind::MalformedHtmlTemplate,
                    file_path,
                )
            })
        })
        .collect()
}

fn validate_html_source(source: &str) -> Vec<HtmlSourceWarning> {
    let lowered = source.to_ascii_lowercase();
    let chars: Vec<char> = lowered.chars().collect();
    let mut warnings = Vec::new();

    push_literal_match_warnings(
        &chars,
        "<script",
        "Potentially unsafe '<script' tag found in '$html' template.",
        &mut warnings,
    );

    push_literal_match_warnings(
        &chars,
        "javascript:",
        "Potentially unsafe 'javascript:' URL found in '$html' template.",
        &mut warnings,
    );

    warnings.extend(scan_inline_event_handler_warnings(&chars));
    warnings
}

fn push_literal_match_warnings(
    chars: &[char],
    pattern: &str,
    message: &str,
    warnings: &mut Vec<HtmlSourceWarning>,
) {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    if pattern_chars.is_empty() || pattern_chars.len() > chars.len() {
        return;
    }

    for index in 0..=chars.len() - pattern_chars.len() {
        if chars[index..index + pattern_chars.len()] == *pattern_chars {
            warnings.push(HtmlSourceWarning {
                message: message.to_owned(),
                start_offset: index,
                end_offset: index + pattern_chars.len(),
            });
        }
    }
}

fn scan_inline_event_handler_warnings(chars: &[char]) -> Vec<HtmlSourceWarning> {
    let mut warnings = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] != 'o' || chars.get(index + 1) != Some(&'n') {
            index += 1;
            continue;
        }

        if index > 0 && is_attribute_name_char(chars[index - 1]) {
            index += 1;
            continue;
        }

        let mut cursor = index + 2;
        let mut has_attribute_name = false;
        while cursor < chars.len() && is_attribute_name_char(chars[cursor]) {
            has_attribute_name = true;
            cursor += 1;
        }

        if !has_attribute_name {
            index += 1;
            continue;
        }

        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }

        if cursor < chars.len() && chars[cursor] == '=' {
            warnings.push(HtmlSourceWarning {
                message:
                    "Potentially unsafe inline 'on*=' event handler found in '$html' template."
                        .to_owned(),
                start_offset: index,
                end_offset: cursor.saturating_add(1),
            });
            index = cursor.saturating_add(1);
            continue;
        }

        index += 1;
    }

    warnings
}

fn is_attribute_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn map_warning_span_to_text_location(
    spans: &[BodySourceSpan],
    warning: &HtmlSourceWarning,
) -> Option<TextLocation> {
    let start = warning.start_offset;
    let end_inclusive = warning.end_offset.saturating_sub(1).max(start);

    let start_point = map_offset_to_point_location(spans, start)?;
    let end_point = map_offset_to_point_location(spans, end_inclusive)?;

    if start_point.scope != end_point.scope {
        return Some(start_point);
    }

    Some(TextLocation {
        scope: start_point.scope,
        start_pos: start_point.start_pos,
        end_pos: end_point.end_pos,
    })
}

fn map_offset_to_point_location(spans: &[BodySourceSpan], offset: usize) -> Option<TextLocation> {
    let total_chars = spans.last().map(|span| span.end_offset)?;
    if total_chars == 0 {
        return None;
    }

    let clamped_offset = offset.min(total_chars.saturating_sub(1));
    let span = spans
        .iter()
        .find(|span| clamped_offset >= span.start_offset && clamped_offset < span.end_offset)
        .or_else(|| spans.last())?;
    let local_offset = clamped_offset.saturating_sub(span.start_offset);
    let position = position_after_chars(&span.location.start_pos, &span.text, local_offset);

    Some(TextLocation {
        scope: span.location.scope.to_owned(),
        start_pos: position,
        end_pos: position,
    })
}

fn position_after_chars(start: &CharPosition, text: &str, consumed_chars: usize) -> CharPosition {
    let mut position = *start;
    for ch in text.chars().take(consumed_chars) {
        if ch == '\n' {
            position.line_number += 1;
            position.char_column = 0;
        } else {
            position.char_column += 1;
        }
    }

    position
}
