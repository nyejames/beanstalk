//! Single-file and directory frontend compilation.
//!
//! WHAT: compiles project modules through the frontend pipeline for single-file and directory entries.
//! WHY: separating the two flows keeps each path readable as orchestration over named steps.

use crate::build_system::build::{CompiledModuleResult, Module};

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::instrumentation::{FrontendCounter, add_frontend_counter};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringTable, StringTableForkSource};

use crate::libraries::{LibrarySet, SourceFileKind};
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};

use rayon::prelude::*;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::frontend_orchestration::FrontendModuleBuildContext;
use super::module_inventory;
use super::project_roots;
use super::reachable_file_discovery;
use super::root_validation::validate_source_library_roots;
use super::source_library_discovery::prepare_source_library_roots;
use super::source_tree_index::SourceTreeIndex;

/// Record a Stage 0 build-system timing through the central `timers` substrate.
///
/// WHAT: delegates to `timing::record_started_pipeline_timing`, which stores the
///      observation in the active collection scope and emits the stable
///      `BST_BENCH timing` line when the output mode permits.
/// WHY:  single-file and directory Stage 0 flows use dotted `stage0.*` metric names
///      through the concise `timers` substrate. The start token is zero-sized when
///      `timers` is off, so regular builds do not read clocks for instrumentation-only
///      measurements.
fn log_stage_timing(metric: &str, start: crate::timing::PipelineTimingStart) {
    crate::timing::record_started_pipeline_timing(metric, start);
}

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

    let total_start = crate::timing::start_pipeline_timing();

    // 2. Resolve canonical entry path.
    let entry_canonicalize_start = crate::timing::start_pipeline_timing();
    let entry_path = match fs::canonicalize(&config.entry_dir) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &config.entry_dir,
                format!("Failed to resolve entry file path: {error}"),
                string_table,
            );

            log_stage_timing(
                "stage0.single_file.entry_canonicalize",
                entry_canonicalize_start,
            );
            log_stage_timing("stage0.single_file.total", total_start);
            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };
    log_stage_timing(
        "stage0.single_file.entry_canonicalize",
        entry_canonicalize_start,
    );

    let source_root = entry_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    // 3. Initialize path resolver for imports.
    let path_resolver_start = crate::timing::start_pipeline_timing();
    let prepared_source_library_roots = prepare_source_library_roots(&libraries.source_libraries);
    if let Err(messages) =
        validate_source_library_roots(&prepared_source_library_roots, string_table)
    {
        log_stage_timing("stage0.single_file.path_resolver", path_resolver_start);
        log_stage_timing("stage0.single_file.total", total_start);
        return Err(messages);
    }

    let module_roots = if entry_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('#') && name.ends_with(".bst"))
    {
        match SourceTreeIndex::bounded_module_roots_for_single_file(
            &entry_path,
            config,
            string_table,
        ) {
            Ok(module_roots) => module_roots,
            Err(messages) => {
                log_stage_timing("stage0.single_file.path_resolver", path_resolver_start);
                log_stage_timing("stage0.single_file.total", total_start);
                return Err(messages);
            }
        }
    } else {
        crate::compiler_frontend::paths::module_roots::ModuleRootTable::empty()
    };
    let project_path_resolver = match ProjectPathResolver::new_with_module_roots(
        source_root.clone(),
        source_root.clone(),
        prepared_source_library_roots,
        &libraries.source_file_kinds,
        module_roots,
    ) {
        Ok(resolver) => resolver,
        Err(error) => {
            log_stage_timing("stage0.single_file.path_resolver", path_resolver_start);
            log_stage_timing("stage0.single_file.total", total_start);
            return Err(CompilerMessages::from_error_ref(error, string_table));
        }
    };
    log_stage_timing("stage0.single_file.path_resolver", path_resolver_start);

    // 4. Discover all transitively reachable files.
    let mut external_imports = reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut libraries.external_packages,
        providers: &libraries.external_import_providers,
        cache: &mut libraries.external_import_cache,
        resolution_table: &mut libraries.external_import_resolution_table,
    };

    let reachable_files_start = crate::timing::start_pipeline_timing();
    let input_files = match reachable_file_discovery::collect_reachable_input_files(
        &entry_path,
        &project_path_resolver,
        style_directives,
        &mut external_imports,
        string_table,
    ) {
        Ok(input_files) => input_files,
        Err(messages) => {
            log_stage_timing("stage0.single_file.reachable_files", reachable_files_start);
            log_stage_timing("stage0.single_file.total", total_start);
            return Err(messages);
        }
    };
    log_stage_timing("stage0.single_file.reachable_files", reachable_files_start);

    // Share the effective external package registry immutably for the rest of the frontend
    // pipeline so each stage does not need its own deep clone.
    let external_packages = Arc::new(libraries.external_packages.clone());

    // 5. Run the module compilation pipeline with a local string-table delta.
    add_frontend_counter(FrontendCounter::ModuleCompilationSerialCount, 1);

    let string_table_fork_start = crate::timing::start_pipeline_timing();
    let string_table_fork = string_table.fork_for_module();
    let (local_table, base_len) = string_table_fork.into_parts();
    log_stage_timing(
        "stage0.single_file.string_table_fork",
        string_table_fork_start,
    );

    let compile_module_start = crate::timing::start_pipeline_timing();
    let result = match (FrontendModuleBuildContext {
        config,
        build_profile,
        project_path_resolver: Some(project_path_resolver),
        style_directives,
        external_packages: Arc::clone(&external_packages),
        external_import_resolution_table: &libraries.external_import_resolution_table,
        builder_runtime_packages: &libraries.builder_runtime_packages,
    })
    .compile_module(&input_files, &entry_path, local_table)
    {
        Ok(result) => result,
        Err(messages) => {
            log_stage_timing("stage0.single_file.compile_module", compile_module_start);
            log_stage_timing("stage0.single_file.total", total_start);
            return Err(messages);
        }
    };
    log_stage_timing("stage0.single_file.compile_module", compile_module_start);

    // 6. Merge local results back into the global build context.
    let merge_delta_start = crate::timing::start_pipeline_timing();
    let remap = string_table.merge_delta_from(&result.string_table, base_len);
    let mut module = result.module;
    if !remap.is_identity() {
        module.remap_string_ids(&remap);
    }
    log_stage_timing("stage0.single_file.merge_delta", merge_delta_start);

    log_stage_timing("stage0.single_file.total", total_start);

    Ok(vec![module])
}

// -------------------------
//  Directory Compilation
// -------------------------

/// Module compilation result plus the fork marker needed at merge time.
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

struct DirectoryModuleCompileContext<'a> {
    string_table_fork_source: &'a StringTableForkSource,
    config: &'a Config,
    build_profile: FrontendBuildProfile,
    project_path_resolver: &'a ProjectPathResolver,
    style_directives: &'a StyleDirectiveRegistry,
    external_packages: &'a Arc<ExternalPackageRegistry>,
    libraries: &'a LibrarySet,
}

impl DirectoryModuleCompileContext<'_> {
    fn compile(&self, discovered: module_inventory::DiscoveredModule) -> ModuleCompileOutcome {
        let string_table_fork = self.string_table_fork_source.fork_for_module();
        let (local_table, base_len) = string_table_fork.into_parts();
        let result = FrontendModuleBuildContext {
            config: self.config,
            build_profile: self.build_profile,
            project_path_resolver: Some(self.project_path_resolver.clone()),
            style_directives: self.style_directives,
            external_packages: Arc::clone(self.external_packages),
            external_import_resolution_table: &self.libraries.external_import_resolution_table,
            builder_runtime_packages: &self.libraries.builder_runtime_packages,
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
    }
}

/// Discover all entry modules in a directory project and compile each one.
pub(crate) fn compile_directory_frontend(
    config: &Config,
    build_profile: FrontendBuildProfile,
    style_directives: &StyleDirectiveRegistry,
    libraries: &mut LibrarySet,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    let total_start = crate::timing::start_pipeline_timing();

    // 1. Setup path resolution based on config settings.
    let path_resolver_start = crate::timing::start_pipeline_timing();
    let project_setup = match project_roots::build_project_path_resolver_with_index(
        config,
        &libraries.source_libraries,
        &libraries.source_file_kinds,
        string_table,
    ) {
        Ok(resolver) => resolver,
        Err(error) => {
            log_stage_timing("stage0.directory.path_resolver", path_resolver_start);
            log_stage_timing("stage0.directory.total", total_start);
            return Err(error);
        }
    };
    log_stage_timing("stage0.directory.path_resolver", path_resolver_start);
    let project_path_resolver = project_setup.resolver;

    // 2. Scan the directory for entry modules and their reachable files.
    let mut external_imports = reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut libraries.external_packages,
        providers: &libraries.external_import_providers,
        cache: &mut libraries.external_import_cache,
        resolution_table: &mut libraries.external_import_resolution_table,
    };

    let module_inventory_start = crate::timing::start_pipeline_timing();
    let discovered_modules = match module_inventory::discover_all_modules_in_project(
        config,
        &project_path_resolver,
        &project_setup.source_tree_index,
        style_directives,
        &mut external_imports,
        string_table,
    ) {
        Ok(discovered_modules) => discovered_modules,
        Err(messages) => {
            log_stage_timing("stage0.directory.module_inventory", module_inventory_start);
            log_stage_timing("stage0.directory.total", total_start);
            return Err(messages);
        }
    };
    log_stage_timing("stage0.directory.module_inventory", module_inventory_start);

    // Share the effective external package registry immutably across all module compilations;
    // directory modules may compile in parallel and can safely read the same Arc.
    let external_packages = Arc::new(libraries.external_packages.clone());

    // 3. Compile modules, each with its own local string-table delta.
    //
    // The fork source owns one shared base snapshot for the whole batch. Individual module forks
    // then keep only strings introduced during that module's frontend pipeline.
    let string_table_fork_source = string_table.fork_source();
    let compile_context = DirectoryModuleCompileContext {
        string_table_fork_source: &string_table_fork_source,
        config,
        build_profile,
        project_path_resolver: &project_path_resolver,
        style_directives,
        external_packages: &external_packages,
        libraries,
    };
    let compile_in_parallel = discovered_modules.len() > 1;
    if compile_in_parallel {
        add_frontend_counter(
            FrontendCounter::ModuleCompilationParallelTaskCount,
            discovered_modules.len(),
        );
    } else {
        add_frontend_counter(
            FrontendCounter::ModuleCompilationSerialCount,
            discovered_modules.len(),
        );
    }

    let module_compile_batch_start = crate::timing::start_pipeline_timing();
    let results: Vec<ModuleCompileOutcome> = if compile_in_parallel {
        discovered_modules
            .into_par_iter()
            .map(|discovered| compile_context.compile(discovered))
            .collect()
    } else {
        discovered_modules
            .into_iter()
            .map(|discovered| compile_context.compile(discovered))
            .collect()
    };
    log_stage_timing(
        "stage0.directory.module_compile_batch",
        module_compile_batch_start,
    );

    // 4. Deterministic ordering by entry path.
    let result_sort_start = crate::timing::start_pipeline_timing();
    let mut results = results;
    results.sort_by(|a, b| a.entry_path.cmp(&b.entry_path));
    log_stage_timing("stage0.directory.result_sort", result_sort_start);

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
        let failure_aggregation_start = crate::timing::start_pipeline_timing();
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
        log_stage_timing(
            "stage0.directory.failure_aggregation",
            failure_aggregation_start,
        );
        log_stage_timing("stage0.directory.total", total_start);
        return Err(aggregated_messages);
    }

    // 7. All succeeded: merge each local table into the build table and remap.
    let success_merge_start = crate::timing::start_pipeline_timing();
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
    log_stage_timing("stage0.directory.success_merge", success_merge_start);

    log_stage_timing("stage0.directory.total", total_start);

    Ok(compiled_modules)
}
