//! Markdown parsing helpers: list items, headings, links, and atom utilities.
//!
//! WHAT: parses markdown line structures and inline link syntax from atom streams.
//! WHY: separating parsing from rendering keeps the grammar rules in one place
//!      and makes the renderer's control flow easier to follow.

use super::{
    MarkdownInlineAtom, MarkdownLine, MarkdownListKind, ParsedMarkdownHeadingLine,
    ParsedMarkdownLink, ParsedMarkdownListItemLine,
};

pub(super) fn parse_heading_line(line: &MarkdownLine) -> Option<ParsedMarkdownHeadingLine> {
    let start = skip_leading_horizontal_whitespace(&line.atoms);
    let mut index = start;
    let mut level = 0usize;

    while let Some('#') = super::types::atom_char(&line.atoms, index) {
        level += 1;
        index += 1;
    }

    if level == 0 {
        return None;
    }

    let separator = super::types::atom_char(&line.atoms, index)?;
    if !separator.is_whitespace() {
        return None;
    }

    Some(ParsedMarkdownHeadingLine {
        level,
        content: line.atoms[index + 1..].to_vec(),
    })
}

pub(super) fn parse_list_item_line(line: &MarkdownLine) -> Option<ParsedMarkdownListItemLine> {
    if super::line_is_blank(line) {
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
    let marker = super::types::atom_char(atoms, start_index)?;
    if !matches!(marker, '-' | '*' | '+') {
        return None;
    }

    let separator = super::types::atom_char(atoms, start_index + 1)?;
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

    while let Some(ch) = super::types::atom_char(atoms, index) {
        if !ch.is_ascii_digit() {
            break;
        }
        index += 1;
    }

    if index == start_index {
        return None;
    }

    let marker = super::types::atom_char(atoms, index)?;
    if !matches!(marker, '.' | ')') {
        return None;
    }

    let separator = super::types::atom_char(atoms, index + 1)?;
    if !separator.is_whitespace() {
        return None;
    }

    Some(ParsedMarkdownListItemLine {
        indent_width,
        kind: MarkdownListKind::Ordered,
        content: trim_atoms(&atoms[index + 2..]),
    })
}

pub(super) fn join_lines_with_spaces(lines: &[Vec<MarkdownInlineAtom>]) -> Vec<MarkdownInlineAtom> {
    let mut joined = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            // Preserve a soft line boundary for inline parsing rules (for example
            // preventing cross-line link parsing). Inline rendering turns this into
            // exactly one visible space.
            joined.push(MarkdownInlineAtom::Char('\n'));
        }
        joined.extend(line.iter().copied());
    }

    joined
}

pub(super) fn trim_atoms(atoms: &[MarkdownInlineAtom]) -> Vec<MarkdownInlineAtom> {
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

pub(super) fn trim_leading_horizontal_whitespace(
    atoms: &[MarkdownInlineAtom],
) -> Vec<MarkdownInlineAtom> {
    atoms[skip_leading_horizontal_whitespace(atoms)..].to_vec()
}

pub(super) fn skip_leading_horizontal_whitespace(atoms: &[MarkdownInlineAtom]) -> usize {
    let mut index = 0usize;
    while let Some(MarkdownInlineAtom::Char(' ' | '\t')) = atoms.get(index) {
        index += 1;
    }
    index
}

pub(super) fn try_parse_link_at_atoms(
    atoms: &[MarkdownInlineAtom],
    at_index: usize,
) -> Option<ParsedMarkdownLink> {
    if super::types::atom_char(atoms, at_index)? != '@' {
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

    if super::types::atom_char(atoms, cursor)? != '(' {
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

    if super::types::atom_char(atoms, cursor)? != ')' {
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
        .filter_map(|atom| match atom {
            MarkdownInlineAtom::Char(ch) => Some(*ch),
            MarkdownInlineAtom::Opaque(_) => None,
        })
        .collect()
}

/// Consumes the scheme or path prefix at the start of a markdown link target.
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

    match super::types::atom_char(atoms, *cursor) {
        Some('/' | '#' | '?') => {
            *cursor += 1;
            true
        }
        Some(ch) if ch.is_ascii_alphabetic() => consume_scheme_prefix_atoms(atoms, cursor),
        _ => false,
    }
}

fn consume_scheme_prefix_atoms(atoms: &[MarkdownInlineAtom], cursor: &mut usize) -> bool {
    if !matches!(super::types::atom_char(atoms, *cursor), Some(ch) if ch.is_ascii_alphabetic()) {
        return false;
    }

    *cursor += 1;
    while let Some(ch) = super::types::atom_char(atoms, *cursor) {
        if !is_scheme_char(ch) {
            break;
        }
        *cursor += 1;
    }

    if super::types::atom_char(atoms, *cursor) != Some(':') {
        return false;
    }

    *cursor += 1;
    true
}

fn starts_with_chars(atoms: &[MarkdownInlineAtom], start: usize, prefix: &[char]) -> bool {
    prefix
        .iter()
        .enumerate()
        .all(|(offset, expected)| super::types::atom_char(atoms, start + offset) == Some(*expected))
}

fn is_scheme_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-')
}

fn is_horizontal_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}
