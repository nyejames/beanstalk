//! Single-file and directory frontend compilation.
//!
//! WHAT: compiles project modules through the frontend pipeline for single-file and directory entries.
//! WHY: separating the two flows keeps each path readable as orchestration over named steps.

use crate::build_system::build::{CompiledModuleResult, Module};
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::frontend_orchestration::FrontendModuleBuildContext;
use super::module_discovery;
use super::reachable_file_discovery;

/// Compile a single `.bst` file as its own module.
pub(crate) fn compile_single_file_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
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
        external_packages,
        string_table,
    )?;
    let local_table = string_table.clone();
    let result = FrontendModuleBuildContext {
        config,
        build_profile,
        project_path_resolver: Some(project_path_resolver),
        style_directives,
        external_packages,
    }
    .compile_module(&input_files, &entry_path, local_table)?;

    let remap = string_table.merge_from(&result.string_table);
    let mut module = result.module;
    module.remap_string_ids(&remap);
    Ok(vec![module])
}

/// Discover all entry modules in a directory project and compile each one.
pub(crate) fn compile_directory_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    let project_path_resolver =
        module_discovery::build_project_path_resolver(config, string_table)?;

    let discovered_modules = match module_discovery::discover_all_modules_in_project(
        config,
        &project_path_resolver,
        style_directives,
        external_packages,
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
                    external_packages,
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
