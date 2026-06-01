#![allow(clippy::result_large_err)]

//! Per-file import clause recording for header parsing.
//!
//! WHAT: parses top-level import clauses into normalized file-local import records.
//! WHY: import shells and their local names must be known before declaration headers are built,
//! but full visibility and facade validation remain later header-stage responsibilities.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::file_state::HeaderFileParseState;
use crate::compiler_frontend::headers::imports::normalize_import_dependency_path;
use crate::compiler_frontend::headers::types::{FileImport, HeaderParseContext};
use crate::compiler_frontend::paths::const_paths::parse_import_clause_items;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};

pub(super) fn parse_and_record_imports(
    token_stream: &mut FileTokens,
    state: &mut HeaderFileParseState,
    context: &mut HeaderParseContext<'_>,
    import_location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    let import_index = token_stream.index.saturating_sub(1);

    let (items, next_index) =
        parse_import_clause_items(&token_stream.tokens, import_index, context.string_table)?;

    for item in items {
        let normalized_path = normalize_import_dependency_path(
            &item.path,
            &token_stream.src_path,
            &item.path_location,
            context.string_table,
        )?;

        let local_name = item.alias.or_else(|| normalized_path.name());
        if let Some(name) = local_name {
            state
                .encountered_symbols
                .insert(name, import_location.clone());
        }

        if state
            .seen_imports
            .insert((normalized_path.to_owned(), item.alias))
        {
            state.file_import_paths.insert(normalized_path.to_owned());
            state.file_imports.push(FileImport {
                header_path: normalized_path,
                alias: item.alias,
                location: import_location.clone(),
                path_location: item.path_location,
                alias_location: item.alias_location,
                from_grouped: item.from_grouped,
            });
        }
    }

    token_stream.index = next_index;

    Ok(())
}
