//! Token-line structural scanning helpers.
//!
//! WHAT: exposes small utilities for finding top-level separators on the current
//! logical source line after tokenization.
//! WHY: header splitting and AST statement parsing both need token-boundary facts,
//! but neither stage should duplicate delimiter-depth scans or depend on the other.

use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::utilities::token_scan::NestingDepth;

fn find_top_level_token_on_line(
    token_stream: &FileTokens,
    start_index: usize,
    matches_target: impl Fn(&TokenKind) -> bool,
) -> Option<usize> {
    let mut nesting_depth = NestingDepth::default();

    for index in start_index..token_stream.length {
        let kind = &token_stream.tokens[index].kind;
        match kind {
            TokenKind::Newline | TokenKind::End | TokenKind::Eof => break,
            _ if nesting_depth.is_top_level() && matches_target(kind) => return Some(index),
            _ => nesting_depth.step(kind),
        }
    }

    None
}

pub(crate) fn find_top_level_fat_arrow_on_line(
    token_stream: &FileTokens,
    start_index: usize,
) -> Option<usize> {
    find_top_level_token_on_line(token_stream, start_index, |kind| {
        matches!(kind, TokenKind::FatArrow)
    })
}

pub(crate) fn find_top_level_colon_on_line(
    token_stream: &FileTokens,
    start_index: usize,
) -> Option<usize> {
    find_top_level_token_on_line(token_stream, start_index, |kind| {
        matches!(kind, TokenKind::Colon)
    })
}
