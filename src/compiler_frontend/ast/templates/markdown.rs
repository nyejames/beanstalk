use crate::compiler_frontend::ast::templates::template::{Formatter, TemplateFormatter};
use std::sync::Arc;

// Custom-flavoured Markdown parser
#[derive(PartialEq, Debug, Clone)]
pub enum MarkdownContext {
    None,
    Default, // Usually P tag. Could also be a list item or something
    Heading(u32),

    // Bool is false if it's inside a P tag
    // If not, this is a naked emphasis tag
    Em(i32),
}

pub const HIDDEN_SKIP_CHAR: char = '\u{FFFC}';

#[derive(Debug)]
pub struct MarkdownTemplateFormatter;

impl TemplateFormatter for MarkdownTemplateFormatter {
    fn format(&self, content: &mut String) {
        *content = to_markdown(content, "p");
    }
}

pub fn markdown_formatter() -> Formatter {
    Formatter {
        id: "markdown",
        skip_if_already_formatted: false,
        formatter: Arc::new(MarkdownTemplateFormatter),
    }
}

pub fn to_markdown(content: &str, default_tag: &str) -> String {
    let mut context = MarkdownContext::None;
    const NEWLINES_BEFORE_NEW_P: usize = 2;
    const NEWLINES_BEFORE_BREAK: usize = 3;

    let chars: Vec<char> = content.chars().collect();
    let mut output = String::new();

    // Headings must be at the start of the line,
    // so we'll keep track of when we're at the start of a line
    // Any amount of indentation or tabs at the start of a line will be ignored
    let mut newlines = 0;
    let mut prev_whitespace = false;

    // Keeping track of how strong the special context is
    let mut heading_strength = 0;

    // If negative, then it's inside an emphasis tag and tracking the closing count
    let mut em_strength: i32 = 0;

    let mut skip_parsing = false;

    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];

        // Special object replace character that signals to ignore parsing a section into Markdown
        // This is used to ignore nested templates that have already been parsed
        // And may not be mark down. e.g. raw strings
        if ch == HIDDEN_SKIP_CHAR {
            skip_parsing = !skip_parsing;
            index += 1;
            continue;
        }
        // // Codeblock indicator character (invisible multiply)
        // if ch == '\u{2062}' {
        //     if !skip_parsing {
        //         output.push_str("</code>");
        //         skip_parsing = true;
        //     } else {
        //         output.push_str("<code>");
        //         skip_parsing = false;
        //     }
        //     continue;
        // }
        if skip_parsing {
            output.push(ch);
            index += 1;
            continue;
        }

        // HANDLING WHITESPACE
        // Ignore indentation on newlines
        if ch == '\t' || ch == ' ' {
            prev_whitespace = true;

            // Break out of em tags if it hasn't started yet
            // Must have the * immediately before the first character and after a space
            if em_strength > 0 {
                em_strength = 0;
            }

            // If spaces are after a newline, ignore them?
            // if newlines > 0 {
            //     continue
            // }

            // We are now making a heading
            if heading_strength > 0 {
                output.push_str(&format!("<h{}>", heading_strength));
                context = MarkdownContext::Heading(heading_strength);
                heading_strength = 0;
            } else {
                push_escaped_html_char(&mut output, ch);
            }

            index += 1;
            continue;
        }

        // Check for new lines
        if ch == '\n' {
            newlines += 1;
            prev_whitespace = true;

            // Newlines are stripped from the output
            // But if we build up enough of them, we need to add a break tag
            if newlines >= NEWLINES_BEFORE_BREAK {
                output.push_str("<br>");

                // Bring the newlines back to 1
                // As this is still considered a newline
                newlines = 1;
            }

            // Stop making our heading
            // Go back to P tag mode
            if let MarkdownContext::Heading(strength) = context {
                output.push_str(&format!("</h{}>", strength));
                context = MarkdownContext::None;
            }

            if let MarkdownContext::Default = context {
                // Close this P tag and start another one
                // If there are at least 2 newlines after the P tag
                if newlines >= NEWLINES_BEFORE_NEW_P {
                    output.push_str(&format!("</{default_tag}>"));
                    context = MarkdownContext::None;
                } else {
                    // Otherwise just add a space
                    // This is so you don't have to add a space before newlines in P tags
                    output.push(' ');
                }
            }

            index += 1;
            continue;
        }

        // HANDLING SPECIAL CHARACTERS

        // New heading
        // Don't switch context to heading until finished getting strength.
        // Once a heading marker sequence starts at the beginning of a line,
        // keep consuming consecutive '#' characters for strengths like '##'.
        if ch == '#' && (newlines > 0 || heading_strength > 0) {
            heading_strength += 1;
            prev_whitespace = false;
            newlines = 0;
            index += 1;
            continue;
        }

        if ch == '*' {
            // Already in emphasis
            // How negative the em strength is the number of consecutive * while inside an emphasis tag
            if let MarkdownContext::Em(strength) = context {
                em_strength -= 1;

                if strength == em_strength.abs() {
                    output.push_str(em_tag_strength(strength, true));

                    context = MarkdownContext::Default;

                    prev_whitespace = false;
                    em_strength = 0;
                }

                index += 1;
                continue;
            } else if prev_whitespace && em_strength >= 0 {
                // Possible new emphasis tag
                em_strength += 1;
                newlines = 0;

                index += 1;
                continue;
            }
        }

        // Start a new emphasis tag
        // Only resets if em_strength is positive so tags can be closed
        if em_strength > 0 {
            if let MarkdownContext::Default = context {
                context = MarkdownContext::Em(em_strength);
                output.push_str(em_tag_strength(em_strength, false));
            }

            if let MarkdownContext::None = context {
                context = MarkdownContext::Em(em_strength);
                output.push_str(&format!(
                    "<{default_tag}>{}",
                    em_tag_strength(em_strength, false)
                ));
            }

            em_strength = 0;
        }

        if ch == '@' {
            if let Some(link) = try_parse_link_at(&chars, index) {
                ensure_default_context(&mut output, &mut context, default_tag);
                flush_pending_markers(&mut output, &mut heading_strength, &mut em_strength);

                newlines = 0;
                prev_whitespace = false;
                output.push_str("<a href=\"");
                push_escaped_html_text(&mut output, &link.target);
                output.push_str("\">");
                push_escaped_html_text(&mut output, &link.label);
                output.push_str("</a>");
                index += link.consumed_chars;
                continue;
            }
        }

        // If nothing else special has happened, and we are not inside a P tag
        // Then start a new P tag
        ensure_default_context(&mut output, &mut context, default_tag);

        // If it's fallen through, then strengths and newlines can be reset

        // If heading strength or emphasis is positive (or negative for emphasis)
        // Before it's reset, those characters need to be added to the output
        flush_pending_markers(&mut output, &mut heading_strength, &mut em_strength);

        newlines = 0;
        prev_whitespace = false;
        push_escaped_html_char(&mut output, ch);
        index += 1;
    }

    // Close off the final tag if needed
    match context {
        MarkdownContext::Default => {
            output.push_str(&format!("</{default_tag}>"));
        }

        MarkdownContext::Heading(strength) => {
            output.push_str(&format!("</h{strength}>"));
        }

        MarkdownContext::Em(strength) => {
            output.push_str(em_tag_strength(strength, true));
        }

        MarkdownContext::None => {}
    }

    output
}

#[derive(Debug)]
struct ParsedMarkdownLink {
    target: String,
    label: String,
    consumed_chars: usize,
}

fn try_parse_link_at(chars: &[char], at_index: usize) -> Option<ParsedMarkdownLink> {
    if at_index >= chars.len() || chars[at_index] != '@' {
        return None;
    }

    if at_index > 0 {
        let prev = chars[at_index - 1];
        if prev != HIDDEN_SKIP_CHAR && !prev.is_whitespace() {
            return None;
        }
    }

    let target_start = at_index + 1;
    let mut cursor = target_start;
    if !consume_target_start(chars, &mut cursor) {
        return None;
    }

    while cursor < chars.len() && !chars[cursor].is_whitespace() {
        cursor += 1;
    }
    let target_end = cursor;
    if target_end == target_start {
        return None;
    }

    let spacing_start = cursor;
    while cursor < chars.len() && is_horizontal_whitespace(chars[cursor]) {
        cursor += 1;
    }
    if spacing_start == cursor {
        return None;
    }

    if cursor >= chars.len() || chars[cursor] != '(' {
        return None;
    }
    cursor += 1;

    let label_start = cursor;
    while cursor < chars.len() && chars[cursor] != ')' {
        cursor += 1;
    }
    if cursor >= chars.len() || chars[cursor] != ')' {
        return None;
    }

    let label = chars[label_start..cursor].iter().collect::<String>();
    if label.chars().all(char::is_whitespace) {
        return None;
    }

    let target = chars[target_start..target_end].iter().collect::<String>();

    Some(ParsedMarkdownLink {
        target,
        label,
        consumed_chars: cursor + 1 - at_index,
    })
}

fn consume_target_start(chars: &[char], cursor: &mut usize) -> bool {
    if *cursor >= chars.len() {
        return false;
    }

    let remaining = &chars[*cursor..];
    if starts_with_chars(remaining, &['/', '/']) {
        *cursor += 2;
        return true;
    }
    if starts_with_chars(remaining, &['.', '/']) {
        *cursor += 2;
        return true;
    }
    if starts_with_chars(remaining, &['.', '.', '/']) {
        *cursor += 3;
        return true;
    }

    match chars[*cursor] {
        '/' | '#' | '?' => {
            *cursor += 1;
            true
        }
        ch if ch.is_ascii_alphabetic() => consume_scheme_prefix(chars, cursor),
        _ => false,
    }
}

fn consume_scheme_prefix(chars: &[char], cursor: &mut usize) -> bool {
    if *cursor >= chars.len() || !chars[*cursor].is_ascii_alphabetic() {
        return false;
    }

    *cursor += 1;
    while *cursor < chars.len() && is_scheme_char(chars[*cursor]) {
        *cursor += 1;
    }

    if *cursor >= chars.len() || chars[*cursor] != ':' {
        return false;
    }

    *cursor += 1;
    true
}

fn is_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-')
}

fn starts_with_chars(input: &[char], prefix: &[char]) -> bool {
    input.starts_with(prefix)
}

fn is_horizontal_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

fn ensure_default_context(output: &mut String, context: &mut MarkdownContext, default_tag: &str) {
    if *context != MarkdownContext::None {
        return;
    }

    output.push_str(&format!("<{default_tag}>"));
    *context = MarkdownContext::Default;
}

fn flush_pending_markers(output: &mut String, heading_strength: &mut u32, em_strength: &mut i32) {
    if *heading_strength > 0 {
        output.push_str(&"#".repeat(*heading_strength as usize));
    }

    if *em_strength != 0 {
        output.push_str(&"*".repeat(em_strength.unsigned_abs() as usize));
    }

    *heading_strength = 0;
    *em_strength = 0;
}

fn push_escaped_html_text(output: &mut String, text: &str) {
    for ch in text.chars() {
        push_escaped_html_char(output, ch);
    }
}

fn push_escaped_html_char(output: &mut String, ch: char) {
    match ch {
        '<' => output.push_str("&lt;"),
        '>' => output.push_str("&gt;"),
        '&' => output.push_str("&amp;"),
        '"' => output.push_str("&quot;"),
        '\'' => output.push_str("&#39;"),
        _ => output.push(ch),
    }
}

fn em_tag_strength(strength: i32, closing: bool) -> &'static str {
    if closing {
        match strength {
            2 => "</strong>",
            3 => "</em></strong>",
            _ => "</em>",
        }
    } else {
        match strength {
            2 => "<strong>",
            3 => "<em><strong>",
            _ => "<em>",
        }
    }
}

#[cfg(test)]
#[path = "tests/markdown_tests.rs"]
mod markdown_tests;
