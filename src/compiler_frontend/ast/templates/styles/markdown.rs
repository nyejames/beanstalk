//! Built-in `$markdown` template style support.
//!
//! WHAT:
//! - Converts template body text into a narrow, deterministic HTML-flavoured markdown output.
//! - Supports unordered and ordered list blocks with indentation-based nesting.
//! - Preserves child-template and dynamic-expression anchors as opaque inline/block boundaries.
//!
//! WHY:
//! - Templates need lightweight markdown support without adding a full markdown dependency.
//! - Parent markdown runs must keep child template output sealed while still preserving
//!   paragraph and list structure across opaque anchors.

use crate::compiler_frontend::ast::templates::styles::whitespace::TemplateWhitespacePassProfile;
use crate::compiler_frontend::ast::templates::template::{
    Formatter, FormatterResult, TemplateFormatter,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterInput, FormatterInputPiece, FormatterOpaqueKind, FormatterOpaquePiece,
    FormatterOutput, FormatterOutputPiece,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveArgumentValue;
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownInlineAtom {
    Char(char),
    Opaque(FormatterOpaquePiece),
}

#[derive(Clone, Debug, Default)]
struct MarkdownLine {
    atoms: Vec<MarkdownInlineAtom>,
}

#[derive(Debug)]
struct ParsedMarkdownListItemLine {
    indent_width: usize,
    kind: MarkdownListKind,
    content: Vec<MarkdownInlineAtom>,
}

#[derive(Debug)]
struct ParsedMarkdownHeadingLine {
    level: usize,
    content: Vec<MarkdownInlineAtom>,
}

#[derive(Debug, Clone)]
enum MarkdownListItemFragment {
    Line(Vec<MarkdownInlineAtom>),
    NestedList(Vec<FormatterOutputPiece>),
}

#[derive(Debug, Clone)]
enum MarkdownListItemBlock {
    Paragraph(Vec<Vec<MarkdownInlineAtom>>),
    StandaloneInline(Vec<MarkdownInlineAtom>),
    NestedList(Vec<FormatterOutputPiece>),
}

#[derive(Debug)]
struct ParsedMarkdownLink {
    target: String,
    label: String,
    consumed_atoms: usize,
}

#[derive(Debug, Default)]
struct MarkdownOutputBuilder {
    pieces: Vec<FormatterOutputPiece>,
    text_buffer: String,
}

impl MarkdownOutputBuilder {
    fn push_raw(&mut self, text: &str) {
        self.text_buffer.push_str(text);
    }

    fn push_escaped_char(&mut self, ch: char) {
        match ch {
            '<' => self.text_buffer.push_str("&lt;"),
            '>' => self.text_buffer.push_str("&gt;"),
            '&' => self.text_buffer.push_str("&amp;"),
            '"' => self.text_buffer.push_str("&quot;"),
            '\'' => self.text_buffer.push_str("&#39;"),
            _ => self.text_buffer.push(ch),
        }
    }

    fn push_escaped_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.push_escaped_char(ch);
        }
    }

    fn push_opaque(&mut self, anchor: FormatterOpaquePiece) {
        self.flush_text();
        self.pieces.push(FormatterOutputPiece::Opaque(anchor));
    }

    fn append_pieces(&mut self, pieces: Vec<FormatterOutputPiece>) {
        for piece in pieces {
            match piece {
                FormatterOutputPiece::Text(text) => self.text_buffer.push_str(&text),
                FormatterOutputPiece::Opaque(anchor) => self.push_opaque(anchor),
            }
        }
    }

    fn finish(mut self) -> Vec<FormatterOutputPiece> {
        self.flush_text();
        self.pieces
    }

    fn flush_text(&mut self) {
        if self.text_buffer.is_empty() {
            return;
        }

        self.pieces.push(FormatterOutputPiece::Text(std::mem::take(
            &mut self.text_buffer,
        )));
    }
}

#[derive(Debug)]
pub struct MarkdownTemplateFormatter;

impl TemplateFormatter for MarkdownTemplateFormatter {
    fn format(
        &self,
        input: FormatterInput,
        string_table: &mut StringTable,
    ) -> Result<FormatterResult, CompilerMessages> {
        let lines = split_formatter_input_into_lines(input, string_table);
        let pieces = render_markdown_stream(&lines, "p");

        Ok(FormatterResult {
            output: FormatterOutput { pieces },
            warnings: Vec::new(),
        })
    }
}

pub fn markdown_formatter() -> Formatter {
    Formatter {
        // `$markdown` opts into the shared default body dedent/trim pass explicitly.
        pre_format_whitespace_passes: vec![TemplateWhitespacePassProfile::default_template_body()],
        formatter: Arc::new(MarkdownTemplateFormatter),
        post_format_whitespace_passes: Vec::new(),
    }
}

pub(crate) fn markdown_formatter_factory(
    argument: Option<&StyleDirectiveArgumentValue>,
) -> Result<Option<Formatter>, String> {
    if argument.is_some() {
        return Err("'$markdown' does not accept arguments.".to_string());
    }

    Ok(Some(markdown_formatter()))
}

/// Converts formatter input into newline-delimited markdown lines without flattening anchors.
///
/// WHAT:
/// - Splits text pieces on `\n`.
/// - Preserves opaque anchors inline in the current line.
///
/// WHY:
/// - Block parsing needs line boundaries for headings/lists while inline rendering still
///   needs anchored child/dynamic pieces in the original order.
fn split_formatter_input_into_lines(
    input: FormatterInput,
    string_table: &StringTable,
) -> Vec<MarkdownLine> {
    let mut lines = vec![MarkdownLine::default()];

    for piece in input.pieces {
        match piece {
            FormatterInputPiece::Text(text_piece) => {
                for ch in string_table.resolve(text_piece.text).chars() {
                    if ch == '\n' {
                        lines.push(MarkdownLine::default());
                    } else {
                        lines
                            .last_mut()
                            .expect("line buffer should exist while splitting markdown input")
                            .atoms
                            .push(MarkdownInlineAtom::Char(ch));
                    }
                }
            }
            FormatterInputPiece::Opaque(anchor) => {
                lines
                    .last_mut()
                    .expect("line buffer should exist while splitting markdown input")
                    .atoms
                    .push(MarkdownInlineAtom::Opaque(anchor));
            }
        }
    }

    lines
}

#[cfg_attr(not(test), allow(dead_code))]
fn split_text_into_lines(content: &str) -> Vec<MarkdownLine> {
    let mut lines = vec![MarkdownLine::default()];

    for ch in content.chars() {
        if ch == '\n' {
            lines.push(MarkdownLine::default());
        } else {
            lines
                .last_mut()
                .expect("line buffer should exist while splitting markdown text")
                .atoms
                .push(MarkdownInlineAtom::Char(ch));
        }
    }

    lines
}

/// Renders the full markdown line stream while keeping block/list state across anchors.
fn render_markdown_stream(lines: &[MarkdownLine], default_tag: &str) -> Vec<FormatterOutputPiece> {
    let mut output = MarkdownOutputBuilder::default();
    let mut index = 0usize;
    let mut has_rendered_block = false;

    while index < lines.len() {
        if line_is_blank(&lines[index]) {
            let mut blank_run = 0usize;
            while index + blank_run < lines.len() && line_is_blank(&lines[index + blank_run]) {
                blank_run += 1;
            }

            let break_count = if has_rendered_block {
                blank_run / 2
            } else {
                blank_run.saturating_sub(1) / 2
            };
            append_break_tags(&mut output, break_count);
            index += blank_run;
            continue;
        }

        if parse_list_item_line(&lines[index]).is_some() {
            let (rendered, consumed_lines) = render_list_block(&lines[index..], default_tag);
            if consumed_lines == 0 {
                break;
            }

            output.append_pieces(rendered);
            has_rendered_block = true;
            index += consumed_lines;
            continue;
        }

        if let Some(heading) = parse_heading_line(&lines[index]) {
            output.append_pieces(render_heading_line(&heading));
            has_rendered_block = true;
            index += 1;
            continue;
        }

        let region_start = index;
        while index < lines.len() {
            if line_is_blank(&lines[index])
                || parse_list_item_line(&lines[index]).is_some()
                || parse_heading_line(&lines[index]).is_some()
            {
                break;
            }
            index += 1;
        }

        output.append_pieces(render_plain_region(
            &lines[region_start..index],
            default_tag,
        ));
        has_rendered_block = true;
    }

    output.finish()
}

/// Groups plain non-list lines into paragraphs, applying the child-only newline rule.
///
/// WHAT:
/// - Consecutive text/dynamic lines stay in the current paragraph and join with spaces.
/// - A line whose first significant piece is a child-template anchor forces the prior
///   paragraph to close, then renders that line outside the paragraph.
///
/// WHY:
/// - `$markdown` needs to keep child templates opaque while still letting a single
///   newline before a child break paragraph context.
fn render_plain_region(lines: &[MarkdownLine], default_tag: &str) -> Vec<FormatterOutputPiece> {
    enum PlainRegionBlock {
        Paragraph(Vec<Vec<MarkdownInlineAtom>>),
        StandaloneInline(Vec<MarkdownInlineAtom>),
    }

    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<Vec<MarkdownInlineAtom>> = Vec::new();

    for line in lines {
        if line_starts_child_template(&line.atoms) {
            if !paragraph_lines.is_empty() {
                blocks.push(PlainRegionBlock::Paragraph(std::mem::take(
                    &mut paragraph_lines,
                )));
            }

            blocks.push(PlainRegionBlock::StandaloneInline(
                trim_leading_horizontal_whitespace(&line.atoms),
            ));
        } else {
            paragraph_lines.push(line.atoms.clone());
        }
    }

    if !paragraph_lines.is_empty() {
        blocks.push(PlainRegionBlock::Paragraph(paragraph_lines));
    }

    let mut output = MarkdownOutputBuilder::default();
    for block in blocks {
        match block {
            PlainRegionBlock::Paragraph(lines) => {
                let atoms = join_lines_with_spaces(&lines);
                output.append_pieces(render_inline_atoms(&atoms, Some(default_tag), true));
            }
            PlainRegionBlock::StandaloneInline(atoms) => {
                output.append_pieces(render_inline_atoms(&atoms, Some(default_tag), false));
            }
        }
    }

    output.finish()
}

/// Parses a list block recursively so nested list items keep mixed content in one `<li>`.
fn render_list_block(
    lines: &[MarkdownLine],
    default_tag: &str,
) -> (Vec<FormatterOutputPiece>, usize) {
    #[derive(Debug)]
    struct MarkdownRenderedListSection {
        kind: MarkdownListKind,
        items: Vec<Vec<FormatterOutputPiece>>,
    }

    let Some(first_line) = lines.first() else {
        return (Vec::new(), 0);
    };
    let Some(first_item) = parse_list_item_line(first_line) else {
        return (Vec::new(), 0);
    };
    let current_indent = first_item.indent_width;

    let mut sections: Vec<MarkdownRenderedListSection> = Vec::new();
    let mut consumed_lines = 0usize;

    while consumed_lines < lines.len() {
        let line = &lines[consumed_lines];
        if line_is_blank(line) || parse_heading_line(line).is_some() {
            break;
        }

        let Some(list_item) = parse_list_item_line(line) else {
            break;
        };

        if list_item.indent_width != current_indent {
            break;
        }

        if sections.last().map(|section| section.kind) != Some(list_item.kind) {
            sections.push(MarkdownRenderedListSection {
                kind: list_item.kind,
                items: Vec::new(),
            });
        }

        consumed_lines += 1;
        let mut fragments = vec![MarkdownListItemFragment::Line(list_item.content)];

        while consumed_lines < lines.len() {
            let next_line = &lines[consumed_lines];
            if line_is_blank(next_line) || parse_heading_line(next_line).is_some() {
                break;
            }

            if let Some(next_item) = parse_list_item_line(next_line) {
                if next_item.indent_width <= current_indent {
                    break;
                }

                let (nested_rendered, nested_consumed) =
                    render_list_block(&lines[consumed_lines..], default_tag);
                if nested_consumed == 0 {
                    break;
                }

                fragments.push(MarkdownListItemFragment::NestedList(nested_rendered));
                consumed_lines += nested_consumed;
                continue;
            }

            fragments.push(MarkdownListItemFragment::Line(trim_atoms(&next_line.atoms)));
            consumed_lines += 1;
        }

        let rendered_item = render_list_item_fragments(&fragments, default_tag);
        sections
            .last_mut()
            .expect("list section should exist before pushing items")
            .items
            .push(rendered_item);
    }

    let mut output = MarkdownOutputBuilder::default();
    for section in sections {
        output.push_raw(section.kind.open_tag());

        for item in section.items {
            output.push_raw("<li>");
            output.append_pieces(item);
            output.push_raw("</li>");
        }

        output.push_raw(section.kind.close_tag());
    }

    (output.finish(), consumed_lines)
}

/// Converts mixed item fragments into paragraph/list blocks before rendering.
fn render_list_item_fragments(
    fragments: &[MarkdownListItemFragment],
    default_tag: &str,
) -> Vec<FormatterOutputPiece> {
    let blocks = build_list_item_blocks(fragments);
    let paragraph_block_count = blocks
        .iter()
        .filter(|block| matches!(block, MarkdownListItemBlock::Paragraph(_)))
        .count();
    let should_wrap_paragraphs = paragraph_block_count > 1
        || blocks
            .iter()
            .any(|block| matches!(block, MarkdownListItemBlock::StandaloneInline(_)));

    let mut output = MarkdownOutputBuilder::default();
    for block in blocks {
        match block {
            MarkdownListItemBlock::Paragraph(lines) => {
                let atoms = join_lines_with_spaces(&lines);
                if should_wrap_paragraphs {
                    output.append_pieces(render_inline_atoms(&atoms, Some(default_tag), true));
                } else {
                    output.append_pieces(render_inline_atoms(&atoms, None, false));
                }
            }
            MarkdownListItemBlock::StandaloneInline(atoms) => {
                output.append_pieces(render_inline_atoms(&atoms, Some(default_tag), false));
            }
            MarkdownListItemBlock::NestedList(pieces) => {
                output.append_pieces(pieces);
            }
        }
    }

    output.finish()
}

fn build_list_item_blocks(fragments: &[MarkdownListItemFragment]) -> Vec<MarkdownListItemBlock> {
    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<Vec<MarkdownInlineAtom>> = Vec::new();

    for fragment in fragments {
        match fragment {
            MarkdownListItemFragment::Line(atoms) if line_starts_child_template(atoms) => {
                if !paragraph_lines.is_empty() {
                    blocks.push(MarkdownListItemBlock::Paragraph(std::mem::take(
                        &mut paragraph_lines,
                    )));
                }

                blocks.push(MarkdownListItemBlock::StandaloneInline(
                    trim_leading_horizontal_whitespace(atoms),
                ));
            }
            MarkdownListItemFragment::Line(atoms) => {
                paragraph_lines.push(atoms.clone());
            }
            MarkdownListItemFragment::NestedList(pieces) => {
                if !paragraph_lines.is_empty() {
                    blocks.push(MarkdownListItemBlock::Paragraph(std::mem::take(
                        &mut paragraph_lines,
                    )));
                }

                blocks.push(MarkdownListItemBlock::NestedList(pieces.clone()));
            }
        }
    }

    if !paragraph_lines.is_empty() {
        blocks.push(MarkdownListItemBlock::Paragraph(paragraph_lines));
    }

    blocks
}

fn render_heading_line(heading: &ParsedMarkdownHeadingLine) -> Vec<FormatterOutputPiece> {
    let heading_tag = format!("h{}", heading.level);
    render_inline_atoms(&heading.content, Some(heading_tag.as_str()), true)
}

/// Renders inline markdown atoms into escaped HTML and preserved opaque anchors.
///
/// WHAT:
/// - Escapes text, parses the markdown link syntax, and maintains a narrow emphasis
///   state machine across both text and opaque anchors.
/// - Supports lazy wrapper opening so child-template-leading lines can render an
///   anchor first and only open `<p>` when later text appears.
///
/// WHY:
/// - Inline formatting needs to stay structurally continuous across child/dynamic
///   anchors without flattening them into temporary strings.
fn render_inline_atoms(
    atoms: &[MarkdownInlineAtom],
    wrapper_tag: Option<&str>,
    open_wrapper_immediately: bool,
) -> Vec<FormatterOutputPiece> {
    let mut output = MarkdownOutputBuilder::default();
    let mut wrapper_open = false;
    let mut emphasis_strength: Option<usize> = None;
    let mut pending_open_strength = 0usize;
    let mut prev_whitespace = true;
    let mut index = 0usize;

    if open_wrapper_immediately {
        open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
    }

    while index < atoms.len() {
        if pending_open_strength > 0
            && !matches!(atoms[index], MarkdownInlineAtom::Char(' ' | '\t' | '*'))
        {
            open_pending_emphasis(
                &mut output,
                wrapper_tag,
                &mut wrapper_open,
                &mut emphasis_strength,
                &mut pending_open_strength,
            );
        }

        match atoms[index] {
            MarkdownInlineAtom::Opaque(anchor) => {
                output.push_opaque(anchor);
                prev_whitespace = false;
                index += 1;
            }
            MarkdownInlineAtom::Char(' ' | '\t') => {
                literalize_pending_stars(
                    &mut output,
                    wrapper_tag,
                    &mut wrapper_open,
                    &mut pending_open_strength,
                );
                output
                    .push_escaped_char(atom_char(atoms, index).expect("whitespace char expected"));
                prev_whitespace = true;
                index += 1;
            }
            MarkdownInlineAtom::Char('\n') | MarkdownInlineAtom::Char('\r') => {
                literalize_pending_stars(
                    &mut output,
                    wrapper_tag,
                    &mut wrapper_open,
                    &mut pending_open_strength,
                );
                prev_whitespace = true;
                index += 1;
            }
            MarkdownInlineAtom::Char('*') => {
                let star_run = count_consecutive_star_chars(atoms, index);

                if let Some(active_strength) = emphasis_strength {
                    if star_run >= active_strength {
                        output.push_raw(em_tag_strength(active_strength as i32, true));
                        emphasis_strength = None;
                        prev_whitespace = false;
                        index += active_strength;
                    } else {
                        open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
                        output.push_raw(&"*".repeat(star_run));
                        prev_whitespace = false;
                        index += star_run;
                    }
                    continue;
                }

                if prev_whitespace && (1..=3).contains(&star_run) {
                    pending_open_strength = star_run;
                    index += star_run;
                    continue;
                }

                literalize_pending_stars(
                    &mut output,
                    wrapper_tag,
                    &mut wrapper_open,
                    &mut pending_open_strength,
                );
                open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
                output.push_raw(&"*".repeat(star_run));
                prev_whitespace = false;
                index += star_run;
            }
            MarkdownInlineAtom::Char('@') if prev_whitespace => {
                if let Some(link) = try_parse_link_at_atoms(atoms, index) {
                    open_pending_emphasis(
                        &mut output,
                        wrapper_tag,
                        &mut wrapper_open,
                        &mut emphasis_strength,
                        &mut pending_open_strength,
                    );
                    open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
                    output.push_raw("<a href=\"");
                    output.push_escaped_text(&link.target);
                    output.push_raw("\">");
                    output.push_escaped_text(&link.label);
                    output.push_raw("</a>");
                    prev_whitespace = false;
                    index += link.consumed_atoms;
                    continue;
                }

                open_pending_emphasis(
                    &mut output,
                    wrapper_tag,
                    &mut wrapper_open,
                    &mut emphasis_strength,
                    &mut pending_open_strength,
                );
                open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
                output.push_escaped_char('@');
                prev_whitespace = false;
                index += 1;
            }
            MarkdownInlineAtom::Char(ch) => {
                open_pending_emphasis(
                    &mut output,
                    wrapper_tag,
                    &mut wrapper_open,
                    &mut emphasis_strength,
                    &mut pending_open_strength,
                );
                open_wrapper(&mut output, wrapper_tag, &mut wrapper_open);
                output.push_escaped_char(ch);
                prev_whitespace = false;
                index += 1;
            }
        }
    }

    literalize_pending_stars(
        &mut output,
        wrapper_tag,
        &mut wrapper_open,
        &mut pending_open_strength,
    );

    if let Some(active_strength) = emphasis_strength {
        output.push_raw(em_tag_strength(active_strength as i32, true));
    }

    if wrapper_open && let Some(tag) = wrapper_tag {
        output.push_raw(&format!("</{tag}>"));
    }

    output.finish()
}

fn open_wrapper(
    output: &mut MarkdownOutputBuilder,
    wrapper_tag: Option<&str>,
    wrapper_open: &mut bool,
) {
    if *wrapper_open {
        return;
    }

    if let Some(tag) = wrapper_tag {
        output.push_raw(&format!("<{tag}>"));
        *wrapper_open = true;
    }
}

fn open_pending_emphasis(
    output: &mut MarkdownOutputBuilder,
    wrapper_tag: Option<&str>,
    wrapper_open: &mut bool,
    emphasis_strength: &mut Option<usize>,
    pending_open_strength: &mut usize,
) {
    if *pending_open_strength == 0 {
        return;
    }

    open_wrapper(output, wrapper_tag, wrapper_open);
    output.push_raw(em_tag_strength(*pending_open_strength as i32, false));
    *emphasis_strength = Some(*pending_open_strength);
    *pending_open_strength = 0;
}

fn literalize_pending_stars(
    output: &mut MarkdownOutputBuilder,
    wrapper_tag: Option<&str>,
    wrapper_open: &mut bool,
    pending_open_strength: &mut usize,
) {
    if *pending_open_strength == 0 {
        return;
    }

    open_wrapper(output, wrapper_tag, wrapper_open);
    output.push_raw(&"*".repeat(*pending_open_strength));
    *pending_open_strength = 0;
}

fn append_break_tags(output: &mut MarkdownOutputBuilder, break_count: usize) {
    for _ in 0..break_count {
        output.push_raw("<br>");
    }
}

fn line_is_blank(line: &MarkdownLine) -> bool {
    line.atoms.iter().all(|atom| match atom {
        MarkdownInlineAtom::Char(ch) => ch.is_whitespace(),
        MarkdownInlineAtom::Opaque(_) => false,
    })
}

fn line_starts_child_template(atoms: &[MarkdownInlineAtom]) -> bool {
    match atoms.get(skip_leading_horizontal_whitespace(atoms)) {
        Some(MarkdownInlineAtom::Opaque(anchor)) => {
            anchor.kind == FormatterOpaqueKind::ChildTemplate
        }
        _ => false,
    }
}

fn parse_heading_line(line: &MarkdownLine) -> Option<ParsedMarkdownHeadingLine> {
    let start = skip_leading_horizontal_whitespace(&line.atoms);
    let mut index = start;
    let mut level = 0usize;

    while let Some('#') = atom_char(&line.atoms, index) {
        level += 1;
        index += 1;
    }

    if level == 0 {
        return None;
    }

    let separator = atom_char(&line.atoms, index)?;
    if !separator.is_whitespace() {
        return None;
    }

    Some(ParsedMarkdownHeadingLine {
        level,
        content: line.atoms[index + 1..].to_vec(),
    })
}

fn parse_list_item_line(line: &MarkdownLine) -> Option<ParsedMarkdownListItemLine> {
    if line_is_blank(line) {
        return None;
    }

    let (indent_width, start_index) = consume_line_indentation(&line.atoms);

    if let Some(item) = parse_unordered_list_item(&line.atoms, start_index, indent_width) {
        return Some(item);
    }

    parse_ordered_list_item(&line.atoms, start_index, indent_width)
}

fn consume_line_indentation(atoms: &[MarkdownInlineAtom]) -> (usize, usize) {
    let mut indent_width = 0usize;
    let mut index = 0usize;

    while let Some(atom) = atoms.get(index) {
        match atom {
            MarkdownInlineAtom::Char(' ') => {
                indent_width += 1;
                index += 1;
            }
            MarkdownInlineAtom::Char('\t') => {
                // Tabs are treated as a single indentation chunk for nested list parsing.
                indent_width += 4;
                index += 1;
            }
            _ => break,
        }
    }

    (indent_width, index)
}

fn parse_unordered_list_item(
    atoms: &[MarkdownInlineAtom],
    start_index: usize,
    indent_width: usize,
) -> Option<ParsedMarkdownListItemLine> {
    let marker = atom_char(atoms, start_index)?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }

    let separator = atom_char(atoms, start_index + 1)?;
    if !separator.is_whitespace() {
        return None;
    }

    Some(ParsedMarkdownListItemLine {
        indent_width,
        kind: MarkdownListKind::Unordered,
        content: trim_atoms(&atoms[start_index + 2..]),
    })
}

fn parse_ordered_list_item(
    atoms: &[MarkdownInlineAtom],
    start_index: usize,
    indent_width: usize,
) -> Option<ParsedMarkdownListItemLine> {
    let mut index = start_index;

    while let Some(ch) = atom_char(atoms, index) {
        if !ch.is_ascii_digit() {
            break;
        }
        index += 1;
    }

    if index == start_index {
        return None;
    }

    let marker = atom_char(atoms, index)?;
    if !matches!(marker, '.' | ')') {
        return None;
    }

    let separator = atom_char(atoms, index + 1)?;
    if !separator.is_whitespace() {
        return None;
    }

    Some(ParsedMarkdownListItemLine {
        indent_width,
        kind: MarkdownListKind::Ordered,
        content: trim_atoms(&atoms[index + 2..]),
    })
}

fn join_lines_with_spaces(lines: &[Vec<MarkdownInlineAtom>]) -> Vec<MarkdownInlineAtom> {
    let mut joined = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            // Preserve a soft line boundary for inline parsing rules (for example
            // preventing cross-line link parsing) without forcing a rendered space.
            joined.push(MarkdownInlineAtom::Char('\n'));
        }
        joined.extend(line.iter().copied());
    }

    joined
}

fn trim_atoms(atoms: &[MarkdownInlineAtom]) -> Vec<MarkdownInlineAtom> {
    let mut start = 0usize;
    let mut end = atoms.len();

    while start < end && matches!(atoms[start], MarkdownInlineAtom::Char(ch) if ch.is_whitespace())
    {
        start += 1;
    }

    while end > start
        && matches!(atoms[end - 1], MarkdownInlineAtom::Char(ch) if ch.is_whitespace())
    {
        end -= 1;
    }

    atoms[start..end].to_vec()
}

fn trim_leading_horizontal_whitespace(atoms: &[MarkdownInlineAtom]) -> Vec<MarkdownInlineAtom> {
    atoms[skip_leading_horizontal_whitespace(atoms)..].to_vec()
}

fn skip_leading_horizontal_whitespace(atoms: &[MarkdownInlineAtom]) -> usize {
    let mut index = 0usize;
    while let Some(MarkdownInlineAtom::Char(' ' | '\t')) = atoms.get(index) {
        index += 1;
    }
    index
}

fn count_consecutive_star_chars(atoms: &[MarkdownInlineAtom], start_index: usize) -> usize {
    let mut count = 0usize;
    let mut index = start_index;

    while let Some('*') = atom_char(atoms, index) {
        count += 1;
        index += 1;
    }

    count
}

fn atom_char(atoms: &[MarkdownInlineAtom], index: usize) -> Option<char> {
    match atoms.get(index)? {
        MarkdownInlineAtom::Char(ch) => Some(*ch),
        MarkdownInlineAtom::Opaque(_) => None,
    }
}

fn try_parse_link_at_atoms(
    atoms: &[MarkdownInlineAtom],
    at_index: usize,
) -> Option<ParsedMarkdownLink> {
    if atom_char(atoms, at_index)? != '@' {
        return None;
    }

    let target_start = at_index + 1;
    let mut cursor = target_start;
    if !consume_target_start_atoms(atoms, &mut cursor) {
        return None;
    }

    while let Some(atom) = atoms.get(cursor) {
        match atom {
            MarkdownInlineAtom::Char(ch) if !ch.is_whitespace() => cursor += 1,
            MarkdownInlineAtom::Char(_) => break,
            MarkdownInlineAtom::Opaque(_) => return None,
        }
    }
    let target_end = cursor;
    if target_end == target_start {
        return None;
    }

    let spacing_start = cursor;
    while let Some(atom) = atoms.get(cursor) {
        match atom {
            MarkdownInlineAtom::Char(ch) if is_horizontal_whitespace(*ch) => cursor += 1,
            MarkdownInlineAtom::Char(_) => break,
            MarkdownInlineAtom::Opaque(_) => return None,
        }
    }
    if spacing_start == cursor {
        return None;
    }

    if atom_char(atoms, cursor)? != '(' {
        return None;
    }
    cursor += 1;

    let label_start = cursor;
    while let Some(atom) = atoms.get(cursor) {
        match atom {
            MarkdownInlineAtom::Char(')') => break,
            MarkdownInlineAtom::Char(_) => cursor += 1,
            MarkdownInlineAtom::Opaque(_) => return None,
        }
    }

    if atom_char(atoms, cursor)? != ')' {
        return None;
    }

    let label = collect_plain_chars(&atoms[label_start..cursor]);
    if label.chars().all(char::is_whitespace) {
        return None;
    }

    let target = collect_plain_chars(&atoms[target_start..target_end]);

    Some(ParsedMarkdownLink {
        target,
        label,
        consumed_atoms: cursor + 1 - at_index,
    })
}

fn collect_plain_chars(atoms: &[MarkdownInlineAtom]) -> String {
    atoms
        .iter()
        .map(|atom| match atom {
            MarkdownInlineAtom::Char(ch) => *ch,
            MarkdownInlineAtom::Opaque(_) => {
                unreachable!("link parsing should only collect plain character atoms")
            }
        })
        .collect()
}

fn consume_target_start_atoms(atoms: &[MarkdownInlineAtom], cursor: &mut usize) -> bool {
    if *cursor >= atoms.len() {
        return false;
    }

    if starts_with_chars(atoms, *cursor, &['/', '/']) {
        *cursor += 2;
        return true;
    }
    if starts_with_chars(atoms, *cursor, &['.', '/']) {
        *cursor += 2;
        return true;
    }
    if starts_with_chars(atoms, *cursor, &['.', '.', '/']) {
        *cursor += 3;
        return true;
    }

    match atom_char(atoms, *cursor) {
        Some('/' | '#' | '?') => {
            *cursor += 1;
            true
        }
        Some(ch) if ch.is_ascii_alphabetic() => consume_scheme_prefix_atoms(atoms, cursor),
        _ => false,
    }
}

fn consume_scheme_prefix_atoms(atoms: &[MarkdownInlineAtom], cursor: &mut usize) -> bool {
    if !matches!(atom_char(atoms, *cursor), Some(ch) if ch.is_ascii_alphabetic()) {
        return false;
    }

    *cursor += 1;
    while let Some(ch) = atom_char(atoms, *cursor) {
        if !is_scheme_char(ch) {
            break;
        }
        *cursor += 1;
    }

    if atom_char(atoms, *cursor) != Some(':') {
        return false;
    }

    *cursor += 1;
    true
}

fn starts_with_chars(atoms: &[MarkdownInlineAtom], start: usize, prefix: &[char]) -> bool {
    prefix
        .iter()
        .enumerate()
        .all(|(offset, expected)| atom_char(atoms, start + offset) == Some(*expected))
}

fn is_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-')
}

fn is_horizontal_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

fn em_tag_strength(strength: i32, closing: bool) -> &'static str {
    if closing {
        match strength {
            2 => "</strong>",
            3 => "</strong></em>",
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
