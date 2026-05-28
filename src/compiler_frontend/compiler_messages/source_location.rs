//! Shared source-location model for frontend and diagnostics.
//!
//! WHAT: defines the one canonical source span type used throughout the compiler pipeline.
//! WHY: all diagnostics now preserve interned paths and resolve them through the shared
//!      `StringTable` only at rendering or filesystem-adjacent boundaries.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringIdRemap, StringTable};
use std::cmp::Ordering;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
pub struct CharPosition {
    pub line_number: i32,
    pub char_column: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct SourceLocation {
    pub scope: InternedPath,
    pub start_pos: CharPosition,
    pub end_pos: CharPosition,
}

impl SourceLocation {
    pub fn new(scope: InternedPath, start: CharPosition, end: CharPosition) -> Self {
        Self {
            scope,
            start_pos: start,
            end_pos: end,
        }
    }

    /// Create a file-level location by interning the provided filesystem path.
    ///
    /// WHAT: preserves non-tokenized file diagnostics in the same interned-path model as parsed
    /// source locations.
    /// WHY: terminal/dev-server renderers now resolve all diagnostic paths through the shared
    /// string table instead of storing owned `PathBuf`s on errors or warnings.
    pub fn from_path(path: &Path, string_table: &mut StringTable) -> Self {
        Self::new(
            InternedPath::from_path_buf(path, string_table),
            CharPosition::default(),
            CharPosition::default(),
        )
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.scope.remap_string_ids(remap);
    }
}

impl PartialOrd for SourceLocation {
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
