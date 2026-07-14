//! Project-local source-backed package discovery for Stage 0.
//!
//! WHAT: scans configured package folders, merges project-local packages with builder-provided
//! libraries, and rejects ambiguous import-prefix ownership.
//! WHY: source-backed package discovery is project input preparation. It should not live inside module
//! inventory or frontend semantic import handling.

use crate::builder_surface::{ProvidedSourceRoot, SourcePackageRegistry};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::source_packages::root_file::{
    HashRootFileDiscovery, PreparedSourcePackageRoots, discover_hash_root_file,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};

/// Prepare canonical source-backed package roots and their direct-child public-surface states.
///
/// WHAT: performs the one source-backed package filesystem preflight used by directory, single-file,
///     and config compilation.
/// WHY: Stage 0 owns filesystem preparation; path resolution consumes the resulting immutable
///     contract and must not scan or canonicalize source-backed package roots during construction.
pub(crate) fn prepare_source_package_roots(
    source_packages: &SourcePackageRegistry,
) -> PreparedSourcePackageRoots {
    let entries = source_packages.iter().map(|package| {
        let ProvidedSourceRoot::Filesystem(path) = &package.root;
        let canonical_root = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        let discovery = match discover_hash_root_file(&canonical_root) {
            Ok(HashRootFileDiscovery::Unique(root_file)) => match fs::canonicalize(&root_file) {
                Ok(canonical_root_file) => HashRootFileDiscovery::Unique(canonical_root_file),
                Err(error) => HashRootFileDiscovery::Unreadable(format!(
                    "Failed to canonicalize source-backed package public-surface file '{}': {error}",
                    root_file.display()
                )),
            },
            Ok(discovery) => discovery,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                HashRootFileDiscovery::Missing
            }
            Err(error) => HashRootFileDiscovery::Unreadable(error.to_string()),
        };

        (package.import_prefix.clone(), canonical_root, discovery)
    });

    PreparedSourcePackageRoots::from_entries(entries)
}

/// Discover project-local source-backed packages from configured `package_folders`.
///
/// WHAT: scans each configured top-level folder under the project root and registers one source
/// package root per direct child directory.
/// WHY: project-local package discovery must follow config rather than hardcoding `/lib`.
pub(super) fn discover_project_local_source_packages(
    config: &Config,
    project_root: &Path,
    string_table: &mut StringTable,
) -> Result<SourcePackageRegistry, CompilerMessages> {
    let mut discovered_packages = SourcePackageRegistry::new();
    let mut discovered_prefixes: BTreeMap<String, PathBuf> = BTreeMap::new();

    for configured_folder in &config.package_folders {
        let folder_path = project_root.join(configured_folder);

        // Validate configured package roots before scanning children so config mistakes stay as
        // typed diagnostics instead of becoming later import-resolution failures.
        if !folder_path.exists() {
            if config.has_explicit_package_folders {
                return Err(config_diagnostic_messages(
                    config,
                    "package_folders",
                    InvalidConfigReason::ConfiguredPackageFolderMissing {
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
                "package_folders",
                InvalidConfigReason::ConfiguredPackageFolderNotDirectory {
                    folder: path_id(configured_folder, string_table),
                },
                string_table,
            ));
        }

        scan_project_package_folder(
            config,
            &folder_path,
            &mut discovered_packages,
            &mut discovered_prefixes,
            string_table,
        )?;
    }

    Ok(discovered_packages)
}

/// Merge builder-provided and project-local packages, preserving the builder/project collision
/// diagnostic that config validation expects.
pub(super) fn merge_source_packages(
    config: &Config,
    builder_source_packages: &SourcePackageRegistry,
    project_local_packages: &SourcePackageRegistry,
    string_table: &mut StringTable,
) -> Result<SourcePackageRegistry, CompilerMessages> {
    let mut merged_packages = builder_source_packages.clone();

    if let Err(collisions) = merged_packages.merge(project_local_packages) {
        let collision_list = collisions.join(", ");

        return Err(config_diagnostic_messages(
            config,
            "package_folders",
            InvalidConfigReason::SourcePackageBuilderPrefixCollision {
                prefixes: string_table.get_or_intern(collision_list),
                package_folders: string_table
                    .get_or_intern(format_package_folder_list(&config.package_folders)),
            },
            string_table,
        ));
    }

    Ok(merged_packages)
}

fn scan_project_package_folder(
    config: &Config,
    folder_path: &Path,
    discovered_packages: &mut SourcePackageRegistry,
    discovered_prefixes: &mut BTreeMap<String, PathBuf>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let entries = fs::read_dir(folder_path).map_err(|error| {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                folder_path,
                format!("Failed to read configured package folder: {error}"),
                string_table,
            ),
            string_table,
        )
    })?;

    // Collect directory entries before registration so prefix collision diagnostics and
    // registration order are deterministic regardless of filesystem iteration order.
    let mut library_entries: Vec<(String, PathBuf)> = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|error| {
            CompilerMessages::from_error_ref(
                CompilerError::file_error(
                    folder_path,
                    format!("Failed to read package folder entry: {error}"),
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
        library_entries.push((prefix.to_owned(), library_root));
    }

    library_entries.sort_by(|(prefix_a, _), (prefix_b, _)| prefix_a.cmp(prefix_b));

    for (prefix, library_root) in library_entries {
        // Prevent duplicate @prefixes across different project-local package roots.
        if let Some(previous_root) = discovered_prefixes.get(&prefix) {
            return Err(config_diagnostic_messages(
                config,
                "package_folders",
                InvalidConfigReason::SourcePackagePrefixCollision {
                    prefix: string_table.intern(&prefix),
                    first_root: path_id(previous_root, string_table),
                    second_root: path_id(&library_root, string_table),
                },
                string_table,
            ));
        }

        discovered_prefixes.insert(prefix.clone(), library_root.clone());
        discovered_packages.register_filesystem_root(
            prefix,
            library_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );
    }

    Ok(())
}

fn format_package_folder_list(package_folders: &[PathBuf]) -> String {
    let mut folders = package_folders
        .iter()
        .map(|folder| folder.display().to_string())
        .collect::<Vec<_>>();
    folders.sort();
    folders.join(", ")
}
