//! Built-in `$markdown` template style support.
//!
//! WHAT:
//! - Converts template body text into a narrow, deterministic HTML-flavoured markdown output.
//! - Supports unordered and ordered list blocks with indentation-based nesting.
//! - Preserves nested pre-formatted segments using a shared hidden guard marker.
//!
//! WHY:
//! - Templates need lightweight markdown support without adding a full markdown dependency.
//! - Nested template formatting must not be reparsed by parent markdown runs.

use crate::compiler_frontend::ast::templates::styles::TEMPLATE_FORMAT_GUARD_CHAR;
use crate::compiler_frontend::ast::templates::styles::whitespace::TemplateWhitespacePassProfile;
use crate::compiler_frontend::ast::templates::template::{Formatter, TemplateFormatter};
use std::sync::Arc;

/// Parser/render context for the lightweight markdown formatter.
#[derive(PartialEq, Debug, Clone)]
pub enum MarkdownContext {
    None,
    Default, // Usually P tag. Could also be a list item or something
    Heading(u32),

    // Bool is false if it's inside a P tag
    // If not, this is a naked emphasis tag
    Em(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownListKind {
    Unordered,
    Ordered,
}

impl MarkdownListKind {
    fn open_tag(self) -> &'static str {
        match self {
            Self::Unordered => "<ul>",
            Self::Ordered => "<ol>",
        }
    }

    fn close_tag(self) -> &'static str {
        match self {
            Self::Unordered => "</ul>",
            Self::Ordered => "</ol>",
        }
    }
}

#[derive(Debug)]
struct ParsedMarkdownListItemLine {
    indent_width: usize,
    kind: MarkdownListKind,
    content: String,
}

#[derive(Debug)]
struct MarkdownListLevel {
    indent_width: usize,
    kind: MarkdownListKind,
    has_open_item: bool,
}

/// Backward-compatible alias used by existing markdown tests/helpers.
pub const HIDDEN_SKIP_CHAR: char = TEMPLATE_FORMAT_GUARD_CHAR;

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
        // `$markdown` opts into the shared default body dedent/trim pass explicitly.
        pre_format_whitespace_passes: vec![TemplateWhitespacePassProfile::default_template_body()],
        formatter: Arc::new(MarkdownTemplateFormatter),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub fn to_markdown(content: &str, default_tag: &str) -> String {
    // Block parsing handles list structure first, while non-list blocks keep the
    // existing inline markdown behavior for headings/emphasis/links/escaping.
    to_markdown_with_lists(content, default_tag)
}

fn to_markdown_with_lists(content: &str, default_tag: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut index = 0usize;
    let mut output = String::new();
    let mut plain_buffer = String::new();

    while index < lines.len() {
        let line = lines[index];

        if parse_list_item_line(line).is_some() {
            flush_plain_block(&mut output, &mut plain_buffer, default_tag);

            let (rendered_list_block, consumed_lines) =
                render_list_block(&lines[index..], default_tag);
            if consumed_lines == 0 {
                // Defensive fallback to avoid stalling on malformed parser state.
                append_line_to_plain_buffer(&mut plain_buffer, line, index + 1 < lines.len());
                index += 1;
                continue;
            }

            output.push_str(&rendered_list_block);
            index += consumed_lines;
            continue;
        }

        append_line_to_plain_buffer(&mut plain_buffer, line, index + 1 < lines.len());
        index += 1;
    }

    flush_plain_block(&mut output, &mut plain_buffer, default_tag);
    output
}

fn append_line_to_plain_buffer(buffer: &mut String, line: &str, append_newline: bool) {
    buffer.push_str(line);
    if append_newline {
        buffer.push('\n');
    }
}

fn flush_plain_block(output: &mut String, plain_buffer: &mut String, default_tag: &str) {
    if plain_buffer.is_empty() {
        return;
    }

    output.push_str(&to_markdown_inline(plain_buffer, default_tag));
    plain_buffer.clear();
}

fn render_list_block(lines: &[&str], default_tag: &str) -> (String, usize) {
    let mut output = String::new();
    let mut list_stack: Vec<MarkdownListLevel> = Vec::new();
    let mut consumed_lines = 0usize;

    while consumed_lines < lines.len() {
        let line = lines[consumed_lines];

        // A blank separator line ends the list block.
        if line.chars().all(char::is_whitespace) {
            break;
        }

        // Headings break out of list mode immediately, even without an empty line.
        if line_starts_heading(line) {
            break;
        }

        if let Some(list_item) = parse_list_item_line(line) {
            append_list_item_to_output(&mut output, &mut list_stack, &list_item, default_tag);
            consumed_lines += 1;
            continue;
        }

        // Non-list lines directly following a list item are treated as continuation
        // text for that item, matching this markdown flavor's newline behavior.
        if append_list_item_continuation_line(&mut output, &list_stack, line, default_tag) {
            consumed_lines += 1;
            continue;
        }

        break;
    }

    close_all_list_levels(&mut output, &mut list_stack);
    (output, consumed_lines)
}

fn append_list_item_to_output(
    output: &mut String,
    list_stack: &mut Vec<MarkdownListLevel>,
    list_item: &ParsedMarkdownListItemLine,
    default_tag: &str,
) {
    // Dedent closes nested lists until this item can attach at its indentation level.
    while let Some(top_level) = list_stack.last() {
        if list_item.indent_width < top_level.indent_width {
            close_current_list_level(output, list_stack);
            continue;
        }
        break;
    }

    // Same-level kind switches close the prior list before opening the new list kind.
    if let Some(top_level) = list_stack.last_mut()
        && list_item.indent_width == top_level.indent_width
    {
        if list_item.kind != top_level.kind {
            close_current_list_level(output, list_stack);
        } else if top_level.has_open_item {
            output.push_str("</li>");
            top_level.has_open_item = false;
        }
    }

    // A deeper indentation opens a nested list inside the currently open list item.
    let needs_new_level = list_stack.last().is_none_or(|top_level| {
        list_item.indent_width > top_level.indent_width || list_item.kind != top_level.kind
    });
    if needs_new_level {
        output.push_str(list_item.kind.open_tag());
        list_stack.push(MarkdownListLevel {
            indent_width: list_item.indent_width,
            kind: list_item.kind,
            has_open_item: false,
        });
    }

    let list_level = list_stack
        .last_mut()
        .expect("list level should exist after opening/appending item");

    if list_level.has_open_item {
        output.push_str("</li>");
    }

    output.push_str("<li>");
    output.push_str(&render_list_item_content(&list_item.content, default_tag));
    list_level.has_open_item = true;
}

fn close_current_list_level(output: &mut String, list_stack: &mut Vec<MarkdownListLevel>) {
    let Some(level) = list_stack.pop() else {
        return;
    };

    if level.has_open_item {
        output.push_str("</li>");
    }
    output.push_str(level.kind.close_tag());
}

fn close_all_list_levels(output: &mut String, list_stack: &mut Vec<MarkdownListLevel>) {
    while !list_stack.is_empty() {
        close_current_list_level(output, list_stack);
    }
}

fn append_list_item_continuation_line(
    output: &mut String,
    list_stack: &[MarkdownListLevel],
    line: &str,
    default_tag: &str,
) -> bool {
    let Some(current_level) = list_stack.last() else {
        return false;
    };
    if !current_level.has_open_item {
        return false;
    }

    let continuation_text = line.trim();
    if continuation_text.is_empty() {
        return false;
    }

    let rendered = render_list_item_content(continuation_text, default_tag);
    if rendered.is_empty() {
        return false;
    }

    // Newlines inside list items collapse to a single separating space.
    output.push(' ');
    output.push_str(&rendered);
    true
}

fn line_starts_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let mut hash_count = 0usize;

    for ch in trimmed.chars() {
        if ch == '#' {
            hash_count += 1;
            continue;
        }

        return hash_count > 0 && ch.is_whitespace();
    }

    false
}

fn parse_list_item_line(line: &str) -> Option<ParsedMarkdownListItemLine> {
    if line.chars().all(char::is_whitespace) {
        return None;
    }

    let (indent_width, start_index) = consume_line_indentation(line);
    let remainder = &line[start_index..];
    if remainder.is_empty() {
        return None;
    }

    if let Some(item) = parse_unordered_list_item(remainder, indent_width) {
        return Some(item);
    }

    parse_ordered_list_item(remainder, indent_width)
}

fn consume_line_indentation(line: &str) -> (usize, usize) {
    let mut indent_width = 0usize;
    let mut start_index = 0usize;

    for (index, ch) in line.char_indices() {
        match ch {
            ' ' => {
                indent_width += 1;
                start_index = index + ch.len_utf8();
            }
            '\t' => {
                // Tabs are treated as one indentation level chunk for nested list detection.
                indent_width += 4;
                start_index = index + ch.len_utf8();
            }
            _ => {
                break;
            }
        }
    }

    (indent_width, start_index)
}

fn parse_unordered_list_item(
    remainder: &str,
    indent_width: usize,
) -> Option<ParsedMarkdownListItemLine> {
    let mut chars = remainder.char_indices();
    let (_, marker) = chars.next()?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }

    let (separator_index, separator) = chars.next()?;
    if !separator.is_whitespace() {
        return None;
    }

    let content_start = separator_index + separator.len_utf8();
    let content = remainder[content_start..]
        .trim_start()
        .trim_end()
        .to_owned();
    Some(ParsedMarkdownListItemLine {
        indent_width,
        kind: MarkdownListKind::Unordered,
        content,
    })
}

fn parse_ordered_list_item(
    remainder: &str,
    indent_width: usize,
) -> Option<ParsedMarkdownListItemLine> {
    let bytes = remainder.as_bytes();
    let mut cursor = 0usize;

    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }

    if cursor == 0 || cursor >= bytes.len() {
        return None;
    }

    if !matches!(bytes[cursor], b'.' | b')') {
        return None;
    }
    cursor += 1;

    if cursor >= bytes.len() {
        return None;
    }

    let separator = remainder[cursor..].chars().next()?;
    if !separator.is_whitespace() {
        return None;
    }
    cursor += separator.len_utf8();

    let content = remainder[cursor..].trim_start().trim_end().to_owned();
    Some(ParsedMarkdownListItemLine {
        indent_width,
        kind: MarkdownListKind::Ordered,
        content,
    })
}

fn render_list_item_content(content: &str, default_tag: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    let rendered = to_markdown_inline(content, default_tag);
    unwrap_single_default_tag_block(rendered, default_tag)
}

fn unwrap_single_default_tag_block(rendered: String, default_tag: &str) -> String {
    let open_tag = format!("<{default_tag}>");
    let close_tag = format!("</{default_tag}>");
    let split_token = format!("{close_tag}{open_tag}");

    if !rendered.starts_with(&open_tag) || !rendered.ends_with(&close_tag) {
        return rendered;
    }

    if rendered.contains(&split_token) {
        return rendered;
    }

    rendered[open_tag.len()..rendered.len() - close_tag.len()].to_owned()
}

fn to_markdown_inline(content: &str, default_tag: &str) -> String {
    let mut context = MarkdownContext::None;
    const NEWLINES_BEFORE_NEW_P: usize = 2;
    const NEWLINES_BEFORE_BREAK: usize = 3;

    let chars: Vec<char> = content.chars().collect();
    let mut output = String::new();

    // Headings must be at the start of the line, so we'll keep track of when
    // we're at the start of a line.
    let mut newlines = 0;
    let mut prev_whitespace = false;

    // Keeping track of how strong the special context is.
    let mut heading_strength = 0;

    // If negative, then it's inside an emphasis tag and tracking the closing count.
    let mut em_strength: i32 = 0;

    let mut skip_parsing = false;

    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];

        // Special object replace character that signals to ignore parsing a section
        // into markdown. This preserves nested formatted template segments.
        if ch == HIDDEN_SKIP_CHAR {
            skip_parsing = !skip_parsing;
            index += 1;
            continue;
        }

        if skip_parsing {
            output.push(ch);
            index += 1;
            continue;
        }

        // HANDLING WHITESPACE
        if ch == '\t' || ch == ' ' {
            prev_whitespace = true;

            // Break out of em tags if it hasn't started yet.
            if em_strength > 0 {
                em_strength = 0;
            }

            // Heading marker sequence completed.
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

        // Check for new lines.
        if ch == '\n' {
            newlines += 1;
            prev_whitespace = true;

            // Newlines are stripped from the output, but if we build up enough of
            // them we inject a break tag.
            if newlines >= NEWLINES_BEFORE_BREAK {
                output.push_str("<br>");
                newlines = 1;
            }

            // Stop making our heading and return to the default context.
            if let MarkdownContext::Heading(strength) = context {
                output.push_str(&format!("</h{}>", strength));
                context = MarkdownContext::None;
            }

            if let MarkdownContext::Default = context {
                // Two+ newlines close the paragraph.
                if newlines >= NEWLINES_BEFORE_NEW_P {
                    output.push_str(&format!("</{default_tag}>"));
                    context = MarkdownContext::None;
                } else {
                    output.push(' ');
                }
            }

            index += 1;
            continue;
        }

        // HANDLING SPECIAL CHARACTERS

        // Heading markers at line start.
        if ch == '#' && (index == 0 || newlines > 0 || heading_strength > 0) {
            heading_strength += 1;
            prev_whitespace = false;
            newlines = 0;
            index += 1;
            continue;
        }

        if ch == '*' {
            // Already in emphasis.
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
                // Possible new emphasis tag.
                em_strength += 1;
                newlines = 0;

                index += 1;
                continue;
            }
        }

        // Start a new emphasis tag.
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

        if ch == '@'
            && let Some(link) = try_parse_link_at(&chars, index)
        {
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

        // If nothing else special has happened and we're not inside a paragraph,
        // start a new default block.
        ensure_default_context(&mut output, &mut context, default_tag);

        // If heading or emphasis markers were pending, push them literally before reset.
        flush_pending_markers(&mut output, &mut heading_strength, &mut em_strength);

        newlines = 0;
        prev_whitespace = false;
        push_escaped_html_char(&mut output, ch);
        index += 1;
    }

    // Close off the final tag if needed.
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
#[path = "../tests/markdown_tests.rs"]
mod markdown_tests;
