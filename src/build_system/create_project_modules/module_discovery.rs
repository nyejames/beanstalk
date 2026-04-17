//! Project-level module inventory for Beanstalk directory projects.
//!
//! Discovers all root entry files (`#*.bst`) under the configured entry root, resolves each to
//! its full set of reachable source files, and assembles `DiscoveredModule` values ready for
//! frontend compilation.

use super::reachable_file_discovery::discover_reachable_files;
use super::source_loading::extract_source_code;
use crate::build_system::build::InputFile;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::paths::path_resolution::{
    ProjectPathResolver, resolve_project_entry_root,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::return_file_error;
use std::collections::VecDeque;
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
    match ProjectPathResolver::new(project_root, entry_root, &config.root_folders) {
        Ok(resolver) => Ok(resolver),
        Err(error) => Err(CompilerMessages::from_error_ref(error, string_table)),
    }
}

pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
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

    project_path_resolver.validate_entry_root_collisions(string_table)?;

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
