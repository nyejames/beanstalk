//! Single-file and directory frontend compilation.
//!
//! WHAT: compiles project modules through the frontend pipeline for single-file and directory entries.
//! WHY: separating the two flows keeps each path readable as orchestration over named steps.

use crate::build_system::build::{CompiledModuleResult, Module};

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use crate::libraries::{LibrarySet, SourceFileKind};
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};

use rayon::prelude::*;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use super::frontend_orchestration::FrontendModuleBuildContext;
use super::module_inventory;
use super::project_roots;
use super::reachable_file_discovery;

// -------------------------
//  Single-File Compilation
// -------------------------

/// Compile a single `.bst` file as its own module.
pub(crate) fn compile_single_file_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    libraries: &mut LibrarySet,
    extension: &OsStr,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    // 1. Verify standard Beanstalk file extension.
    let extension_text = extension.to_str().unwrap_or_default();
    if extension_text != BEANSTALK_FILE_EXTENSION {
        if SourceFileKind::from_extension(extension_text).is_some() {
            let path = InternedPath::from_path_buf(&config.entry_dir, string_table);
            let extension = string_table.intern(extension_text);
            let location = SourceLocation::from_path(&config.entry_dir, string_table);
            let diagnostic =
                CompilerDiagnostic::invalid_source_file_entry(path, extension, location);

            return Err(CompilerMessages::from_diagnostic(
                diagnostic,
                string_table.clone(),
            ));
        }

        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Unsupported file extension for compilation. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
            string_table,
        );

        return Err(CompilerMessages::from_error_ref(err, string_table));
    }

    // 2. Resolve canonical entry path.
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

    // 3. Initialize path resolver for imports.
    let project_path_resolver = match ProjectPathResolver::new(
        source_root.clone(),
        source_root.clone(),
        &libraries.source_libraries,
        &libraries.source_file_kinds,
    ) {
        Ok(resolver) => resolver,
        Err(error) => return Err(CompilerMessages::from_error_ref(error, string_table)),
    };

    // 4. Discover all transitively reachable files.
    let mut external_imports = reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut libraries.external_packages,
        providers: &libraries.external_import_providers,
        cache: &mut libraries.external_import_cache,
        resolution_table: &mut libraries.external_import_resolution_table,
    };

    let input_files = reachable_file_discovery::collect_reachable_input_files(
        &entry_path,
        &project_path_resolver,
        style_directives,
        &mut external_imports,
        string_table,
    )?;

    // 5. Run the module compilation pipeline with a local string-table delta.
    let string_table_fork = string_table.fork_for_module();
    let (local_table, base_len) = string_table_fork.into_parts();
    let result = FrontendModuleBuildContext {
        config,
        build_profile,
        project_path_resolver: Some(project_path_resolver),
        style_directives,
        external_packages: &libraries.external_packages,
        external_import_resolution_table: &libraries.external_import_resolution_table,
        builder_runtime_packages: &libraries.builder_runtime_packages,
    }
    .compile_module(&input_files, &entry_path, local_table)?;

    // 6. Merge local results back into the global build context.
    let remap = string_table.merge_delta_from(&result.string_table, base_len);
    let mut module = result.module;
    if !remap.is_identity() {
        module.remap_string_ids(&remap);
    }

    Ok(vec![module])
}

// -------------------------
//  Directory Compilation
// -------------------------

/// Parallel compilation result plus the fork marker needed at merge time.
struct ModuleCompileOutcome {
    entry_path: PathBuf,
    string_table_base_len: usize,
    result: Result<CompiledModuleResult, CompilerMessages>,
}

struct SuccessfulModuleCompilation {
    string_table_base_len: usize,
    compiled: CompiledModuleResult,
}

struct FailedModuleCompilation {
    string_table_base_len: usize,
    messages: CompilerMessages,
}

/// Discover all entry modules in a directory project and compile each one.
pub(crate) fn compile_directory_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    libraries: &mut LibrarySet,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    // 1. Setup path resolution based on config settings.
    let project_path_resolver = project_roots::build_project_path_resolver(
        config,
        &libraries.source_libraries,
        &libraries.source_file_kinds,
        string_table,
    )?;

    // 2. Scan the directory for entry modules and their reachable files.
    let mut external_imports = reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut libraries.external_packages,
        providers: &libraries.external_import_providers,
        cache: &mut libraries.external_import_cache,
        resolution_table: &mut libraries.external_import_resolution_table,
    };

    let discovered_modules = module_inventory::discover_all_modules_in_project(
        config,
        &project_path_resolver,
        style_directives,
        &mut external_imports,
        string_table,
    )?;

    // 3. Compile modules in parallel, each with its own local string-table delta.
    //
    // The fork source owns one shared base snapshot for the whole batch. Individual module forks
    // then keep only strings introduced during that module's frontend pipeline.
    let string_table_fork_source = string_table.fork_source();
    let results: Vec<ModuleCompileOutcome> = discovered_modules
        .into_par_iter()
        .map(|discovered| {
            let string_table_fork = string_table_fork_source.fork_for_module();
            let (local_table, base_len) = string_table_fork.into_parts();
            let result = FrontendModuleBuildContext {
                config,
                build_profile,
                project_path_resolver: Some(project_path_resolver.clone()),
                style_directives,
                external_packages: &libraries.external_packages,
                external_import_resolution_table: &libraries.external_import_resolution_table,
                builder_runtime_packages: &libraries.builder_runtime_packages,
            }
            .compile_module(
                &discovered.input_files,
                &discovered.entry_point,
                local_table,
            );

            ModuleCompileOutcome {
                entry_path: discovered.entry_point,
                string_table_base_len: base_len,
                result,
            }
        })
        .collect();

    // 4. Deterministic ordering by entry path.
    let mut results = results;
    results.sort_by(|a, b| a.entry_path.cmp(&b.entry_path));

    // 5. Partition into successes and failures.
    let mut successes = Vec::with_capacity(results.len());
    let mut failures = Vec::new();

    for outcome in results {
        match outcome.result {
            Ok(compiled) => successes.push(SuccessfulModuleCompilation {
                string_table_base_len: outcome.string_table_base_len,
                compiled,
            }),
            Err(messages) => failures.push(FailedModuleCompilation {
                string_table_base_len: outcome.string_table_base_len,
                messages,
            }),
        }
    }

    // 6. If any module failed, aggregate all diagnostics deterministically and exit.
    if !failures.is_empty() {
        let aggregation_fork = string_table_fork_source.fork_for_module();
        let (mut aggregated_table, _) = aggregation_fork.into_parts();
        let mut aggregated_messages = CompilerMessages::empty(aggregated_table.clone());

        for mut failure in failures {
            let remap = aggregated_table.merge_delta_from(
                &failure.messages.string_table,
                failure.string_table_base_len,
            );

            if !remap.is_identity() {
                failure.messages.remap_string_ids(&remap);
            }

            aggregated_messages.append_messages_preserving_context(failure.messages);
        }

        aggregated_messages.string_table = aggregated_table;
        return Err(aggregated_messages);
    }

    // 7. All succeeded: merge each local table into the build table and remap.
    let mut compiled_modules = Vec::with_capacity(successes.len());

    for mut success in successes {
        let remap = string_table.merge_delta_from(
            &success.compiled.string_table,
            success.string_table_base_len,
        );
        if !remap.is_identity() {
            success.compiled.module.remap_string_ids(&remap);
        }
        compiled_modules.push(success.compiled.module);
    }

    Ok(compiled_modules)
}
