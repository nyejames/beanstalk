//! Per-file import path extraction for Beanstalk source files.
//!
//! Tokenizes a single source file and returns the import paths declared in it.

use super::source_loading::extract_source_code;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::const_paths::collect_paths_from_tokens;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use std::path::Path;

pub(super) fn extract_import_paths(
    file_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    newline_mode: NewlineMode,
    string_table: &mut StringTable,
) -> Result<Vec<InternedPath>, CompilerError> {
    let source = extract_source_code(file_path, string_table)?;
    let interned_path = InternedPath::from_path_buf(file_path, string_table);
    let tokens = tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        newline_mode,
        style_directives,
        string_table,
        None,
    )?;

    let imports = collect_paths_from_tokens(&tokens.tokens)?;

    Ok(imports)
}
