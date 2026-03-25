//! Built-in `$html` template style support.
//!
//! WHAT:
//! - Parses `$html` as an argumentless directive.
//! - Runs cheap compile-time sanitation checks on static HTML template bodies.
//!
//! WHY:
//! - The frontend and editor tooling need a dedicated HTML style marker.
//! - The compiler can surface fast, low-cost warnings for risky HTML constructs.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateDirectiveValidation, TemplateSegmentOrigin,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{
    CharPosition, FileTokens, TextLocation, TokenKind,
};
use crate::return_syntax_error;

#[derive(Clone, Debug)]
pub(crate) struct HtmlTemplateDiagnostic {
    pub message: String,
    pub location: TextLocation,
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

pub(crate) fn configure_html_style(
    token_stream: &FileTokens,
    template: &mut Template,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
        return_syntax_error!(
            "'$html' does not accept arguments.",
            token_stream.current_location().to_error_location(string_table),
            {
                PrimarySuggestion => "Use '$html' without '(...)'",
            }
        );
    }

    template.apply_style_updates(|style| {
        style.id = "html";
        style.formatter = None;
    });
    template.set_directive_validation(TemplateDirectiveValidation::Html);

    Ok(())
}

pub(crate) fn validate_html_template(
    template: &Template,
    string_table: &StringTable,
) -> Vec<HtmlTemplateDiagnostic> {
    let spans = collect_body_source_spans(template, string_table);
    if spans.is_empty() {
        return Vec::new();
    }

    let source = spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    if source.trim().is_empty() {
        return Vec::new();
    }

    let warnings = validate_html_source(&source);
    warnings
        .into_iter()
        .filter_map(|warning| {
            map_warning_span_to_text_location(&spans, &warning).map(|location| {
                HtmlTemplateDiagnostic {
                    message: warning.message,
                    location,
                }
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

fn collect_body_source_spans(
    template: &Template,
    string_table: &StringTable,
) -> Vec<BodySourceSpan> {
    let mut spans = Vec::new();
    let mut offset = 0usize;

    for atom in &template.content.atoms {
        let TemplateAtom::Content(segment) = atom else {
            continue;
        };

        if segment.origin != TemplateSegmentOrigin::Body {
            continue;
        }

        let ExpressionKind::StringSlice(text) = segment.expression.kind else {
            continue;
        };

        let text = string_table.resolve(text).to_owned();
        let char_len = text.chars().count();
        if char_len == 0 {
            continue;
        }

        spans.push(BodySourceSpan {
            start_offset: offset,
            end_offset: offset + char_len,
            text,
            location: segment.expression.location.clone(),
        });
        offset += char_len;
    }

    spans
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
