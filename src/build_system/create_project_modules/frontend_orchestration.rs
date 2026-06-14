//! Per-module frontend compilation pipeline for Beanstalk projects.
//!
//! Drives a single discovered module through the full frontend pipeline:
//! parallel file preparation (tokenization + header parsing) → dependency sort → AST → HIR →
//! borrow checking.

use crate::build_system::build::{CompiledModuleResult, InputFile, Module, ResolvedConstFragment};

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::{
    ExternalFunctionId, ExternalPackageId, ExternalPackageRegistry,
};
use crate::compiler_frontend::headers::parse_file_headers::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, HeaderKind, HeaderParseOptions, Headers,
    parse_headers,
};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::reachability::collect_reachability_from_start;
use crate::compiler_frontend::instrumentation::{FrontendCounter, add_frontend_counter};
use crate::compiler_frontend::module_dependencies::SortedHeaders;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::{
    CompilerFrontend, FrontendBuildProfile, FrontendFilePrepareContext, FrontendFilePrepareInput,
};
use crate::libraries::external_import_providers::provider::BuilderRuntimePackageMetadata;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;

use crate::projects::settings::Config;
use crate::{benchmark_timer_log, borrow_log};

use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::path::Path;
use std::time::Instant;

// -------------------------
//  Compilation Context
// -------------------------

/// Lifetime-bound context for compiling one module through the full frontend pipeline.
///
/// WHAT: bundles the long-lived inputs shared across tokenization, headers, AST, HIR, and borrow
/// checking for a single module.
/// WHY: bundling these together keeps call sites in the coordinator short and makes the
/// `StringTable` handoff between orchestration and `CompilerFrontend` explicit in one place.
pub(super) struct FrontendModuleBuildContext<'a> {
    pub(super) config: &'a Config,
    pub(super) build_profile: FrontendBuildProfile,
    pub(super) project_path_resolver: Option<ProjectPathResolver>,
    pub(super) style_directives: &'a StyleDirectiveRegistry,
    pub(super) external_packages: &'a ExternalPackageRegistry,
    pub(super) external_import_resolution_table: &'a ExternalImportResolutionTable,
    pub(super) builder_runtime_packages: &'a [BuilderRuntimePackageMetadata],
}

impl FrontendModuleBuildContext<'_> {
    /// Compile one discovered module through the full frontend pipeline.
    pub(super) fn compile_module(
        self,
        module: &[InputFile],
        entry_file_path: &Path,
        string_table: StringTable,
    ) -> Result<CompiledModuleResult, CompilerMessages> {
        record_module_input_counters(module);

        let external_import_resolution_table = self.external_import_resolution_table;

        let mut compiler = CompilerFrontend::new(
            self.config,
            string_table,
            self.style_directives.to_owned(),
            self.external_packages.clone(),
            self.project_path_resolver.clone(),
        );

        let compile_result = (|| {
            let mut warnings = Vec::new();

            // 1. Map input source files into the compiler's source table.
            Self::attach_source_files(&mut compiler, module, entry_file_path)?;

            // 2. Prepare all files: tokenize and parse headers in one local string-table
            //    per file, then merge/remap once before aggregation.
            let (module_headers, file_warnings) =
                timed_frontend_stage("file_prepare_ms", "Files Prepared in: ", || {
                    Self::prepare_module_files(
                        &mut compiler,
                        module,
                        entry_file_path,
                        external_import_resolution_table,
                    )
                })?;
            warnings.extend(file_warnings);

            // 3. Resolve dependencies and sort headers for linear processing.
            let sorted = timed_frontend_stage(
                "dependency_sort_ms",
                "Dependency graph created in: ",
                || Self::sort_headers(&mut compiler, module_headers, &warnings),
            )?;

            let entry_runtime_fragment_count = sorted.entry_runtime_fragment_count;

            // 4. Build the Abstract Syntax Tree (AST).
            let module_ast = timed_frontend_stage("ast_ms", "AST created in: ", || {
                self.build_ast(&mut compiler, sorted, entry_file_path, &mut warnings)
            })?;

            // 5. Resolve const fragment StringIds to strings before AST is consumed by HIR.
            let const_top_level_fragments = module_ast
                .const_top_level_fragments
                .iter()
                .map(|fragment| ResolvedConstFragment {
                    runtime_insertion_index: fragment.runtime_insertion_index,
                    rendered_text: compiler.string_table.resolve(fragment.value).to_owned(),
                })
                .collect::<Vec<_>>();

            // 6. Lower AST to Higher-level Intermediate Representation (HIR).
            let (hir_module, type_environment) =
                timed_frontend_stage("hir_ms", "HIR generated in: ", || {
                    Self::lower_hir(&mut compiler, module_ast, &warnings)
                })?;

            // 7. Run static analysis (Borrow Checker).
            let borrow_analysis =
                timed_frontend_stage("borrow_ms", "Borrow checking completed in: ", || {
                    Self::check_borrows(&compiler, &hir_module, &warnings)
                })?;
            record_borrow_counters(&borrow_analysis);

            // Runtime import metadata is tied to calls that can execute from entry `start`.
            // The registry and provider table stay fully populated for type checking and
            // diagnostics; only the backend-facing module metadata is reachability-filtered.
            let reachability = collect_reachability_from_start(&hir_module)
                .map_err(|error| CompilerMessages::from_error_ref(error, &compiler.string_table))?;
            let reachable_external_package_ids = collect_reachable_external_package_ids(
                &reachability.reachable_external_functions,
                &compiler.external_package_registry,
            );

            // -------------------------
            //  Finalize Module Build
            // -------------------------

            borrow_log!("=== BORROW CHECKER OUTPUT ===");
            borrow_log!(format!(
                "Borrow checking completed successfully (states={} functions={} blocks={} conflicts_checked={} stmt_facts={} term_facts={} value_facts={})",
                borrow_analysis.analysis.total_state_snapshots(),
                borrow_analysis.stats.functions_analyzed,
                borrow_analysis.stats.blocks_analyzed,
                borrow_analysis.stats.conflicts_checked,
                borrow_analysis.analysis.statement_facts.len(),
                borrow_analysis.analysis.terminator_facts.len(),
                borrow_analysis.analysis.value_facts.len()
            ));
            borrow_log!("=== END BORROW CHECKER OUTPUT ===");

            // Collect provider-resolved imports used by this module after the frontend has
            // consumed them. HIR still carries only stable external IDs; this side payload is for
            // backend asset/glue planning.
            let source_logical_paths = collect_module_source_logical_paths(
                module,
                self.project_path_resolver.as_ref(),
                &mut compiler.string_table,
            )
            .map_err(|error| CompilerMessages::from_error_ref(error, &compiler.string_table))?;

            let mut module_external_imports: Vec<crate::build_system::build::ModuleExternalImport> =
                external_import_resolution_table
                    .collect_unique_resolved_imports_for_source_files(&source_logical_paths)
                    .into_iter()
                    .filter(|resolved| {
                        reachable_external_package_ids.contains(&resolved.package_id)
                    })
                    .map(
                        |resolved| crate::build_system::build::ModuleExternalImport {
                            package_id: resolved.package_id,
                            runtime_asset: resolved.runtime_asset,
                            required_runtime_imports: resolved.required_runtime_imports,
                        },
                    )
                    .collect();

            // Append reachable builder-runtime packages so they share the same runtime asset/glue
            // emission path as reachable provider-resolved imports.
            for builder_runtime in self.builder_runtime_packages {
                if reachable_external_package_ids.contains(&builder_runtime.package_id) {
                    module_external_imports.push(
                        crate::build_system::build::ModuleExternalImport {
                            package_id: builder_runtime.package_id,
                            runtime_asset: builder_runtime.runtime_asset.clone(),
                            required_runtime_imports: builder_runtime
                                .required_runtime_imports
                                .clone(),
                        },
                    );
                }
            }

            module_external_imports.sort_by_key(|import| import.package_id.0);
            module_external_imports.dedup_by_key(|import| import.package_id);

            Ok(Module {
                entry_point: entry_file_path.to_path_buf(),
                hir: hir_module,
                type_environment,
                borrow_analysis,
                warnings,
                const_top_level_fragments,
                entry_runtime_fragment_count,
                external_package_registry: compiler.external_package_registry.clone(),
                module_external_imports,
            })
        })();

        let string_table = compiler.string_table;

        compile_result.map(|module| CompiledModuleResult {
            module,
            string_table,
        })
    }

    // -------------------------
    //  Pipeline Stage Helpers
    // -------------------------

    fn attach_source_files(
        compiler: &mut CompilerFrontend,
        module: &[InputFile],
        entry_file_path: &Path,
    ) -> Result<(), CompilerMessages> {
        let source_files = SourceFileTable::build(
            module
                .iter()
                .map(|input_file| input_file.source_path.as_path()),
            entry_file_path,
            compiler.project_path_resolver.as_ref(),
            &mut compiler.string_table,
        )
        .map_err(|error| CompilerMessages::from_error_ref(error, &compiler.string_table))?;

        compiler.set_source_files(source_files);
        Ok(())
    }

    /// Prepare all source files in the module by tokenizing and header-parsing each one in
    /// parallel against a local string-table fork, then merging and remapping in deterministic
    /// input order.
    ///
    /// WHAT: replaces the previous serial loop with a Rayon parallel map where each worker owns
    ///       its own immutable inputs and local string table. Results are collected and merged
    ///       back into the module table in deterministic input order before module-wide aggregation.
    /// WHY: parallel preparation avoids serializing per-file work on the global string table,
    ///      while deterministic merge ordering preserves stable output across runs.
    fn prepare_module_files(
        compiler: &mut CompilerFrontend,
        module: &[InputFile],
        entry_file_path: &Path,
        external_import_resolution_table: &ExternalImportResolutionTable,
    ) -> Result<(Headers, Vec<CompilerDiagnostic>), CompilerMessages> {
        let entry_file_id = compiler
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map(|identity| identity.file_id);

        let options = HeaderParseOptions {
            entry_file_id,
            project_path_resolver: compiler.project_path_resolver.clone(),
        };

        // Create one shared fork source for all parallel workers. Each worker gets its own local
        // table forked from this immutable base, so no worker needs mutable access to the module
        // string table during tokenization or header parsing.
        let fork_source = compiler.string_table.fork_source();
        let base_len = fork_source.base_len();

        // Offsets are only relevant for entry files, and there is exactly one entry file per
        // module. Non-entry files produce zero const templates and zero runtime fragments, so
        // every file can safely start from offset zero without name collisions.
        let const_template_offset = 0usize;
        let runtime_fragment_offset = 0usize;

        // -------------------------
        //  Parallel file preparation
        // -------------------------

        let parallel_results: Vec<(
            usize,
            Result<FileFrontendPrepareOutput, FileFrontendPrepareError>,
            StringTable,
        )> = {
            let prepare_context = FrontendFilePrepareContext {
                source_files: &compiler.source_files,
                style_directives: &compiler.style_directives,
                external_package_registry: &compiler.external_package_registry,
                entry_file_path,
                options: &options,
            };

            module
                .par_iter()
                .enumerate()
                .map(|(index, file)| {
                    let (mut local_string_table, _) = fork_source.fork_for_module().into_parts();
                    let input = FrontendFilePrepareInput {
                        source_code: &file.source_code,
                        source_path: &file.source_path,
                        source_kind: file.source_kind,
                        const_template_offset,
                        runtime_fragment_offset,
                    };
                    let result = CompilerFrontend::prepare_file_frontend_local(
                        &prepare_context,
                        input,
                        &mut local_string_table,
                    );
                    (index, result, local_string_table)
                })
                .collect()
        };

        // Sort by original module input index so merge and aggregation stay deterministic
        // regardless of which parallel worker finishes first.
        let mut parallel_results = parallel_results;
        parallel_results.sort_by_key(|(index, _, _)| *index);

        let mut prepared_outputs = Vec::with_capacity(module.len());
        let mut warnings = Vec::new();
        let mut diagnostics = Vec::new();
        let mut const_fragment_source_count = 0usize;
        let mut runtime_fragment_source_count = 0usize;

        for (_index, result, local_string_table) in parallel_results {
            let remap = compiler
                .string_table
                .merge_delta_from(&local_string_table, base_len);

            match result {
                Ok(mut output) => {
                    if output.const_template_count > 0 {
                        const_fragment_source_count += 1;
                    }
                    if output.runtime_fragment_count > 0 {
                        runtime_fragment_source_count += 1;
                    }
                    output.remap_string_ids(&remap);
                    warnings.append(&mut output.warnings);
                    prepared_outputs.push(output);
                }
                Err(mut error) => {
                    error.remap_string_ids(&remap);
                    warnings.extend(error.warnings);
                    diagnostics.push(*error.diagnostic);
                }
            }
        }

        debug_assert!(
            const_fragment_source_count <= 1,
            "only the single entry file may contribute top-level const templates"
        );
        debug_assert!(
            runtime_fragment_source_count <= 1,
            "only the single entry file may contribute runtime fragments"
        );

        if !diagnostics.is_empty() {
            let mut messages =
                CompilerMessages::from_diagnostics(diagnostics, compiler.string_table.clone());
            messages.prepend_diagnostics_preserving_context(warnings);
            return Err(messages);
        }

        let prepared_file_count = prepared_outputs.len();
        let token_count = prepared_outputs
            .iter()
            .map(|output| output.token_count)
            .sum();
        let headers = parse_headers(
            prepared_outputs,
            &compiler.external_package_registry,
            external_import_resolution_table,
            options.project_path_resolver.as_ref(),
            &mut compiler.string_table,
        )
        .map_err(|bag| {
            let mut messages = CompilerMessages::from_diagnostics(
                bag.into_diagnostics(),
                compiler.string_table.clone(),
            );
            messages.prepend_diagnostics_preserving_context(warnings.iter().cloned());
            messages
        })?;

        add_frontend_counter(FrontendCounter::PreparedFileCount, prepared_file_count);
        add_frontend_counter(FrontendCounter::TokenCount, token_count);
        record_header_counters(&headers);

        Ok((headers, warnings))
    }

    fn sort_headers(
        compiler: &mut CompilerFrontend,
        module_headers: Headers,
        warnings: &[CompilerDiagnostic],
    ) -> Result<SortedHeaders, CompilerMessages> {
        compiler.sort_headers(module_headers).map_err(|bag| {
            let mut messages = CompilerMessages::from_diagnostics(
                bag.into_diagnostics(),
                compiler.string_table.clone(),
            );
            messages.prepend_diagnostics_preserving_context(warnings.iter().cloned());
            messages
        })
    }

    fn build_ast(
        &self,
        compiler: &mut CompilerFrontend,
        sorted: SortedHeaders,
        entry_file_path: &Path,
        warnings: &mut Vec<CompilerDiagnostic>,
    ) -> Result<Ast, CompilerMessages> {
        match compiler.headers_to_ast(sorted, entry_file_path, self.build_profile) {
            Ok(ast) => {
                warnings.extend(ast.warnings.clone());
                Ok(ast)
            }

            Err(messages) => Err(merge_stage_messages(
                messages,
                warnings,
                &compiler.string_table,
            )),
        }
    }

    fn lower_hir(
        compiler: &mut CompilerFrontend,
        module_ast: Ast,
        warnings: &[CompilerDiagnostic],
    ) -> Result<(HirModule, TypeEnvironment), CompilerMessages> {
        compiler
            .generate_hir(module_ast)
            .map_err(|messages| merge_stage_messages(messages, warnings, &compiler.string_table))
    }

    fn check_borrows(
        compiler: &CompilerFrontend,
        hir_module: &HirModule,
        warnings: &[CompilerDiagnostic],
    ) -> Result<BorrowCheckReport, CompilerMessages> {
        compiler
            .check_borrows(hir_module)
            .map_err(|messages| merge_stage_messages(messages, warnings, &compiler.string_table))
    }
}

// -------------------------
//  Shared Helpers
// -------------------------

fn record_module_input_counters(module: &[InputFile]) {
    add_frontend_counter(FrontendCounter::ModuleCount, 1);
    add_frontend_counter(FrontendCounter::SourceFileCount, module.len());

    let source_byte_count = module
        .iter()
        .map(|input_file| input_file.source_code.len())
        .sum();
    add_frontend_counter(FrontendCounter::SourceByteCount, source_byte_count);
}

fn record_header_counters(headers: &Headers) {
    add_frontend_counter(FrontendCounter::HeaderCount, headers.headers.len());

    let import_count = headers
        .module_symbols
        .file_imports_by_source
        .values()
        .map(Vec::len)
        .sum();
    add_frontend_counter(FrontendCounter::ImportCount, import_count);

    let top_level_declaration_count = headers
        .headers
        .iter()
        .filter(|header| {
            matches!(
                header.kind,
                HeaderKind::Function { .. }
                    | HeaderKind::Constant { .. }
                    | HeaderKind::Struct { .. }
                    | HeaderKind::Choice { .. }
                    | HeaderKind::TypeAlias { .. }
            )
        })
        .count();
    add_frontend_counter(
        FrontendCounter::TopLevelDeclarationCount,
        top_level_declaration_count,
    );
}

fn record_borrow_counters(report: &BorrowCheckReport) {
    add_frontend_counter(
        FrontendCounter::BorrowFunctionCount,
        report.stats.functions_analyzed,
    );
    add_frontend_counter(
        FrontendCounter::BorrowBlockCount,
        report.stats.blocks_analyzed,
    );
    add_frontend_counter(
        FrontendCounter::BorrowConflictCheckCount,
        report.stats.conflicts_checked,
    );

    let state_snapshot_count = report.analysis.block_entry_states.len()
        + report.analysis.block_exit_states.len()
        + report.analysis.statement_entry_states.len();
    add_frontend_counter(
        FrontendCounter::BorrowStateSnapshotCount,
        state_snapshot_count,
    );

    add_frontend_counter(
        FrontendCounter::BorrowStatementFactCount,
        report.analysis.statement_facts.len(),
    );
    add_frontend_counter(
        FrontendCounter::BorrowTerminatorFactCount,
        report.analysis.terminator_facts.len(),
    );
    add_frontend_counter(
        FrontendCounter::BorrowValueFactCount,
        report.analysis.value_facts.len(),
    );
}

fn merge_stage_messages(
    messages: CompilerMessages,
    warnings: &[CompilerDiagnostic],
    string_table: &StringTable,
) -> CompilerMessages {
    let mut messages = messages;
    messages.prepend_diagnostics_preserving_context(warnings.iter().cloned());
    messages.string_table = string_table.clone();
    messages
}

fn collect_module_source_logical_paths(
    module: &[InputFile],
    project_path_resolver: Option<&ProjectPathResolver>,
    string_table: &mut StringTable,
) -> Result<Vec<String>, CompilerError> {
    let Some(project_path_resolver) = project_path_resolver else {
        return Ok(Vec::new());
    };

    let mut logical_paths = Vec::with_capacity(module.len());
    for input_file in module {
        let logical_path = project_path_resolver
            .logical_path_for_canonical_file(&input_file.source_path, string_table)?;
        logical_paths.push(logical_path.to_string_lossy().replace('\\', "/"));
    }

    Ok(logical_paths)
}

fn timed_frontend_stage<T>(
    metric_name: &str,
    label: &str,
    stage: impl FnOnce() -> Result<T, CompilerMessages>,
) -> Result<T, CompilerMessages> {
    let start = Instant::now();
    let result = stage();
    benchmark_timer_log!(start, metric_name, label);

    // Detailed-timer builds consume these through the macro; regular builds
    // still need to keep them visibly used.
    let _ = (start, metric_name, label);

    result
}

/// Maps reachable external functions to the packages that own them.
///
/// WHY: provider-created packages and builder-runtime packages use the same backend metadata path,
/// so the build boundary derives one package-ID set from HIR reachability and applies it to both
/// sources of available runtime metadata.
fn collect_reachable_external_package_ids(
    reachable_external_functions: &FxHashSet<ExternalFunctionId>,
    registry: &ExternalPackageRegistry,
) -> FxHashSet<ExternalPackageId> {
    let mut package_ids = FxHashSet::default();

    for function_id in reachable_external_functions {
        if let Some(package_id) = registry.resolve_function_package_id(*function_id) {
            package_ids.insert(package_id);
        }
    }

    package_ids
}

#[cfg(test)]
#[path = "../tests/frontend_orchestration_tests.rs"]
mod tests;
