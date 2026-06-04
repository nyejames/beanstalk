//! Per-file import path extraction for Beanstalk source files.
//!
//! Tokenizes a single source file and returns the import paths declared in it.
#![allow(clippy::result_large_err)]
// Import scanning preserves the same `SourceDiscoveryError` boundary as reachable-file discovery,
// so syntax diagnostics and file/tooling failures stay typed until the Stage 0 boundary.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::const_paths::collect_paths_from_tokens;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizerEntryMode;

use std::path::Path;

use super::source_discovery_error::SourceDiscoveryError;
use super::source_loading::extract_source_code;

// -------------------------
//  Import Path Extraction
// -------------------------

pub(crate) fn extract_import_paths(
    file_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<InternedPath>, SourceDiscoveryError> {
    // 1. Load raw source code.
    let source =
        extract_source_code(file_path, string_table).map_err(SourceDiscoveryError::from)?;
    let interned_path = InternedPath::from_path_buf(file_path, string_table);

    // 2. Tokenize the file to find path declarations.
    let tokens = tokenize(
        &source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        style_directives,
        string_table,
        None,
    )?;

    // 3. Extract paths from the token stream.
    let imports = collect_paths_from_tokens(&tokens.tokens)?;

    Ok(imports)
}
