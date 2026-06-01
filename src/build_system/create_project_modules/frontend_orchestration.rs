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
mod tests {
    use super::merge_stage_messages;
    use crate::compiler_frontend::compiler_errors::{CompilerMessages, SourceLocation};
    use crate::compiler_frontend::compiler_messages::display_messages::format_terse_compiler_messages;
    use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, TypeMismatchContext};
    use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;

    use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::{FrontendFilePrepareContext, FrontendFilePrepareInput};

    #[test]
    fn merge_stage_messages_preserves_render_type_context_with_warnings() {
        let string_table = StringTable::new();
        let type_environment = TypeEnvironment::new();
        let diagnostic = CompilerDiagnostic::type_mismatch(
            type_environment.builtins().int,
            type_environment.builtins().string,
            TypeMismatchContext::Assignment,
            SourceLocation::default(),
        );
        let warning = CompilerDiagnostic::unreachable_match_arm(SourceLocation::default());
        let messages = CompilerMessages::from_diagnostics(vec![diagnostic], string_table.clone())
            .with_type_context_for_all_diagnostics(type_environment);

        let merged = merge_stage_messages(messages, &[warning], &string_table);
        let rendered_lines = format_terse_compiler_messages(&merged);

        assert_eq!(merged.render_type_contexts().len(), 1);
        assert_eq!(rendered_lines.len(), 2);
        assert!(rendered_lines[1].contains("expected Int, found String"));
        assert!(!rendered_lines[1].contains("TypeId("));
    }

    #[test]
    fn fused_preparation_merges_local_forks_and_resolves_source_and_generated_strings() {
        use crate::compiler_frontend::CompilerFrontend;
        use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
        use crate::compiler_frontend::headers::parse_file_headers::{
            HeaderKind, HeaderParseOptions, parse_headers,
        };
        use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
        use crate::compiler_frontend::symbols::identity::SourceFileTable;
        use crate::compiler_frontend::tokenizer::tokens::TokenKind;
        use crate::projects::settings::Config;
        use std::fs;

        let temp_dir = tempfile::tempdir().expect("should create temp dir");
        let file_a = temp_dir.path().join("a.bst");
        let file_b = temp_dir.path().join("b.bst");
        // File A is the entry file with a runtime statement and a const template (which generates
        // a synthetic header name during header parsing).
        fs::write(&file_a, "alpha = 1\n#[hello]\n").unwrap();
        // File B is a normal source file with an exported constant declaration.
        fs::write(&file_b, "beta #= 2\n").unwrap();

        let canonical_a = fs::canonicalize(&file_a).unwrap();
        let canonical_b = fs::canonicalize(&file_b).unwrap();

        let mut string_table = StringTable::new();
        let source_files = SourceFileTable::build(
            &[&canonical_a, &canonical_b],
            &canonical_a,
            None,
            &mut string_table,
        )
        .expect("source file table should build");

        let module_table_size_before = string_table.len();

        let mut frontend = CompilerFrontend::new(
            &Config::new(temp_dir.path().to_path_buf()),
            string_table,
            StyleDirectiveRegistry::built_ins(),
            ExternalPackageRegistry::new(),
            None,
        );
        frontend.set_source_files(source_files);

        let options = HeaderParseOptions {
            entry_file_id: frontend
                .source_files
                .get_by_canonical_path(&canonical_a)
                .map(|i| i.file_id),
            project_path_resolver: frontend.project_path_resolver.clone(),
        };

        // Helper to prepare one file using the local-table variant and merge its delta back
        // into the module string table, returning the remapped output.
        let mut prepare_and_merge =
            |source_code: &str,
             source_path: &std::path::PathBuf,
             const_template_offset: usize,
             runtime_fragment_offset: usize| {
                let fork_source = frontend.string_table.fork_source();
                let (mut local_string_table, base_len) = fork_source.fork_for_module().into_parts();

                let result = {
                    let prepare_context = FrontendFilePrepareContext {
                        source_files: &frontend.source_files,
                        style_directives: &frontend.style_directives,
                        external_package_registry: &frontend.external_package_registry,
                        entry_file_path: &canonical_a,
                        options: &options,
                    };
                    let input = FrontendFilePrepareInput {
                        source_code,
                        source_path,
                        const_template_offset,
                        runtime_fragment_offset,
                    };

                    CompilerFrontend::prepare_file_frontend_local(
                        &prepare_context,
                        input,
                        &mut local_string_table,
                    )
                };

                let remap = frontend
                    .string_table
                    .merge_delta_from(&local_string_table, base_len);
                match result {
                    Ok(mut output) => {
                        output.remap_string_ids(&remap);
                        Ok(output)
                    }
                    Err(mut error) => {
                        error.remap_string_ids(&remap);
                        Err(error)
                    }
                }
            };

        // Prepare file A (entry) — tokenization creates "alpha" and "hello"; header parsing
        // creates the synthetic "#const_template0" name for the const template.
        let output_a = prepare_and_merge("alpha = 1\n#[hello]\n", &canonical_a, 0, 0)
            .expect("file A preparation should succeed");

        // Prepare file B — tokenization creates "beta".
        let output_b = prepare_and_merge(
            "beta #= 2\n",
            &canonical_b,
            output_a.const_template_count,
            output_a.runtime_fragment_count,
        )
        .expect("file B preparation should succeed");

        // The module table should have grown: source strings (alpha, hello, beta) plus
        // header-generated strings (#const_template0 and possibly others).
        assert!(
            frontend.string_table.len() > module_table_size_before + 2,
            "module table should contain source strings plus header-generated strings"
        );

        // Aggregate the remapped outputs.
        let headers = parse_headers(
            vec![output_a, output_b],
            &frontend.external_package_registry,
            &ExternalImportResolutionTable::default(),
            options.project_path_resolver.as_ref(),
            &mut frontend.string_table,
        )
        .expect("header aggregation should succeed");

        // Verify source text string "beta" resolves through the module table in file B headers.
        let beta_header = headers
            .headers
            .iter()
            .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("beta"));
        assert!(
            beta_header.is_some(),
            "beta header should exist with name resolvable through module table"
        );

        // Verify header-generated string "#const_template0" resolves correctly after merge.
        let const_template_header = headers
            .headers
            .iter()
            .find(|h| matches!(h.kind, HeaderKind::ConstTemplate { .. }));
        assert!(
            const_template_header.is_some(),
            "const template header should exist"
        );
        let const_template_name = const_template_header
            .unwrap()
            .tokens
            .src_path
            .name_str(&frontend.string_table)
            .expect("const template should have a name");
        assert_eq!(
            const_template_name, "#const_template0",
            "generated const template name should resolve through module table"
        );

        // Verify token symbols inside the const template also resolve.
        let hello_token = const_template_header
            .unwrap()
            .tokens
            .tokens
            .iter()
            .find_map(|t| match &t.kind {
                TokenKind::Symbol(id) if frontend.string_table.resolve(*id) == "hello" => Some(*id),
                _ => None,
            });
        assert!(
            hello_token.is_some(),
            "hello symbol inside const template should resolve through module table"
        );

        // Verify that beta and the const template have different global IDs, proving
        // non-identity remapping occurred for at least one file's local suffix.
        let beta_id = beta_header
            .unwrap()
            .tokens
            .src_path
            .name()
            .expect("beta should have a name ID");
        let const_template_id = const_template_header
            .unwrap()
            .tokens
            .src_path
            .name()
            .expect("const template should have a name ID");
        assert_ne!(
            beta_id, const_template_id,
            "beta and #const_template0 should have different global IDs after non-identity remapping"
        );
    }

    #[test]
    fn parallel_file_preparation_produces_deterministic_ordered_output() {
        use super::FrontendModuleBuildContext;
        use crate::build_system::build::InputFile;
        use crate::compiler_frontend::CompilerFrontend;
        use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
        use crate::compiler_frontend::headers::parse_file_headers::HeaderKind;
        use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
        use crate::compiler_frontend::symbols::identity::SourceFileTable;
        use crate::compiler_frontend::tokenizer::tokens::TokenKind;
        use crate::projects::settings::Config;
        use std::fs;

        let temp_dir = tempfile::tempdir().expect("should create temp dir");
        let file_a = temp_dir.path().join("a.bst");
        let file_b = temp_dir.path().join("b.bst");
        let file_c = temp_dir.path().join("c.bst");

        // File A is the entry file with a runtime template, a const template, and a declaration.
        fs::write(&file_a, "alpha = 1\n#[hello]\n[runtime]\n").unwrap();
        // File B is a normal source file with an exported constant (PascalCase produces a warning).
        fs::write(&file_b, "Beta #= 2\n").unwrap();
        // File C is a normal source file with another exported constant (PascalCase produces a warning).
        fs::write(&file_c, "Gamma #= 3\n").unwrap();

        let canonical_a = fs::canonicalize(&file_a).unwrap();
        let canonical_b = fs::canonicalize(&file_b).unwrap();
        let canonical_c = fs::canonicalize(&file_c).unwrap();

        let mut string_table = StringTable::new();
        let source_files = SourceFileTable::build(
            &[&canonical_a, &canonical_b, &canonical_c],
            &canonical_a,
            None,
            &mut string_table,
        )
        .expect("source file table should build");

        let module_table_size_before = string_table.len();

        let mut frontend = CompilerFrontend::new(
            &Config::new(temp_dir.path().to_path_buf()),
            string_table,
            StyleDirectiveRegistry::built_ins(),
            ExternalPackageRegistry::new(),
            None,
        );
        frontend.set_source_files(source_files);

        let input_files = vec![
            InputFile {
                source_code: "alpha = 1\n#[hello]\n[runtime]\n".to_owned(),
                source_path: canonical_a.clone(),
            },
            InputFile {
                source_code: "Beta #= 2\n".to_owned(),
                source_path: canonical_b.clone(),
            },
            InputFile {
                source_code: "Gamma #= 3\n".to_owned(),
                source_path: canonical_c.clone(),
            },
        ];

        let (headers, warnings) = FrontendModuleBuildContext::prepare_module_files(
            &mut frontend,
            &input_files,
            &canonical_a,
            &ExternalImportResolutionTable::default(),
        )
        .expect("parallel preparation should succeed");

        // The module table should have grown with strings from all three files plus
        // header-generated strings.
        assert!(
            frontend.string_table.len() > module_table_size_before + 4,
            "module table should contain source strings from all files plus header-generated strings"
        );

        // Verify deterministic header ordering: input order is preserved before aggregation.
        let header_source_names: Vec<_> = headers
            .headers
            .iter()
            .map(|h| {
                h.source_file
                    .to_path_buf(&frontend.string_table)
                    .file_name()
                    .expect("test logical source path should have a file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        let last_a = header_source_names
            .iter()
            .rposition(|name| name == "a.bst")
            .expect("file A headers should be present");
        let first_b = header_source_names
            .iter()
            .position(|name| name == "b.bst")
            .expect("file B headers should be present");
        let first_c = header_source_names
            .iter()
            .position(|name| name == "c.bst")
            .expect("file C headers should be present");
        assert!(
            last_a < first_b && first_b < first_c,
            "prepared headers should preserve input file order, got: {header_source_names:?}"
        );

        // Verify headers from all files exist and strings resolve.
        let beta_header = headers
            .headers
            .iter()
            .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("Beta"));
        assert!(beta_header.is_some(), "Beta header should exist");

        let gamma_header = headers
            .headers
            .iter()
            .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("Gamma"));
        assert!(gamma_header.is_some(), "Gamma header should exist");

        let const_template_header = headers
            .headers
            .iter()
            .find(|h| matches!(h.kind, HeaderKind::ConstTemplate { .. }));
        assert!(
            const_template_header.is_some(),
            "const template header should exist"
        );
        let const_template_name = const_template_header
            .unwrap()
            .tokens
            .src_path
            .name_str(&frontend.string_table)
            .expect("const template should have a name");
        assert_eq!(
            const_template_name, "#const_template0",
            "generated const template name should resolve through module table"
        );

        // Verify token symbols inside the const template resolve.
        let hello_token = const_template_header
            .unwrap()
            .tokens
            .tokens
            .iter()
            .find_map(|t| match &t.kind {
                TokenKind::Symbol(id) if frontend.string_table.resolve(*id) == "hello" => Some(*id),
                _ => None,
            });
        assert!(
            hello_token.is_some(),
            "hello symbol inside const template should resolve through module table"
        );

        // Verify runtime fragment count from entry file.
        assert_eq!(
            headers.entry_runtime_fragment_count, 1,
            "entry file should contribute exactly one runtime fragment"
        );

        // Verify const fragment from entry file.
        assert_eq!(
            headers.top_level_const_fragments.len(),
            1,
            "entry file should contribute exactly one const fragment"
        );

        // Verify warnings from multiple files are preserved deterministically.
        assert_eq!(
            warnings.len(),
            2,
            "expected two naming-convention warnings from Beta and Gamma"
        );
        assert!(
            warnings.iter().all(|w| matches!(
                w.kind,
                crate::compiler_frontend::compiler_messages::DiagnosticKind::Rule(
                    crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
                )
            )),
            "all warnings should be naming convention warnings"
        );

        // Verify non-identity remapping: Beta and the const template should have different
        // global IDs, proving at least one file's local suffix was remapped.
        let beta_id = beta_header
            .unwrap()
            .tokens
            .src_path
            .name()
            .expect("Beta should have a name ID");
        let const_template_id = const_template_header
            .unwrap()
            .tokens
            .src_path
            .name()
            .expect("const template should have a name ID");
        assert_ne!(
            beta_id, const_template_id,
            "Beta and #const_template0 should have different global IDs after non-identity remapping"
        );
    }
}
