//! Project-local source-library discovery for Stage 0.
//!
//! WHAT: scans configured library folders, merges project-local libraries with builder-provided
//! libraries, and rejects ambiguous import-prefix ownership.
//! WHY: source-library discovery is project input preparation. It should not live inside module
//! inventory or frontend semantic import handling.

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::source_libraries::root_file::{
    HashRootFileDiscovery, PreparedSourceLibraryRoots, discover_hash_root_file,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::{ProvidedSourceRoot, SourceLibraryRegistry};
use crate::projects::settings::Config;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::project_structure_diagnostics::{
    config_diagnostic_messages, path_id, project_structure_messages,
};

/// Prepare canonical source-library roots and their direct-child public-surface states.
///
/// WHAT: performs the one source-library filesystem preflight used by directory, single-file,
///     and config compilation.
/// WHY: Stage 0 owns filesystem preparation; path resolution consumes the resulting immutable
///     contract and must not scan or canonicalize source-library roots during construction.
pub(crate) fn prepare_source_library_roots(
    source_libraries: &SourceLibraryRegistry,
) -> PreparedSourceLibraryRoots {
    let entries = source_libraries.iter().map(|library| {
        let ProvidedSourceRoot::Filesystem(path) = &library.root;
        let canonical_root = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        let discovery = match discover_hash_root_file(&canonical_root) {
            Ok(HashRootFileDiscovery::Unique(root_file)) => match fs::canonicalize(&root_file) {
                Ok(canonical_root_file) => HashRootFileDiscovery::Unique(canonical_root_file),
                Err(error) => HashRootFileDiscovery::Unreadable(format!(
                    "Failed to canonicalize source library public-surface file '{}': {error}",
                    root_file.display()
                )),
            },
            Ok(discovery) => discovery,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                HashRootFileDiscovery::Missing
            }
            Err(error) => HashRootFileDiscovery::Unreadable(error.to_string()),
        };

        (library.import_prefix.clone(), canonical_root, discovery)
    });

    PreparedSourceLibraryRoots::from_entries(entries)
}

/// Discover project-local source libraries from configured `library_folders`.
///
/// WHAT: scans each configured top-level folder under the project root and registers one source
/// library root per direct child directory.
/// WHY: project-local library discovery must follow config rather than hardcoding `/lib`.
pub(super) fn discover_project_local_source_libraries(
    config: &Config,
    project_root: &Path,
    string_table: &mut StringTable,
) -> Result<SourceLibraryRegistry, CompilerMessages> {
    let mut discovered_libraries = SourceLibraryRegistry::new();
    let mut discovered_prefixes: HashMap<String, PathBuf> = HashMap::new();

    for configured_folder in &config.library_folders {
        let folder_path = project_root.join(configured_folder);

        // Validate configured library roots before scanning children so config mistakes stay as
        // typed diagnostics instead of becoming later import-resolution failures.
        if !folder_path.exists() {
            if config.has_explicit_library_folders {
                return Err(config_diagnostic_messages(
                    config,
                    "library_folders",
                    InvalidConfigReason::ConfiguredLibraryFolderMissing {
                        folder: path_id(configured_folder, string_table),
                    },
                    string_table,
                ));
            }

            continue;
        }

        if !folder_path.is_dir() {
            return Err(config_diagnostic_messages(
                config,
                "library_folders",
                InvalidConfigReason::ConfiguredLibraryFolderNotDirectory {
                    folder: path_id(configured_folder, string_table),
                },
                string_table,
            ));
        }

        scan_project_library_folder(
            config,
            &folder_path,
            &mut discovered_libraries,
            &mut discovered_prefixes,
            string_table,
        )?;
    }

    Ok(discovered_libraries)
}

/// Merge builder-provided and project-local libraries, preserving the builder/project collision
/// diagnostic that config validation expects.
pub(super) fn merge_source_libraries(
    config: &Config,
    builder_source_libraries: &SourceLibraryRegistry,
    project_local_libraries: &SourceLibraryRegistry,
    string_table: &mut StringTable,
) -> Result<SourceLibraryRegistry, CompilerMessages> {
    let mut merged_libraries = builder_source_libraries.clone();

    if let Err(collisions) = merged_libraries.merge(project_local_libraries) {
        let collision_list = collisions.join(", ");

        return Err(config_diagnostic_messages(
            config,
            "library_folders",
            InvalidConfigReason::SourceLibraryBuilderPrefixCollision {
                prefixes: string_table.get_or_intern(collision_list),
                library_folders: string_table
                    .get_or_intern(format_library_folder_list(&config.library_folders)),
            },
            string_table,
        ));
    }

    Ok(merged_libraries)
}

/// Reject entry-root folders whose names collide with source-library import prefixes.
///
/// WHY: `foo/...` under the entry root and `@foo/...` source-library imports occupy distinct
/// syntactic surfaces, but allowing both roots to share a prefix makes diagnostics and generated
/// logical paths ambiguous.
pub(super) fn validate_entry_root_library_prefix_collisions(
    entry_root: &Path,
    source_libraries: &SourceLibraryRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let entry_dir_entries = fs::read_dir(entry_root).map_err(|error| {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                entry_root,
                format!(
                    "Failed to read entry root while checking for library prefix collisions: {error}"
                ),
                string_table,
            ),
            string_table,
        )
    })?;

    for entry in entry_dir_entries {
        let entry = entry.map_err(|error| {
            CompilerMessages::from_error_ref(
                CompilerError::file_error(
                    entry_root,
                    format!(
                        "Failed to read entry root directory entry while checking for library prefix collisions: {error}"
                    ),
                    string_table,
                ),
                string_table,
            )
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Some(folder_name) = path.file_name().and_then(|name| name.to_str())
            && source_libraries.has_prefix(folder_name)
        {
            return Err(project_structure_messages(
                &path,
                InvalidConfigReason::EntryRootLibraryPrefixCollision {
                    prefix: string_table.intern(folder_name),
                    entry_folder: path_id(&path, string_table),
                },
                string_table,
            ));
        }
    }

    Ok(())
}

fn scan_project_library_folder(
    config: &Config,
    folder_path: &Path,
    discovered_libraries: &mut SourceLibraryRegistry,
    discovered_prefixes: &mut HashMap<String, PathBuf>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let entries = fs::read_dir(folder_path).map_err(|error| {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                folder_path,
                format!("Failed to read configured library folder: {error}"),
                string_table,
            ),
            string_table,
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            CompilerMessages::from_error_ref(
                CompilerError::file_error(
                    folder_path,
                    format!("Failed to read library folder entry: {error}"),
                    string_table,
                ),
                string_table,
            )
        })?;

        let library_root = entry.path();
        if !library_root.is_dir() {
            continue;
        }

        let Some(prefix) = library_root.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let prefix = prefix.to_owned();

        // Prevent duplicate @prefixes across different project-local library roots.
        if let Some(previous_root) = discovered_prefixes.get(&prefix) {
            return Err(config_diagnostic_messages(
                config,
                "library_folders",
                InvalidConfigReason::SourceLibraryPrefixCollision {
                    prefix: string_table.intern(&prefix),
                    first_root: path_id(previous_root, string_table),
                    second_root: path_id(&library_root, string_table),
                },
                string_table,
            ));
        }

        discovered_prefixes.insert(prefix.clone(), library_root.clone());
        discovered_libraries.register_filesystem_root(prefix, library_root);
    }

    Ok(())
}

fn format_library_folder_list(library_folders: &[PathBuf]) -> String {
    let mut folders = library_folders
        .iter()
        .map(|folder| folder.display().to_string())
        .collect::<Vec<_>>();
    folders.sort();
    folders.join(", ")
}
