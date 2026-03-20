//! Shared default template-body whitespace normalization.
//!
//! WHAT:
//! - Applies the compiler's default template-body trimming + dedent behavior.
//!
//! WHY:
//! - Template bodies are commonly authored with indentation for readability.
//! - Normalizing at parse/fold time keeps output stable without requiring explicit `$raw`.

use crate::compiler_frontend::basic_utility_functions::NumericalParsing;

/// Shared whitespace passes that template formatters and default template parsing can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateWhitespacePass {
    /// The compiler's default template dedent/trim pass.
    DefaultTemplateBody,
}

/// Position of the current body run within the whole template stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateBodyRunPosition {
    Only,
    First,
    Middle,
    Last,
}

impl TemplateBodyRunPosition {
    /// `true` when this run is at the start boundary of the template body stream.
    pub(crate) fn is_first(self) -> bool {
        matches!(self, Self::Only | Self::First)
    }

    /// `true` when this run is at the end boundary of the template body stream.
    pub(crate) fn is_last(self) -> bool {
        matches!(self, Self::Only | Self::Last)
    }
}

/// Configuration for running one whitespace pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemplateWhitespacePassProfile {
    pub pass: TemplateWhitespacePass,
    pub trim_leading_boundary: bool,
    pub trim_trailing_boundary: bool,
}

impl TemplateWhitespacePassProfile {
    pub(crate) const fn new(
        pass: TemplateWhitespacePass,
        trim_leading_boundary: bool,
        trim_trailing_boundary: bool,
    ) -> Self {
        Self {
            pass,
            trim_leading_boundary,
            trim_trailing_boundary,
        }
    }

    /// Default template dedent/trim profile used by plain templates and markdown pre-pass.
    pub(crate) const fn default_template_body() -> Self {
        Self::new(TemplateWhitespacePass::DefaultTemplateBody, true, true)
    }
}

/// Runs each configured pass in order for the provided body-run position.
pub(crate) fn apply_whitespace_passes(
    content: &mut String,
    passes: &[TemplateWhitespacePassProfile],
    run_position: TemplateBodyRunPosition,
) {
    for pass in passes {
        apply_whitespace_pass(content, *pass, run_position);
    }
}

/// Runs one whitespace pass while respecting run-boundary controls.
pub(crate) fn apply_whitespace_pass(
    content: &mut String,
    pass: TemplateWhitespacePassProfile,
    run_position: TemplateBodyRunPosition,
) {
    if content.is_empty() {
        return;
    }

    let trim_leading_boundary = pass.trim_leading_boundary && run_position.is_first();
    let trim_trailing_boundary = pass.trim_trailing_boundary && run_position.is_last();

    match pass.pass {
        TemplateWhitespacePass::DefaultTemplateBody => normalize_default_template_body_whitespace(
            content,
            trim_leading_boundary,
            trim_trailing_boundary,
        ),
    }
}

fn normalize_default_template_body_whitespace(
    content: &mut String,
    trim_leading_boundary: bool,
    trim_trailing_boundary: bool,
) {
    if content.is_empty() {
        return;
    }

    let chars: Vec<char> = content.chars().collect();
    let mut cursor = 0usize;

    // Capture one leading newline block (optional leading spaces + newline + indentation)
    // so dedent width stays stable even when this run is in the middle of a template.
    while cursor < chars.len() && chars[cursor].is_non_newline_whitespace() {
        cursor += 1;
    }

    let mut dedent_width = 0usize;
    let mut leading_trim_end = 0usize;
    if cursor < chars.len() && chars[cursor] == '\n' {
        cursor += 1;
        let dedent_start = cursor;

        while cursor < chars.len() && chars[cursor].is_non_newline_whitespace() {
            cursor += 1;
        }

        dedent_width = cursor.saturating_sub(dedent_start);
        leading_trim_end = cursor;
    }

    // Leading boundary trimming is only applied to the first body run. Middle runs
    // keep their newline boundary while dedenting still strips indentation after it.
    let start_cursor = if trim_leading_boundary && leading_trim_end > 0 {
        leading_trim_end
    } else {
        0
    };

    let mut normalized: String = chars[start_cursor..].iter().collect();
    if dedent_width > 0 {
        normalized = dedent_after_newlines(&normalized, dedent_width);
    }

    if trim_trailing_boundary {
        trim_trailing_whitespace_from_final_newline(&mut normalized);
    }

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
