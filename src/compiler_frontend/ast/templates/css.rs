//! Built-in `$css` template style support.
//!
//! WHAT:
//! - parse the narrow `$css` directive syntax (`$css` or `$css("inline")`)
//! - run cheap compile-time validation over statically known CSS template bodies
//!
//! WHY:
//! - editor/highlighter tooling can treat these templates as CSS
//! - the compiler can surface low-cost malformed-CSS warnings early, with source spans

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::{
    CssDirectiveMode, TemplateAtom, TemplateSegmentOrigin,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{
    CharPosition, FileTokens, TextLocation, TokenKind,
};
use crate::return_syntax_error;

#[derive(Clone, Debug)]
pub(crate) struct CssTemplateDiagnostic {
    // WHAT: end-user diagnostic text emitted as a warning.
    pub message: String,
    // WHAT: mapped location inside the original template body.
    pub location: TextLocation,
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

pub(crate) fn configure_css_style(
    token_stream: &mut FileTokens,
    template: &mut Template,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // WHAT: parse the directive-local argument syntax only.
    // WHY: keep `$css(...)` parsing isolated from the general expression parser.
    let mode = parse_css_mode_argument(token_stream, string_table)?;
    template.style.id = "css";
    template.style.formatter = None;
    template.style.formatter_precedence = 0;
    template.style.css_mode = Some(mode);
    Ok(())
}

pub(crate) fn validate_css_template(
    template: &Template,
    build_profile: FrontendBuildProfile,
    string_table: &StringTable,
) -> Vec<CssTemplateDiagnostic> {
    // Only templates explicitly marked as CSS participate in this validator.
    let Some(mode) = template.style.css_mode else {
        return Vec::new();
    };

    // Flatten only authored body segments. Head segments are directive/args, not CSS text.
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

    // Control flow scaffold: release path is ready to diverge later (minify/full parse).
    let warnings = match build_profile {
        FrontendBuildProfile::Dev => validate_dev(&source, mode),
        FrontendBuildProfile::Release => validate_release(&source, mode),
    };

    warnings
        .into_iter()
        .filter_map(|warning| {
            map_warning_span_to_text_location(&spans, &warning).map(|location| {
                CssTemplateDiagnostic {
                    message: warning.message,
                    location,
                }
            })
        })
        .collect()
}

fn parse_css_mode_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<CssDirectiveMode, CompilerError> {
    // `$css` with no parentheses defaults to block mode.
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(CssDirectiveMode::Block);
    }

    // Control flow:
    // 1) move from `StyleDirective("css")` -> `(`
    // 2) move once more to the first token inside the parens
    token_stream.advance();
    token_stream.advance();

    let argument_token = token_stream.current_token_kind().to_owned();
    match argument_token {
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "The '$css()' directive cannot use empty parentheses. Use '$css' for block CSS or '$css(\"inline\")' for inline declarations.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Use '$css' or '$css(\"inline\")'",
                }
            )
        }
        TokenKind::StringSliceLiteral(value) => {
            let mode = string_table.resolve(value);
            if mode != "inline" {
                return_syntax_error!(
                    format!(
                        "Unsupported '$css(...)' argument \"{mode}\". The only supported argument is \"inline\"."
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        PrimarySuggestion => "Use '$css' for block CSS or '$css(\"inline\")' for inline declarations",
                    }
                );
            }

            token_stream.advance();
            match token_stream.current_token_kind() {
                TokenKind::CloseParenthesis => Ok(CssDirectiveMode::Inline),
                TokenKind::Comma => {
                    return_syntax_error!(
                        "The '$css(...)' directive supports only one argument.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            PrimarySuggestion => "Use '$css(\"inline\")' with a single quoted argument",
                        }
                    )
                }
                _ => {
                    return_syntax_error!(
                        "Expected ')' after '$css(\"inline\")'.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            SuggestedInsertion => ")",
                        }
                    )
                }
            }
        }
        TokenKind::Eof => {
            return_syntax_error!(
                "Unexpected end of template head while parsing '$css(...)'. Missing ')' to close the directive.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Close the '$css(...)' directive with ')'",
                    SuggestedInsertion => ")",
                }
            )
        }
        _ => {
            return_syntax_error!(
                "The '$css(...)' directive requires a quoted string literal argument: '$css(\"inline\")'.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Use '$css(\"inline\")' with a quoted string literal argument",
                }
            )
        }
    }
}

fn validate_dev(source: &str, mode: CssDirectiveMode) -> Vec<CssSourceWarning> {
    // Dev path currently uses the same cheap validator as release.
    validate_css_source(source, mode)
}

fn validate_release(source: &str, mode: CssDirectiveMode) -> Vec<CssSourceWarning> {
    // Future hook: release-specific CSS parse/minify pipeline can replace this call.
    validate_css_source(source, mode)
}

fn validate_css_source(source: &str, mode: CssDirectiveMode) -> Vec<CssSourceWarning> {
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
    mode: CssDirectiveMode,
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
                if mode == CssDirectiveMode::Inline {
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
                if mode == CssDirectiveMode::Inline {
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
    mode: CssDirectiveMode,
    warnings: &mut Vec<CssSourceWarning>,
) {
    let Some((text, trimmed_start, trimmed_end)) = trimmed_segment(chars, start, end) else {
        return;
    };

    if depth == 0 {
        match mode {
            CssDirectiveMode::Inline => {
                // In inline mode the whole body is declaration-only.
                validate_declaration_statement(&text, trimmed_start, trimmed_end, warnings);
            }
            CssDirectiveMode::Block => {
                // At top-level block mode, bare declarations are not valid CSS unless in blocks.
                if !text.starts_with('@') {
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

    if text.starts_with('@') {
        // At-rules can have custom grammar. Cheap validator treats them as acceptable here.
        return;
    }

    validate_declaration_statement(&text, trimmed_start, trimmed_end, warnings);
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

fn collect_body_source_spans(
    template: &Template,
    string_table: &StringTable,
) -> Vec<BodySourceSpan> {
    let mut spans = Vec::new();
    let mut offset = 0usize;

    // Build a mapping table from flattened CSS offsets -> original template locations.
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

        // Offsets advance in char-space to match how scanner offsets are produced.
        offset += char_len;
    }

    spans
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
#[path = "tests/css_tests.rs"]
mod css_tests;
