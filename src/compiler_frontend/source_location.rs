//! Shared source-location model for frontend and diagnostic scopes.
//!
//! WHAT: defines one generic location shape plus the `TextLocation` and `ErrorLocation`
//! specializations used throughout the compiler.
//! WHY: frontend source locations and owned diagnostic locations represent the same source span
//! concept and should differ only in how the scope path is stored.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use std::cmp::Ordering;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct CharPosition {
    pub line_number: i32,
    pub char_column: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct SourceLocation<Scope> {
    pub scope: Scope,
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

pub type TextLocation = SourceLocation<InternedPath>;
pub type ErrorLocation = SourceLocation<PathBuf>;

impl<Scope> SourceLocation<Scope> {
    pub fn new(scope: Scope, start: CharPosition, end: CharPosition) -> Self {
        Self {
            scope,
            start_pos: start,
            end_pos: end,
        }
    }
}

impl SourceLocation<InternedPath> {
    pub fn to_error_location(&self, string_table: &StringTable) -> ErrorLocation {
        ErrorLocation::new(
            self.scope.to_path_buf(string_table),
            self.start_pos,
            self.end_pos,
        )
    }
}

impl<Scope: PartialEq> PartialOrd for SourceLocation<Scope> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_start_line = self.start_pos.line_number;
        let other_start_line = other.start_pos.line_number;

        if self_start_line < other_start_line {
            let self_end_line = self.end_pos.line_number;

            if self_end_line < other_start_line {
                Some(Ordering::Less)
            } else {
                Some(Ordering::Equal)
            }
        } else if self_start_line > other_start_line {
            let other_end_line = other.end_pos.line_number;

            if other_end_line < self_start_line {
                Some(Ordering::Greater)
            } else {
                Some(Ordering::Equal)
            }
        } else {
            let self_start_col = self.start_pos.char_column;
            let other_start_col = other.start_pos.char_column;

            if self_start_col < other_start_col {
                let self_end_line = self.end_pos.line_number;
                let self_end_col = self.end_pos.char_column;

                if self_end_line < other_start_line
                    || (self_end_line == other_start_line && self_end_col < other_start_col)
                {
                    Some(Ordering::Less)
                } else {
                    Some(Ordering::Equal)
                }
            } else if self_start_col > other_start_col {
                let other_end_line = other.end_pos.line_number;
                let other_end_col = other.end_pos.char_column;

                if other_end_line < self_start_line
                    || (other_end_line == self_start_line && other_end_col < self_start_col)
                {
                    Some(Ordering::Greater)
                } else {
                    Some(Ordering::Equal)
                }
            } else {
                Some(Ordering::Equal)
            }
        }
    }
}
