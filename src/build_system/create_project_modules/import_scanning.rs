//! Per-file import path extraction for Beanstalk source files.
//!
//! Tokenizes a single source file and returns the import paths declared in it.
// Import scanning preserves the same `SourceDiscoveryError` boundary as reachable-file discovery,
// so syntax diagnostics and file/tooling failures stay typed until the Stage 0 boundary.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::paths::const_paths::collect_paths_from_tokens;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::interned_path::{InternedPath, NonUtf8PathComponent};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizerEntryMode;

use std::path::Path;

use super::source_discovery_error::SourceDiscoveryError;
use super::source_loading::extract_source_code;

/// Import scan output that keeps the already-read source available to Stage 0.
///
/// WHAT: pairs import paths with the Beanstalk source text used to discover them.
/// WHY: reachable-file discovery can reuse the source when assembling `InputFile`
///      values instead of reading each scanned `.bst` file again.
pub(super) struct ScannedImportSource {
    pub(super) import_paths: Vec<InternedPath>,
    pub(super) source_code: String,
}

// -------------------------
//  Import Path Extraction
// -------------------------

pub(crate) fn extract_import_paths(
    file_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<InternedPath>, SourceDiscoveryError> {
    Ok(scan_imports_with_source(file_path, style_directives, string_table)?.import_paths)
}

pub(super) fn scan_imports_with_source(
    file_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<ScannedImportSource, SourceDiscoveryError> {
    let source =
        extract_source_code(file_path, string_table).map_err(SourceDiscoveryError::from)?;

    scan_imports_from_source(file_path, source, style_directives, string_table)
}

pub(super) fn scan_imports_from_source(
    file_path: &Path,
    source: String,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<ScannedImportSource, SourceDiscoveryError> {
    let interned_path = match InternedPath::try_from_filesystem_path(file_path, string_table) {
        Ok(path) => path,
        Err(NonUtf8PathComponent { path }) => {
            return Err(SourceDiscoveryError::from(CompilerError::file_error(
                &path,
                format!(
                    "Source file path {path:?} contains a non-UTF-8 component; Beanstalk identity requires UTF-8 paths."
                ),
                string_table,
            )));
        }
    };

    // Tokenize the file to find path declarations. Callers may supply source text that was read
    // during an earlier Stage 0 classification pass so provider-free discovery does not re-read
    // the same Beanstalk file before assembling `InputFile` values.
    let tokens = tokenize(
        &source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        style_directives,
        string_table,
        None,
    )
    .map_err(SourceDiscoveryError::Diagnostic)?;

    let imports =
        collect_paths_from_tokens(&tokens.tokens).map_err(SourceDiscoveryError::Diagnostic)?;

    Ok(ScannedImportSource {
        import_paths: imports,
        source_code: source,
    })
}
