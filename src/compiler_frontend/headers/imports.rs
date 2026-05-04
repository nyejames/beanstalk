//! Header-stage import collection.
//!
//! WHAT: parses top-level import clauses into normalized header dependency paths.
//! WHY: imports affect file-local visibility and header-provided dependency edges, so their normalization
//! belongs to the header stage rather than AST body parsing.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

pub(super) fn normalize_import_dependency_path(
    import_path: &InternedPath,
    source_file: &InternedPath,
    path_location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<InternedPath, CompilerError> {
    if import_path
        .as_components()
        .iter()
        .any(|component| string_table.resolve(*component) == "..")
    {
        let mut error = CompilerError::new_rule_error(
            format!(
                "Import paths containing '..' are not supported: '{}'",
                import_path.to_portable_string(string_table),
            ),
            path_location.clone(),
        );
        error.metadata.insert(
            ErrorMetaDataKey::CompilationStage,
            "Project Structure".to_owned(),
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Use a direct path without parent-directory references.".to_owned(),
        );
        return Err(error);
    }

    let mut import_components = import_path.as_components().iter().copied();
    let Some(first) = import_components.next() else {
        return Ok(import_path.to_owned());
    };

    let first_segment = string_table.resolve(first);
    if first_segment != "." && first_segment != ".." {
        return Ok(import_path.to_owned());
    }

    let mut resolved_components = source_file.as_components().to_vec();
    resolved_components.pop();

    for component in import_path.as_components() {
        match string_table.resolve(*component) {
            "." => {}
            ".." => {
                resolved_components.pop();
            }
            _ => resolved_components.push(*component),
        }
    }

    Ok(InternedPath::from_components(resolved_components))
}
