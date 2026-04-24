//! Frontend compilation coordinator for Beanstalk projects.
//!
//! Dispatches to single-file or directory-project flows, then delegates to focused submodules:
//! - `frontend_orchestration`   — per-module pipeline (tokenization through borrow checking)
//! - `module_discovery`         — project-level entry-file and reachable-file discovery
//! - `reachable_file_discovery` — BFS traversal over import graphs
//! - `import_scanning`          — per-file import path extraction
//! - `source_loading`           — raw file I/O
//!
//! Stage 0 config loading lives in `project_config`. This module begins after config has been
//! applied to `Config`.

mod frontend_orchestration;
mod import_scanning;
mod module_discovery;
mod reachable_file_discovery;
mod source_loading;

#[cfg(test)]
pub(super) use module_discovery::{DiscoveredModule, discover_all_modules_in_project};
pub use source_loading::extract_source_code;

use crate::build_system::build::{CompiledModuleResult, Module};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::{Flag, FrontendBuildProfile};
#[cfg(test)]
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use frontend_orchestration::FrontendModuleBuildContext;
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Compile all project modules through the frontend pipeline.
///
/// WHAT: dispatches to single-file or directory-project flow depending on the entry path.
/// WHY: separating the two flows keeps each path readable as orchestration over named steps.
pub fn compile_project_frontend(
    config: &mut Config,
    flags: &[Flag],
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    let build_profile = if flags.contains(&Flag::Release) {
        FrontendBuildProfile::Release
    } else {
        FrontendBuildProfile::Dev
    };

    // Dispatch: single-file entry vs. directory project.
    if let Some(extension) = config.entry_dir.extension() {
        return compile_single_file_frontend(
            config,
            build_profile,
            style_directives,
            extension,
            string_table,
        );
    }

    if !config.entry_dir.is_dir() {
        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
            string_table,
        );
        return Err(CompilerMessages::from_error_ref(err, string_table));
    }

    compile_directory_frontend(config, build_profile, style_directives, string_table)
}

/// Compile a single `.bst` file as its own module.
fn compile_single_file_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    extension: &OsStr,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    if extension.to_str().unwrap_or_default() != BEANSTALK_FILE_EXTENSION {
        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Unsupported file extension for compilation. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
            string_table,
        );
        return Err(CompilerMessages::from_error_ref(err, string_table));
    }

    let entry_path = match fs::canonicalize(&config.entry_dir) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &config.entry_dir,
                format!("Failed to resolve entry file path: {error}"),
                string_table,
            );
            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };

    let source_root = entry_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let project_path_resolver = match ProjectPathResolver::new(
        source_root.clone(),
        source_root.clone(),
        &config.root_folders,
    ) {
        Ok(resolver) => resolver,
        Err(error) => return Err(CompilerMessages::from_error_ref(error, string_table)),
    };

    let input_files = reachable_file_discovery::collect_reachable_input_files(
        &entry_path,
        &project_path_resolver,
        style_directives,
        string_table,
    )?;
    let local_table = string_table.clone();
    let result = FrontendModuleBuildContext {
        config,
        build_profile,
        project_path_resolver: Some(project_path_resolver),
        style_directives,
    }
    .compile_module(&input_files, &entry_path, local_table)?;

    let remap = string_table.merge_from(&result.string_table);
    let mut module = result.module;
    module.remap_string_ids(&remap);
    Ok(vec![module])
}

/// Discover all entry modules in a directory project and compile each one.
fn compile_directory_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    let project_path_resolver =
        module_discovery::build_project_path_resolver(config, string_table)?;

    let discovered_modules = match module_discovery::discover_all_modules_in_project(
        config,
        &project_path_resolver,
        style_directives,
        string_table,
    ) {
        Ok(modules) => modules,
        Err(error) => return Err(CompilerMessages::from_error_ref(error, string_table)),
    };

    // Compile modules in parallel, each with its own cloned StringTable.
    let results: Vec<(PathBuf, Result<CompiledModuleResult, CompilerMessages>)> =
        discovered_modules
            .into_par_iter()
            .map(|discovered| {
                let local_table = string_table.clone();
                let result = FrontendModuleBuildContext {
                    config,
                    build_profile,
                    project_path_resolver: Some(project_path_resolver.clone()),
                    style_directives,
                }
                .compile_module(
                    &discovered.input_files,
                    &discovered.entry_point,
                    local_table,
                );
                (discovered.entry_point, result)
            })
            .collect();

    // Deterministic ordering by entry path.
    let mut results = results;
    results.sort_by(|a, b| a.0.cmp(&b.0));

    // Partition into successes and failures.
    let mut successes = Vec::with_capacity(results.len());
    let mut failures = Vec::new();
    for (entry_path, result) in results {
        match result {
            Ok(compiled) => successes.push((entry_path, compiled)),
            Err(messages) => failures.push((entry_path, messages)),
        }
    }

    // If any module failed, aggregate all diagnostics deterministically.
    if !failures.is_empty() {
        let mut aggregated_table = string_table.clone();
        let mut all_errors = Vec::new();
        let mut all_warnings = Vec::new();
        for (_, messages) in failures {
            let remap = aggregated_table.merge_from(&messages.string_table);
            for mut error in messages.errors {
                error.remap_string_ids(&remap);
                all_errors.push(error);
            }
            for mut warning in messages.warnings {
                warning.remap_string_ids(&remap);
                all_warnings.push(warning);
            }
        }
        return Err(CompilerMessages {
            errors: all_errors,
            warnings: all_warnings,
            string_table: aggregated_table,
        });
    }

    // All succeeded: merge each local table into the build table and remap.
    let mut compiled_modules = Vec::with_capacity(successes.len());
    for (_, mut compiled) in successes {
        let remap = string_table.merge_from(&compiled.string_table);
        compiled.module.remap_string_ids(&remap);
        compiled_modules.push(compiled.module);
    }

    Ok(compiled_modules)
}

#[cfg(test)]
#[path = "../tests/create_project_modules_tests.rs"]
mod create_project_modules_tests;

#[cfg(test)]
#[path = "../tests/compile_project_frontend_tests.rs"]
mod compile_project_frontend_tests;
