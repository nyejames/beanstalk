//! Markdown inline code span parsing.
//!
//! WHAT: detects and extracts valid same-line single-backtick inline code spans
//! from a slice of markdown inline atoms.
//! WHY: keeps the inline atom renderer focused on rendering while this small,
//! stage-local pass handles the narrow inline-code recognition rules.

use super::{MarkdownInlineAtom, atom_char};
use crate::compiler_frontend::ast::templates::formatter_contract::FormatterOpaqueKind;

/// A successfully parsed inline code span with its content atoms and total
/// consumed atom count (including both delimiters).
#[derive(Debug, Clone)]
pub(super) struct ParsedInlineCodeSpan {
    pub(super) content: Vec<MarkdownInlineAtom>,
    pub(super) consumed_atoms: usize,
}

/// Attempts to parse a valid inline code span starting at `start_index`.
///
/// WHAT:
/// - Accepts only an isolated single backtick as the opening delimiter.
///   Isolation means the backtick is not immediately preceded or followed by
///   another backtick atom, so consecutive backtick runs are treated as literal
///   text.
/// - Scans forward only until newline, carriage return, or end of slice.
/// - Accepts only an isolated single backtick as the closing delimiter.
///   Isolation for the closing delimiter means it is not immediately preceded
///   or followed by another backtick.
/// - Rejects spans that contain a `ChildTemplate` opaque anchor.
/// - Rejects empty spans (no content atoms between delimiters).
///
/// WHY:
/// - Inline code must not cross line boundaries and must keep child templates
///   sealed from parent-formatter inspection.
pub(super) fn try_parse_inline_code_span_at_atoms(
    atoms: &[MarkdownInlineAtom],
    start_index: usize,
) -> Option<ParsedInlineCodeSpan> {
    // Opening delimiter must be a single, isolated backtick.
    if atom_char(atoms, start_index) != Some('`') {
        return None;
    }
    if start_index > 0 && atom_char(atoms, start_index - 1) == Some('`') {
        return None;
    }
    if atom_char(atoms, start_index + 1) == Some('`') {
        return None;
    }

    let mut cursor = start_index + 1;
    let mut content = Vec::new();

    while let Some(atom) = atoms.get(cursor) {
        match atom {
            MarkdownInlineAtom::Char('\n' | '\r') => break,
            MarkdownInlineAtom::Char('`') => {
                // Closing delimiters must be isolated single backticks. Backtick
                // runs inside the candidate span remain literal content unless a
                // later isolated delimiter closes the span.
                let previous_is_backtick =
                    cursor > start_index + 1 && atom_char(atoms, cursor - 1) == Some('`');
                let next_is_backtick = atom_char(atoms, cursor + 1) == Some('`');

                if previous_is_backtick || next_is_backtick {
                    content.push(*atom);
                    cursor += 1;
                    continue;
                }

                if content.is_empty() {
                    return None;
                }

                return Some(ParsedInlineCodeSpan {
                    content,
                    consumed_atoms: cursor + 1 - start_index,
                });
            }
            MarkdownInlineAtom::Opaque(anchor) => {
                if anchor.kind == FormatterOpaqueKind::ChildTemplate {
                    return None;
                }
                content.push(*atom);
                cursor += 1;
            }
            other => {
                content.push(*other);
                cursor += 1;
            }
        }
    }

    None
}
