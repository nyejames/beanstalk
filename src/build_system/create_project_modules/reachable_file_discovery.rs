//! BFS traversal over Beanstalk import graphs to find all reachable source files.
//!
//! Given an entry `.bst` file, walks its import declarations transitively to build the complete
//! set of source files that belong to a module. Also assembles `InputFile` payloads from those
//! paths for downstream compilation stages.

use super::import_scanning::extract_import_paths;
use super::source_loading::extract_source_code;
use crate::build_system::build::InputFile;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Collect all reachable source files for a given entry point and load their content.
pub(super) fn collect_reachable_input_files(
    entry_path: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<InputFile>, CompilerMessages> {
    let reachable_files = match discover_reachable_files(
        entry_path,
        project_path_resolver,
        style_directives,
        external_packages,
        string_table,
    ) {
        Ok(files) => files,
        Err(error) => return Err(CompilerMessages::from_error_ref(error, string_table)),
    };

    let mut input_files = Vec::with_capacity(reachable_files.len());
    for source_path in reachable_files {
        let source_code = match extract_source_code(&source_path, string_table) {
            Ok(code) => code,
            Err(error) => return Err(CompilerMessages::from_error_ref(error, string_table)),
        };
        input_files.push(InputFile {
            source_code,
            source_path,
        });
    }
    Ok(input_files)
}

/// BFS over import declarations starting from `entry_point`.
///
/// WHAT: follows each file's declared imports, resolves them to canonical paths, and returns the
/// full ordered set of files reachable from the entry point.
/// WHY: the set is built with a `BTreeSet` so the output order is deterministic, and each path is
/// only visited once to handle import cycles safely.
pub(super) fn discover_reachable_files(
    entry_point: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, CompilerError> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(entry_point.to_path_buf());

    // Seed all source-library facade files so re-exports are resolvable.
    // WHY: imports may directly resolve to a target file (bypassing the facade fallback),
    // but the facade still needs to be compiled so its re-export map can be built.
    for facade_path in project_path_resolver.facade_files().values() {
        queue.push_back(facade_path.clone());
    }

    while let Some(next_file) = queue.pop_front() {
        let canonical_file = fs::canonicalize(&next_file).map_err(|error| {
            CompilerError::file_error(
                &next_file,
                format!("Failed to canonicalize module file path: {error}"),
                string_table,
            )
        })?;

        if !reachable.insert(canonical_file.clone()) {
            continue;
        }

        let import_paths = extract_import_paths(
            &canonical_file,
            style_directives,
            NewlineMode::NormalizeToLf,
            string_table,
        )?;
        for import_path in &import_paths {
            // Skip virtual package imports — AST resolution handles those.
            if external_packages.is_virtual_package_import(import_path, string_table) {
                continue;
            }
            if let Some(package_path) =
                external_packages.unsupported_known_package_import(import_path, string_table)
            {
                return Err(unsupported_builder_package_error(
                    &canonical_file,
                    package_path,
                    string_table,
                ));
            }
            let resolved = project_path_resolver.resolve_import_to_file_with_facade_fallback(
                import_path,
                &canonical_file,
                string_table,
            )?;
            if !reachable.contains(&resolved) {
                queue.push_back(resolved);
            }
        }
    }

    Ok(reachable.into_iter().collect())
}

fn unsupported_builder_package_error(
    importer: &Path,
    package_path: &str,
    string_table: &mut StringTable,
) -> CompilerError {
    let mut error = CompilerError::file_error(
        importer,
        format!("Core package '{package_path}' is not supported by this builder."),
        string_table,
    )
    .with_error_type(ErrorType::Rule);
    error.metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        "Project Structure".to_owned(),
    );
    error.metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        "Use a builder that exposes this core package or remove the import.".to_owned(),
    );
    error
}
