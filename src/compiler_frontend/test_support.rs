//! Shared frontend test utilities.
//!
//! WHAT: provides low-churn helpers reused across frontend subsystem tests.
//! WHY: path-resolution setup and source location construction are identical in several suites
//!      and should stay consistent.

use crate::compiler_frontend::ast::ast_nodes::SourceLocation;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::tokenizer::tokens::CharPosition;

pub(crate) fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(cwd.clone(), cwd, &[]).expect("test path resolver should be valid")
}

/// Creates a single-line `SourceLocation` at the given line number for use in test fixtures.
///
/// WHAT: produces a deterministic source location with an arbitrary column span.
/// WHY: many test suites construct locations for the same reason; one canonical helper prevents
///      each suite from defining its own with slightly different shapes.
pub(crate) fn test_source_location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120,
        },
    }
}
