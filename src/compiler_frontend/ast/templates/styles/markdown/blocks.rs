//! Markdown block rendering: paragraphs, lists, and headings.
//!
//! WHAT: renders block-level markdown constructs from line streams.
//! WHY: separating block logic from inline and parsing keeps each module focused
//!      on one level of the markdown formatter.

use super::output::MarkdownOutputBuilder;
use super::{
    LeadingChildTemplateLine, MarkdownInlineAtom, MarkdownLine, MarkdownListItemBlock,
    MarkdownListItemFragment, MarkdownListKind, ParsedMarkdownHeadingLine,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterOpaqueKind, FormatterOutputPiece,
};

/// Groups plain non-list lines into paragraphs, applying child-template line boundaries.
///
/// WHAT:
/// - Consecutive text/dynamic lines stay in the current paragraph and join with spaces.
/// - A child-template anchor alone at line start renders as standalone output.
/// - A child-template anchor followed by same-line content starts a fresh paragraph.
///
/// WHY:
/// - `$markdown` needs to keep child templates opaque while still letting a single
///   newline before a child break paragraph context without splitting inline helpers
///   from their same-line text.
pub(super) fn render_plain_region(
    lines: &[MarkdownLine],
    default_tag: &str,
) -> Vec<FormatterOutputPiece> {
    enum PlainRegionBlock {
        Paragraph(Vec<Vec<MarkdownInlineAtom>>),
        StandaloneInline(Vec<MarkdownInlineAtom>),
    }

    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<Vec<MarkdownInlineAtom>> = Vec::new();

    for line in lines {
        match classify_leading_child_template_line(&line.atoms) {
            LeadingChildTemplateLine::Standalone => {
                if !paragraph_lines.is_empty() {
                    blocks.push(PlainRegionBlock::Paragraph(std::mem::take(
                        &mut paragraph_lines,
                    )));
                }

                blocks.push(PlainRegionBlock::StandaloneInline(
                    super::parsing::trim_leading_horizontal_whitespace(&line.atoms),
                ));
            }
            LeadingChildTemplateLine::InlineContinuation => {
                if !paragraph_lines.is_empty() {
                    blocks.push(PlainRegionBlock::Paragraph(std::mem::take(
                        &mut paragraph_lines,
                    )));
                }

                blocks.push(PlainRegionBlock::Paragraph(vec![
                    super::parsing::trim_leading_horizontal_whitespace(&line.atoms),
                ]));
            }
            LeadingChildTemplateLine::None => {
                paragraph_lines.push(line.atoms.clone());
            }
        }
    }

    if !paragraph_lines.is_empty() {
        blocks.push(PlainRegionBlock::Paragraph(paragraph_lines));
    }

    let mut output = MarkdownOutputBuilder::default();
    for block in blocks {
        match block {
            PlainRegionBlock::Paragraph(lines) => {
                let atoms = super::parsing::join_lines_with_spaces(&lines);
                output.append_pieces(super::inline::render_inline_atoms(
                    &atoms,
                    Some(default_tag),
                    true,
                ));
            }
            PlainRegionBlock::StandaloneInline(atoms) => {
                output.append_pieces(super::inline::render_inline_atoms(
                    &atoms,
                    Some(default_tag),
                    false,
                ));
            }
        }
    }

    output.finish()
}

/// Parses a list block recursively so nested list items keep mixed content in one `<li>`.
pub(super) fn render_list_block(
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
    let Some(first_item) = super::parsing::parse_list_item_line(first_line) else {
        return (Vec::new(), 0);
    };
    let current_indent = first_item.indent_width;

    let mut sections: Vec<MarkdownRenderedListSection> = Vec::new();
    let mut consumed_lines = 0usize;

    while consumed_lines < lines.len() {
        let line = &lines[consumed_lines];
        if super::line_is_blank(line) || super::parsing::parse_heading_line(line).is_some() {
            break;
        }

        let Some(list_item) = super::parsing::parse_list_item_line(line) else {
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
            if super::line_is_blank(next_line)
                || super::parsing::parse_heading_line(next_line).is_some()
            {
                break;
            }

            if let Some(next_item) = super::parsing::parse_list_item_line(next_line) {
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

            fragments.push(MarkdownListItemFragment::Line(super::parsing::trim_atoms(
                &next_line.atoms,
            )));
            consumed_lines += 1;
        }

        let rendered_item = render_list_item_fragments(&fragments, default_tag);
        if let Some(section) = sections.last_mut() {
            section.items.push(rendered_item);
        } else {
            sections.push(MarkdownRenderedListSection {
                kind: list_item.kind,
                items: vec![rendered_item],
            });
        }
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
                let atoms = super::parsing::join_lines_with_spaces(&lines);
                if should_wrap_paragraphs {
                    output.append_pieces(super::inline::render_inline_atoms(
                        &atoms,
                        Some(default_tag),
                        true,
                    ));
                } else {
                    output.append_pieces(super::inline::render_inline_atoms(&atoms, None, false));
                }
            }
            MarkdownListItemBlock::StandaloneInline(atoms) => {
                output.append_pieces(super::inline::render_inline_atoms(
                    &atoms,
                    Some(default_tag),
                    false,
                ));
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
            MarkdownListItemFragment::Line(atoms) => {
                match classify_leading_child_template_line(atoms) {
                    LeadingChildTemplateLine::Standalone => {
                        if !paragraph_lines.is_empty() {
                            blocks.push(MarkdownListItemBlock::Paragraph(std::mem::take(
                                &mut paragraph_lines,
                            )));
                        }

                        blocks.push(MarkdownListItemBlock::StandaloneInline(
                            super::parsing::trim_leading_horizontal_whitespace(atoms),
                        ));
                    }
                    LeadingChildTemplateLine::InlineContinuation => {
                        if !paragraph_lines.is_empty() {
                            blocks.push(MarkdownListItemBlock::Paragraph(std::mem::take(
                                &mut paragraph_lines,
                            )));
                        }

                        blocks.push(MarkdownListItemBlock::Paragraph(vec![
                            super::parsing::trim_leading_horizontal_whitespace(atoms),
                        ]));
                    }
                    LeadingChildTemplateLine::None => {
                        paragraph_lines.push(atoms.clone());
                    }
                }
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

pub(super) fn render_heading_line(
    heading: &ParsedMarkdownHeadingLine,
) -> Vec<FormatterOutputPiece> {
    let heading_tag = format!("h{}", heading.level);
    super::inline::render_inline_atoms(&heading.content, Some(heading_tag.as_str()), true)
}

fn classify_leading_child_template_line(atoms: &[MarkdownInlineAtom]) -> LeadingChildTemplateLine {
    let mut index = super::parsing::skip_leading_horizontal_whitespace(atoms);

    let Some(MarkdownInlineAtom::Opaque(anchor)) = atoms.get(index) else {
        return LeadingChildTemplateLine::None;
    };

    if anchor.kind != FormatterOpaqueKind::ChildTemplate {
        return LeadingChildTemplateLine::None;
    }

    index += 1;

    while let Some(MarkdownInlineAtom::Char(' ' | '\t')) = atoms.get(index) {
        index += 1;
    }

    if atoms[index..].iter().any(|atom| match atom {
        MarkdownInlineAtom::Char(ch) => !matches!(ch, ' ' | '\t' | '\r' | '\n'),
        MarkdownInlineAtom::Opaque(_) => true,
    }) {
        LeadingChildTemplateLine::InlineContinuation
    } else {
        LeadingChildTemplateLine::Standalone
    }
}
