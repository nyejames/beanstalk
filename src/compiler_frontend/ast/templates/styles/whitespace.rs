//! Shared default template-body whitespace normalization.
//!
//! WHAT:
//! - Applies the compiler's default template-body trimming + dedent behavior.
//!
//! WHY:
//! - Template bodies are commonly authored with indentation for readability.
//! - Normalizing at parse/fold time keeps output stable without requiring explicit `$raw`.

use crate::compiler_frontend::basic_utility_functions::NumericalParsing;

/// Normalizes one contiguous compile-time body run using the default template rules.
///
/// Control-flow summary:
/// 1) Trim initial whitespace, including one leading newline and the indentation after it.
/// 2) Dedent every post-newline run by the indentation captured in step 1.
/// 3) Trim trailing whitespace only when it starts at the final newline.
pub(crate) fn normalize_template_body_whitespace(content: &mut String) {
    if content.is_empty() {
        return;
    }

    let chars: Vec<char> = content.chars().collect();
    let mut cursor = 0usize;

    let mut dedent_width = 0usize;
    while cursor < chars.len() && chars[cursor].is_non_newline_whitespace() {
        cursor += 1;
    }

    if cursor < chars.len() && chars[cursor] == '\n' {
        cursor += 1;
        let dedent_start = cursor;

        while cursor < chars.len() && chars[cursor].is_non_newline_whitespace() {
            cursor += 1;
        }

        dedent_width = cursor.saturating_sub(dedent_start);
    } else {
        // Without an initial newline block, leading spaces are authored content and
        // should be preserved (for example, explicit spacing in inline templates).
        cursor = 0;
    }

    let mut normalized: String = chars[cursor..].iter().collect();
    if dedent_width > 0 {
        normalized = dedent_after_newlines(&normalized, dedent_width);
    }

    trim_trailing_whitespace_from_final_newline(&mut normalized);
    *content = normalized;
}

fn dedent_after_newlines(input: &str, dedent_width: usize) -> String {
    if dedent_width == 0 {
        return input.to_owned();
    }

    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0usize;

    while cursor < chars.len() {
        let current = chars[cursor];
        output.push(current);
        cursor += 1;

        if current != '\n' {
            continue;
        }

        let mut removed = 0usize;
        while cursor < chars.len() && removed < dedent_width {
            let next = chars[cursor];
            if !next.is_non_newline_whitespace() {
                break;
            }

            cursor += 1;
            removed += 1;
        }
    }

    output
}

fn trim_trailing_whitespace_from_final_newline(content: &mut String) {
    let chars: Vec<char> = content.chars().collect();
    let Some(last_newline) = chars.iter().rposition(|ch| *ch == '\n') else {
        return;
    };

    if chars[last_newline + 1..]
        .iter()
        .all(|ch| ch.is_non_newline_whitespace())
    {
        *content = chars[..last_newline].iter().collect();
    }
}
