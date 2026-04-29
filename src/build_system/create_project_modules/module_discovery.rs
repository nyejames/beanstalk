//! Project-level module inventory for Beanstalk directory projects.
//!
//! Discovers all root entry files (`#*.bst`) under the configured entry root, resolves each to
//! its full set of reachable source files, and assembles `DiscoveredModule` values ready for
//! frontend compilation.

use super::reachable_file_discovery::discover_reachable_files;
use super::source_loading::extract_source_code;
use crate::build_system::build::InputFile;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::paths::path_resolution::{
    ProjectPathResolver, resolve_project_entry_root,
};
use crate::compiler_frontend::source_libraries::mod_file::{MOD_FILE_NAME, file_name_is_mod_file};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::SourceLibraryRegistry;
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::return_file_error;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Entry point and all collected source files for one discovered module.
pub(crate) struct DiscoveredModule {
    pub(crate) entry_point: PathBuf,
    pub(crate) input_files: Vec<InputFile>,
}

/// Build the canonical path resolver for a directory project.
///
/// WHY: both `project_root` and `entry_root` must be canonicalized before path resolution; doing
/// this in one helper keeps the canonicalization logic in one place.
pub(super) fn build_project_path_resolver(
    config: &Config,
    builder_source_libraries: &SourceLibraryRegistry,
    string_table: &mut StringTable,
) -> Result<ProjectPathResolver, CompilerMessages> {
    let project_root = match fs::canonicalize(&config.entry_dir) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &config.entry_dir,
                format!("Failed to canonicalize project root: {error}"),
                string_table,
            );
            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };
    let entry_root_path = resolve_project_entry_root(config);
    if !entry_root_path.exists() {
        let file_error = CompilerError::file_error(
            &entry_root_path,
            format!(
                "Configured entry root '{}' does not exist",
                entry_root_path.display()
            ),
            string_table,
        );
        return Err(CompilerMessages::from_error_ref(file_error, string_table));
    }
    let entry_root = match fs::canonicalize(&entry_root_path) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &entry_root_path,
                format!("Failed to canonicalize configured entry root: {error}"),
                string_table,
            );
            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };

    let project_local_libraries =
        discover_project_local_source_libraries(config, &project_root, string_table)?;

    // Check for prefix collisions between builder-provided and project-local libraries.
    let mut merged_libraries = builder_source_libraries.clone();
    if let Err(collisions) = merged_libraries.merge(&project_local_libraries) {
        let collision_list = collisions.join(", ");
        let mut error = CompilerError::file_error(
            &project_root,
            format!(
                "Project-local source libraries collide with builder-provided libraries: {collision_list}"
            ),
            string_table,
        );
        error.new_metadata_entry(
            ErrorMetaDataKey::CompilationStage,
            String::from("Project Structure"),
        );
        error.new_metadata_entry(
            ErrorMetaDataKey::PrimarySuggestion,
            format!(
                "Rename or remove the conflicting project-local library prefix, or update '#library_folders' (currently: {}).",
                format_library_folder_list(&config.library_folders)
            ),
        );
        return Err(CompilerMessages::from_error_ref(error, string_table));
    }

    // Check for collisions between entry-root top-level folders and source-library prefixes.
    let entry_dir_entries = fs::read_dir(&entry_root).map_err(|error| {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                &entry_root,
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
                    &entry_root,
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
            && merged_libraries.has_prefix(folder_name)
        {
            let mut error = CompilerError::file_error(
                &path,
                format!(
                    "Entry-root folder '{folder_name}' collides with source-library prefix '@{folder_name}'. Ambiguous imports are disallowed."
                ),
                string_table,
            )
            .with_error_type(ErrorType::Config);
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Project Structure"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                "Rename the entry-root folder or the source-library directory so they do not share the same name.".to_string(),
            );
            return Err(CompilerMessages::from_error_ref(error, string_table));
        }
    }

    match ProjectPathResolver::new(project_root, entry_root, &merged_libraries) {
        Ok(resolver) => {
            // Validate that every source library root has a #mod.bst facade file.
            for (prefix, root) in resolver.source_library_roots() {
                let mod_file = root.join(MOD_FILE_NAME);
                if !mod_file.is_file() {
                    let mut error = CompilerError::file_error(
                        root,
                        format!(
                            "Source library '@{prefix}' is missing a #mod.bst facade file. Every source library must declare its public export surface through a #mod.bst facade."
                        ),
                        string_table,
                    );
                    error.new_metadata_entry(
                        ErrorMetaDataKey::CompilationStage,
                        String::from("Project Structure"),
                    );
                    error.new_metadata_entry(
                        ErrorMetaDataKey::PrimarySuggestion,
                        String::from("Create a #mod.bst file in the library root that exports the public symbols with '#'."), 
                    );
                    return Err(CompilerMessages::from_error_ref(error, string_table));
                }
            }
            Ok(resolver)
        }
        Err(error) => Err(CompilerMessages::from_error_ref(error, string_table)),
    }
}

/// Discover project-local source libraries from configured `#library_folders`.
///
/// WHAT: scans each configured top-level folder under the project root and registers one source
/// library root per direct child directory.
/// WHY: project-local library discovery must follow config rather than hardcoding `/lib`.
fn discover_project_local_source_libraries(
    config: &Config,
    project_root: &Path,
    string_table: &mut StringTable,
) -> Result<SourceLibraryRegistry, CompilerMessages> {
    let mut discovered_libraries = SourceLibraryRegistry::new();
    let mut discovered_prefixes: HashMap<String, PathBuf> = HashMap::new();

    for configured_folder in &config.library_folders {
        let folder_path = project_root.join(configured_folder);
        if !folder_path.exists() {
            if config.has_explicit_library_folders {
                let mut error = CompilerError::file_error(
                    &folder_path,
                    format!(
                        "Configured library folder '{}' does not exist.",
                        configured_folder.display()
                    ),
                    string_table,
                )
                .with_error_type(ErrorType::Config);
                error.new_metadata_entry(
                    ErrorMetaDataKey::CompilationStage,
                    String::from("Project Structure"),
                );
                error.new_metadata_entry(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Create the folder or remove it from '#library_folders'.".to_string(),
                );
                return Err(CompilerMessages::from_error_ref(error, string_table));
            }
            continue;
        }

        if !folder_path.is_dir() {
            let mut error = CompilerError::file_error(
                &folder_path,
                format!(
                    "Configured library folder '{}' is not a directory.",
                    configured_folder.display()
                ),
                string_table,
            )
            .with_error_type(ErrorType::Config);
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("Project Structure"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                "Point '#library_folders' to top-level directories.".to_string(),
            );
            return Err(CompilerMessages::from_error_ref(error, string_table));
        }

        let entries = fs::read_dir(&folder_path).map_err(|error| {
            CompilerMessages::from_error_ref(
                CompilerError::file_error(
                    &folder_path,
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
                        &folder_path,
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
            let prefix = prefix.to_string();

            if let Some(previous_root) = discovered_prefixes.get(&prefix) {
                let mut error = CompilerError::file_error(
                    &library_root,
                    format!(
                        "Configured library folder collision: source library prefix '@{prefix}' is defined by both '{}' and '{}'.",
                        previous_root.display(),
                        library_root.display()
                    ),
                    string_table,
                )
                .with_error_type(ErrorType::Config);
                error.new_metadata_entry(
                    ErrorMetaDataKey::CompilationStage,
                    String::from("Project Structure"),
                );
                error.new_metadata_entry(
                    ErrorMetaDataKey::PrimarySuggestion,
                    "Rename one of the colliding source-library directories so each '@prefix' is unique.".to_string(),
                );
                return Err(CompilerMessages::from_error_ref(error, string_table));
            }

            discovered_prefixes.insert(prefix.clone(), library_root.clone());
            discovered_libraries.register_filesystem_root(prefix, library_root);
        }
    }

    Ok(discovered_libraries)
}

fn format_library_folder_list(library_folders: &[PathBuf]) -> String {
    let mut folders = library_folders
        .iter()
        .map(|folder| folder.display().to_string())
        .collect::<Vec<_>>();
    folders.sort();
    folders.join(", ")
}

pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerError> {
    let source_root = resolve_project_entry_root(config);
    if !source_root.exists() {
        return_file_error!(
            string_table,
            &source_root,
            format!(
                "Configured entry root '{}' does not exist",
                source_root.display()
            ),
            {
                CompilationStage => String::from("Project Structure"),
                PrimarySuggestion => String::from("Set '#entry_root' in #config.bst to an existing directory"),
            }
        );
    }

    let entry_points = discover_root_entry_files(project_path_resolver.entry_root(), string_table)?;
    if entry_points.is_empty() {
        return_file_error!(
            string_table,
            project_path_resolver.entry_root(),
            "No root module entries were found. Expected at least one '#*.bst' file under the configured entry root.",
            {
                CompilationStage => String::from("Project Structure"),
                PrimarySuggestion => String::from("Add at least one entry file like '#page.bst' under the configured entry root"),
            }
        );
    }

    let mut modules = Vec::with_capacity(entry_points.len());
    for entry_point in entry_points {
        let reachable_files = discover_reachable_files(
            &entry_point,
            project_path_resolver,
            style_directives,
            external_packages,
            string_table,
        )?;

        let mut input_files = Vec::with_capacity(reachable_files.len());
        for source_path in reachable_files {
            input_files.push(InputFile {
                source_code: extract_source_code(&source_path, string_table)?,
                source_path,
            });
        }

        modules.push(DiscoveredModule {
            entry_point,
            input_files,
        });
    }

    Ok(modules)
}

fn discover_root_entry_files(
    source_root: &Path,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, CompilerError> {
    let mut discovered = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(source_root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let entries = fs::read_dir(&dir).map_err(|error| {
            CompilerError::file_error(
                &dir,
                format!("Failed to read directory while discovering modules: {error}"),
                string_table,
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CompilerError::file_error(
                    &dir,
                    format!("Failed to read directory entry while discovering modules: {error}"),
                    string_table,
                )
            })?;
            let path = entry.path();

            if path.is_dir() {
                queue.push_back(path);
                continue;
            }

            if path.extension().and_then(|extension| extension.to_str())
                != Some(BEANSTALK_FILE_EXTENSION)
            {
                continue;
            }

            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !file_name.starts_with('#') || file_name == settings::CONFIG_FILE_NAME {
                continue;
            }

            // Exclude #mod.bst so source library facades are never treated as module entries.
            if file_name_is_mod_file(file_name) {
                continue;
            }

            discovered.push(fs::canonicalize(&path).map_err(|error| {
                CompilerError::file_error(
                    &path,
                    format!("Failed to canonicalize module entry path: {error}"),
                    string_table,
                )
            })?);
        }
    }

    discovered.sort();
    Ok(discovered)
}
