//! `$css` formatter/validator implementation shared by build-system style registration.
//!
//! WHAT:
//! - provides a formatter that keeps CSS text unchanged while emitting malformed-CSS warnings.
//! - supports optional inline mode (`$css("inline")`) via formatter configuration.
//!
//! WHY:
//! - keeps CSS validation owned by directive-provided formatter behavior.
//! - warnings stay location-aware by mapping flattened CSS offsets back to template source spans.

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CssFormatterMode {
    Block,
    Inline,
}

#[derive(Debug)]
struct CssValidationTemplateFormatter {
    mode: CssFormatterMode,
}

pub(crate) fn css_validation_formatter(mode: CssFormatterMode) -> Formatter {
    Formatter {
        id: "css",
        skip_if_already_formatted: false,
        pre_format_whitespace_passes: Vec::new(),
        formatter: Arc::new(CssValidationTemplateFormatter { mode }),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub(crate) fn css_formatter_factory(
    argument: Option<&StyleDirectiveArgumentValue>,
) -> Result<Option<Formatter>, String> {
    let mode = match argument {
        None => CssFormatterMode::Block,
        Some(StyleDirectiveArgumentValue::String(value)) if value == "inline" => {
            CssFormatterMode::Inline
        }
        Some(StyleDirectiveArgumentValue::String(value)) => {
            return Err(format!(
                "Unsupported '$css(...)' argument \"{value}\". The only supported argument is \"inline\"."
            ));
        }
        Some(_) => {
            return Err(
                "The '$css(...)' directive expects an optional string argument, for example '$css(\"inline\")'."
                    .to_string(),
            );
        }
    };

    Ok(Some(css_validation_formatter(mode)))
}

impl TemplateFormatter for CssValidationTemplateFormatter {
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

        let warnings =
            emit_css_formatter_warnings(&spans, &flattened_source, self.mode, string_table);
        Ok(FormatterResult {
            output: FormatterOutput {
                pieces: output_pieces,
            },
            warnings,
        })
    }
}

#[derive(Clone, Debug)]
struct CssSourceWarning {
    message: String,
    start_offset: usize,
    end_offset: usize, // exclusive
}

#[derive(Clone, Debug)]
struct BodySourceSpan {
    // Offsets are measured in flattened CSS-char coordinates.
    start_offset: usize,
    end_offset: usize, // exclusive
    text: String,
    // Original template location for this slice.
    location: TextLocation,
}

#[derive(Default)]
struct ScanState {
    // Scanner state is intentionally tiny and allocation-free so these checks stay cheap.
    in_comment: bool,
    in_single_quote: bool,
    in_double_quote: bool,
    escaped: bool,
}

fn emit_css_formatter_warnings(
    spans: &[BodySourceSpan],
    source: &str,
    mode: CssFormatterMode,
    string_table: &StringTable,
) -> Vec<CompilerWarning> {
    if spans.is_empty() || source.trim().is_empty() {
        return Vec::new();
    }

    validate_css_source(source, mode)
        .into_iter()
        .filter_map(|warning| {
            map_warning_span_to_text_location(spans, &warning).map(|location| {
                let file_path = location.scope.to_path_buf(string_table);
                CompilerWarning::new(
                    &warning.message,
                    location.to_error_location(string_table),
                    WarningKind::MalformedCssTemplate,
                    file_path,
                )
            })
        })
        .collect()
}

fn validate_css_source(source: &str, mode: CssFormatterMode) -> Vec<CssSourceWarning> {
    // WHAT: one pass for delimiter integrity + one pass for statement/block shape.
    // WHY: keeps diagnostics cheap while still catching common malformed templates.
    let chars: Vec<char> = source.chars().collect();
    let mut warnings = Vec::new();
    validate_balanced_delimiters(&chars, &mut warnings);
    validate_css_shape(&chars, mode, &mut warnings);
    warnings
}

fn validate_balanced_delimiters(chars: &[char], warnings: &mut Vec<CssSourceWarning>) {
    let mut index = 0usize;
    let mut state = ScanState::default();
    let mut stack: Vec<(char, usize)> = Vec::new();

    // Control flow:
    // - `advance_scan_state` consumes comments/strings so delimiters inside them are ignored.
    // - only raw CSS delimiters participate in the balance stack.
    while index < chars.len() {
        if advance_scan_state(&mut state, chars, &mut index) {
            continue;
        }

        match chars[index] {
            '{' | '(' => stack.push((chars[index], index)),
            '}' => match stack.pop() {
                Some(('{', _)) => {}
                Some((open, open_index)) => {
                    push_warning(
                        warnings,
                        format!(
                            "Mismatched closing brace. Expected '{}' to close before '}}'.",
                            matching_closer(open)
                        ),
                        open_index,
                        open_index.saturating_add(1),
                    );
                }
                None => {
                    push_warning(
                        warnings,
                        "Unexpected closing brace '}' with no matching '{'.",
                        index,
                        index.saturating_add(1),
                    );
                }
            },
            ')' => match stack.pop() {
                Some(('(', _)) => {}
                Some((open, open_index)) => {
                    push_warning(
                        warnings,
                        format!(
                            "Mismatched closing parenthesis. Expected '{}' to close before ')'.",
                            matching_closer(open)
                        ),
                        open_index,
                        open_index.saturating_add(1),
                    );
                }
                None => {
                    push_warning(
                        warnings,
                        "Unexpected closing parenthesis ')' with no matching '('.",
                        index,
                        index.saturating_add(1),
                    );
                }
            },
            _ => {}
        }

        index += 1;
    }

    for (open, open_index) in stack {
        push_warning(
            warnings,
            format!("Unclosed '{}' in CSS template body.", open),
            open_index,
            open_index.saturating_add(1),
        );
    }
}

fn validate_css_shape(
    chars: &[char],
    mode: CssFormatterMode,
    warnings: &mut Vec<CssSourceWarning>,
) {
    let mut index = 0usize;
    let mut state = ScanState::default();
    let mut depth = 0usize;
    let mut block_is_at_rule = Vec::new();
    let mut statement_start = 0usize;
    let mut prelude_start = 0usize;

    // One linear scan that keeps enough structure to do cheap shape checks:
    // - `{` validates prelude shape and opens a block
    // - `;` validates a statement/declaration segment
    // - `}` validates trailing segment and closes a block
    while index < chars.len() {
        if advance_scan_state(&mut state, chars, &mut index) {
            continue;
        }

        match chars[index] {
            '{' => {
                if mode == CssFormatterMode::Inline {
                    push_warning(
                        warnings,
                        "Inline '$css(\"inline\")' templates only allow declarations and cannot contain selector blocks.",
                        index,
                        index.saturating_add(1),
                    );
                }

                let Some((prelude, start, end)) = trimmed_segment(chars, prelude_start, index)
                else {
                    push_warning(
                        warnings,
                        "CSS block is missing a selector or at-rule prelude before '{'.",
                        index,
                        index.saturating_add(1),
                    );
                    depth += 1;
                    block_is_at_rule.push(false);
                    statement_start = index + 1;
                    prelude_start = index + 1;
                    index += 1;
                    continue;
                };

                let is_at_rule = prelude.starts_with('@');
                let parent_is_at_rule = block_is_at_rule.last().copied().unwrap_or(false);
                // Nested selectors are usually only sensible under at-rules.
                // We warn conservatively to avoid pretending this parser fully understands nesting.
                if depth > 0 && !parent_is_at_rule && !is_at_rule {
                    push_warning(
                        warnings,
                        "Nested CSS blocks are only lightly validated in '$css'. Use nested at-rules for predictable results.",
                        start,
                        end,
                    );
                }

                depth += 1;
                block_is_at_rule.push(is_at_rule);
                statement_start = index + 1;
                prelude_start = index + 1;
            }
            '}' => {
                if mode == CssFormatterMode::Inline {
                    push_warning(
                        warnings,
                        "Inline '$css(\"inline\")' templates only allow declarations and cannot contain selector blocks.",
                        index,
                        index.saturating_add(1),
                    );
                }

                if depth > 0 {
                    validate_statement_segment(
                        chars,
                        statement_start,
                        index,
                        depth,
                        mode,
                        warnings,
                    );
                    depth -= 1;
                    let _ = block_is_at_rule.pop();
                    statement_start = index + 1;
                    prelude_start = index + 1;
                }
            }
            ';' => {
                // `;` closes one declaration/statement segment.
                validate_statement_segment(chars, statement_start, index, depth, mode, warnings);
                statement_start = index + 1;
                if depth == 0 {
                    prelude_start = index + 1;
                }
            }
            _ => {}
        }

        index += 1;
    }

    validate_statement_segment(chars, statement_start, chars.len(), depth, mode, warnings);
}

fn validate_statement_segment(
    chars: &[char],
    start: usize,
    end: usize,
    depth: usize,
    mode: CssFormatterMode,
    warnings: &mut Vec<CssSourceWarning>,
) {
    let Some((text, trimmed_start, trimmed_end)) = trimmed_segment(chars, start, end) else {
        return;
    };
    let normalized_text = strip_css_comments(&text);
    let normalized_text = normalized_text.trim();
    if normalized_text.is_empty() {
        return;
    }

    if depth == 0 {
        match mode {
            CssFormatterMode::Inline => {
                // In inline mode the whole body is declaration-only.
                validate_declaration_statement(
                    normalized_text,
                    trimmed_start,
                    trimmed_end,
                    warnings,
                );
            }
            CssFormatterMode::Block => {
                // At top-level block mode, bare declarations are not valid CSS unless in blocks.
                if !normalized_text.starts_with('@') {
                    push_warning(
                        warnings,
                        "Top-level CSS content should be selector blocks or at-rules.",
                        trimmed_start,
                        trimmed_end,
                    );
                }
            }
        }
        return;
    }

    if normalized_text.starts_with('@') {
        // At-rules can have custom grammar. Cheap validator treats them as acceptable here.
        return;
    }

    validate_declaration_statement(normalized_text, trimmed_start, trimmed_end, warnings);
}

fn validate_declaration_statement(
    statement: &str,
    start_offset: usize,
    end_offset: usize,
    warnings: &mut Vec<CssSourceWarning>,
) {
    // Declaration check intentionally stays simple:
    // - must contain one `:` split point
    // - property must look identifier-like
    // - value side cannot be empty
    let statement_chars: Vec<char> = statement.chars().collect();
    let Some(colon_index) = statement_chars.iter().position(|ch| *ch == ':') else {
        push_warning(
            warnings,
            "Malformed CSS declaration. Expected 'property: value'.",
            start_offset,
            end_offset,
        );
        return;
    };

    let property = statement_chars[..colon_index]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned();
    if !is_valid_property_name(&property) {
        push_warning(
            warnings,
            format!("Malformed CSS declaration. Invalid property name '{property}'."),
            start_offset,
            start_offset.saturating_add(colon_index.max(1)),
        );
    }

    let value = statement_chars[colon_index + 1..]
        .iter()
        .collect::<String>()
        .trim()
        .to_owned();
    if value.is_empty() {
        let value_offset = start_offset.saturating_add(colon_index);
        push_warning(
            warnings,
            "Malformed CSS declaration. Missing value after ':'.",
            value_offset,
            value_offset.saturating_add(1),
        );
    }
}

fn is_valid_property_name(property: &str) -> bool {
    if property.is_empty() {
        return false;
    }

    if let Some(custom_property) = property.strip_prefix("--") {
        // CSS custom properties (`--token`) allow a broad but still identifier-like charset.
        return !custom_property.is_empty()
            && custom_property
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    }

    let mut chars = property.chars();
    let Some(first_char) = chars.next() else {
        return false;
    };
    if !first_char.is_ascii_alphabetic() && first_char != '-' && first_char != '_' {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn map_warning_span_to_text_location(
    spans: &[BodySourceSpan],
    warning: &CssSourceWarning,
) -> Option<TextLocation> {
    // Warnings use an exclusive end; convert to an inclusive point for location mapping.
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
    // Choose the span that owns this flattened offset.
    let span = spans
        .iter()
        .find(|span| clamped_offset >= span.start_offset && clamped_offset < span.end_offset)
        .or_else(|| spans.last())?;

    let local_offset = clamped_offset.saturating_sub(span.start_offset);
    // Walk from segment start so line/column stays accurate across newlines.
    let position = position_after_chars(&span.location.start_pos, &span.text, local_offset);

    Some(TextLocation {
        scope: span.location.scope.to_owned(),
        start_pos: position,
        end_pos: position,
    })
}

fn position_after_chars(start: &CharPosition, text: &str, consumed_chars: usize) -> CharPosition {
    let mut position = *start;
    // Control flow: newline resets column and increments line; all other chars advance column.
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

fn push_warning(
    warnings: &mut Vec<CssSourceWarning>,
    message: impl Into<String>,
    start_offset: usize,
    end_offset: usize,
) {
    // Ensure every warning has at least a single-character highlight.
    let end_offset = end_offset.max(start_offset.saturating_add(1));
    warnings.push(CssSourceWarning {
        message: message.into(),
        start_offset,
        end_offset,
    });
}

fn trimmed_segment(chars: &[char], start: usize, end: usize) -> Option<(String, usize, usize)> {
    let mut trimmed_start = start.min(chars.len());
    let mut trimmed_end = end.min(chars.len());

    while trimmed_start < trimmed_end && chars[trimmed_start].is_whitespace() {
        trimmed_start += 1;
    }
    while trimmed_end > trimmed_start && chars[trimmed_end - 1].is_whitespace() {
        trimmed_end -= 1;
    }

    if trimmed_start >= trimmed_end {
        return None;
    }

    Some((
        chars[trimmed_start..trimmed_end].iter().collect(),
        trimmed_start,
        trimmed_end,
    ))
}

fn matching_closer(open: char) -> char {
    match open {
        '{' => '}',
        '(' => ')',
        _ => open,
    }
}

fn strip_css_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0usize;
    let mut state = ScanState::default();

    while index < chars.len() {
        let ch = chars[index];
        let next = chars.get(index + 1).copied();

        if state.in_comment {
            if ch == '*' && next == Some('/') {
                state.in_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if state.in_single_quote {
            if state.escaped {
                state.escaped = false;
            } else if ch == '\\' {
                state.escaped = true;
            } else if ch == '\'' {
                state.in_single_quote = false;
            }
            output.push(ch);
            index += 1;
            continue;
        }

        if state.in_double_quote {
            if state.escaped {
                state.escaped = false;
            } else if ch == '\\' {
                state.escaped = true;
            } else if ch == '"' {
                state.in_double_quote = false;
            }
            output.push(ch);
            index += 1;
            continue;
        }

        if ch == '/' && next == Some('*') {
            state.in_comment = true;
            index += 2;
            continue;
        }

        if ch == '\'' {
            state.in_single_quote = true;
        } else if ch == '"' {
            state.in_double_quote = true;
        }

        output.push(ch);
        index += 1;
    }

    output
}

fn advance_scan_state(state: &mut ScanState, chars: &[char], index: &mut usize) -> bool {
    let ch = chars[*index];
    let next = chars.get(*index + 1).copied();

    // Existing comment/string mode consumes input until its own terminator is reached.
    if state.in_comment {
        if ch == '*' && next == Some('/') {
            state.in_comment = false;
            *index += 2;
        } else {
            *index += 1;
        }
        return true;
    }

    if state.in_single_quote {
        if state.escaped {
            state.escaped = false;
        } else if ch == '\\' {
            state.escaped = true;
        } else if ch == '\'' {
            state.in_single_quote = false;
        }
        *index += 1;
        return true;
    }

    if state.in_double_quote {
        if state.escaped {
            state.escaped = false;
        } else if ch == '\\' {
            state.escaped = true;
        } else if ch == '"' {
            state.in_double_quote = false;
        }
        *index += 1;
        return true;
    }

    if ch == '/' && next == Some('*') {
        // CSS block comments are the only comment form we intentionally support here.
        state.in_comment = true;
        *index += 2;
        return true;
    }

    if ch == '\'' {
        state.in_single_quote = true;
        *index += 1;
        return true;
    }

    if ch == '"' {
        state.in_double_quote = true;
        *index += 1;
        return true;
    }

    false
}

#[cfg(test)]
#[path = "../tests/css_tests.rs"]
mod css_tests;
