//! Per-module frontend compilation pipeline for Beanstalk projects.
//!
//! Drives a single discovered module through the full frontend pipeline:
//! scheduled file preparation (tokenization + header parsing) → dependency sort → AST → HIR →
//! borrow checking.

use crate::build_system::build::{
    CompiledModuleResult, InputFile, Module, ModuleRootActivity, ResolvedConstFragment,
};

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::arena::FrontendArenaCapacityEstimate;
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
use crate::compiler_frontend::symbols::string_interning::{StringTable, StringTableForkSource};
use crate::compiler_frontend::{
    CompilerFrontend, FrontendBuildProfile, FrontendFilePrepareContext, FrontendFilePrepareInput,
};
use crate::libraries::external_import_providers::provider::BuilderRuntimePackageMetadata;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;

#[cfg(feature = "detailed_timers")]
use crate::benchmark_timer_log;
use crate::borrow_log;
use crate::projects::settings::Config;

use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
#[cfg(feature = "detailed_timers")]
use std::time::Instant;

/// Parallel file-preparation scheduling policy.
///
/// WHAT: keeps the production strategy thresholds near the code that applies them.
/// WHY: these values are benchmark policy, not language semantics. `RAYON_NUM_THREADS` remains
/// the external concurrency override; this pass deliberately does not add a custom Rayon pool,
/// unsafe scheduling, or hidden per-build thread control.
///
/// File count at or below which Rayon scheduling is consistently more expensive than useful.
///
/// WHY: benchmark checks showed tiny modules regressing under Rayon, while fanout-style modules
/// and the documentation build still benefit from parallel file preparation. Medium modules stay
/// serial unless their total source size crosses `FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES`.
const FILE_PREPARATION_ALWAYS_SERIAL_FILE_COUNT: usize = 2;

/// File count at which chunked Rayon scheduling is consistently worth the overhead.
///
/// WHY: eight-file fanout is the first stable win from the Phase 1 benchmark set, but running one
/// task per small file over-schedules many tiny-file modules. Chunking starts here.
const FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT: usize = 8;

/// Source-size threshold that lets medium-sized modules use parallel file preparation.
///
/// This is benchmark policy, not a language semantic: 3-7 file modules avoid Rayon overhead by
/// default, but a large enough source payload can amortize scheduling and string-table fork costs.
const FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES: usize = 64 * 1024;

/// Target parallel chunks per Rayon worker for many-file module preparation.
///
/// WHY: a small multiple of the worker count gives Rayon enough tasks to balance uneven source
/// sizes without returning to one scheduling task per tiny file.
const FILE_PREPARATION_TARGET_TASKS_PER_THREAD: usize = 2;

/// Lower bound for chunk size when planning chunked file preparation.
///
/// WHY: chunking only helps if each scheduled task does enough serial file preparation to amortize
/// fork and scheduling overhead.
const FILE_PREPARATION_MIN_CHUNK_SIZE: usize = 4;

struct FilePreparationChunk {
    chunk_index: usize,
    file_range: Range<usize>,
    local_string_table: StringTable,
    results: Vec<PreparedFileResult>,
}

struct PreparedFileResult {
    file_index: usize,
    result: Result<FileFrontendPrepareOutput, FileFrontendPrepareError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilePreparationChunkPlan {
    chunk_index: usize,
    file_range: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilePreparationStrategy {
    Serial,
    ParallelPerFile,
    ParallelChunked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilePreparationStrategyReason {
    SmallSerial,
    ByteThresholdSerial,
    MediumByteThresholdParallel,
    LargeChunkedParallel,
}

impl FilePreparationStrategy {
    #[cfg(test)]
    fn for_module(source_file_count: usize, source_byte_count: usize) -> Self {
        Self::selection_for_module(source_file_count, source_byte_count).0
    }

    fn selection_for_module(
        source_file_count: usize,
        source_byte_count: usize,
    ) -> (Self, FilePreparationStrategyReason) {
        if source_file_count <= FILE_PREPARATION_ALWAYS_SERIAL_FILE_COUNT {
            (Self::Serial, FilePreparationStrategyReason::SmallSerial)
        } else if source_file_count >= FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT {
            (
                Self::ParallelChunked,
                FilePreparationStrategyReason::LargeChunkedParallel,
            )
        } else if source_byte_count >= FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES {
            (
                Self::ParallelPerFile,
                FilePreparationStrategyReason::MediumByteThresholdParallel,
            )
        } else {
            (
                Self::Serial,
                FilePreparationStrategyReason::ByteThresholdSerial,
            )
        }
    }
}

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
    pub(super) external_packages: Arc<ExternalPackageRegistry>,
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
        let source_byte_count = record_module_input_counters(module);

        // Build a human-readable attribution label so the concise timing summary can
        // show the slowest module without flooding output with per-module lines.
        let module_label_text =
            module_timing_label(entry_file_path, module.len(), source_byte_count);
        let module_label: Option<&str> = Some(&module_label_text);

        let external_import_resolution_table = self.external_import_resolution_table;

        let mut compiler = CompilerFrontend::new(
            self.config,
            string_table,
            self.style_directives.to_owned(),
            Arc::clone(&self.external_packages),
            self.project_path_resolver.clone(),
        );

        // Record the total frontend time for this module (success or error).
        let module_total_start = crate::timing::start_pipeline_timing();
        let compile_result = (|| {
            let mut warnings = Vec::new();

            // 1. Map input source files into the compiler's source table.
            Self::attach_source_files(&mut compiler, module, entry_file_path)?;

            // 2. Prepare all files: tokenize and parse headers in one local string-table
            //    per file, then merge/remap once before aggregation.
            let (module_headers, file_warnings) = timed_frontend_stage(
                "frontend.file_prepare",
                "Files Prepared in: ",
                module_label,
                || {
                    Self::prepare_module_files(
                        &mut compiler,
                        module,
                        entry_file_path,
                        external_import_resolution_table,
                        source_byte_count,
                    )
                },
            )?;
            warnings.extend(file_warnings);

            let capacity_estimate =
                record_frontend_capacity_estimate(module.len(), source_byte_count, &module_headers);

            // 3. Resolve dependencies and sort headers for linear processing.
            let sorted = timed_frontend_stage(
                "frontend.dependency_sort",
                "Dependency graph created in: ",
                module_label,
                || Self::sort_headers(&mut compiler, module_headers, &warnings),
            )?;

            let root_activity = ModuleRootActivity {
                has_non_trivial_root_body: sorted.has_non_trivial_root_body,
                const_fragment_count: sorted.const_fragment_count,
                runtime_fragment_count: sorted.entry_runtime_fragment_count,
            };

            // 4. Build the Abstract Syntax Tree (AST).
            let module_ast =
                timed_frontend_stage("frontend.ast", "AST created in: ", module_label, || {
                    self.build_ast(
                        &mut compiler,
                        sorted,
                        entry_file_path,
                        capacity_estimate,
                        &mut warnings,
                    )
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
                timed_frontend_stage("frontend.hir", "HIR generated in: ", module_label, || {
                    Self::lower_hir(&mut compiler, module_ast, &warnings)
                })?;

            // 7. Run static analysis (Borrow Checker).
            let borrow_analysis = timed_frontend_stage(
                "frontend.borrow",
                "Borrow checking completed in: ",
                module_label,
                || Self::check_borrows(&compiler, &hir_module, &warnings),
            )?;
            record_borrow_counters(&borrow_analysis);

            // Runtime import metadata is tied to calls that can execute from entry `start`.
            // The registry and provider table stay fully populated for type checking and
            // diagnostics; only the backend-facing module metadata is reachability-filtered.
            let reachability = collect_reachability_from_start(&hir_module)
                .map_err(|error| CompilerMessages::from_error_ref(error, &compiler.string_table))?;
            let reachable_external_package_ids = collect_reachable_external_package_ids(
                &reachability.reachable_external_functions,
                compiler.external_package_registry.as_ref(),
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
                root_activity,
                external_package_registry: Arc::clone(&compiler.external_package_registry),
                module_external_imports,
            })
        })();

        crate::timing::record_started_pipeline_timing_with_label(
            "frontend.module.total",
            module_total_start,
            module_label,
        );

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

    /// Prepare all source files in the module by tokenizing and header-parsing each one against a
    /// local string-table fork, then merging and remapping in deterministic input order.
    ///
    /// WHAT: small modules use a serial fast path, while large modules use Rayon. Both paths
    ///       produce the same per-file result records and share the same merge/remap aggregation.
    /// WHY: keeping scheduling separate from aggregation avoids Rayon overhead on tiny modules
    ///      without changing deterministic merge order or frontend ownership boundaries.
    fn prepare_module_files(
        compiler: &mut CompilerFrontend,
        module: &[InputFile],
        entry_file_path: &Path,
        external_import_resolution_table: &ExternalImportResolutionTable,
        source_byte_count: usize,
    ) -> Result<(Headers, Vec<CompilerDiagnostic>), CompilerMessages> {
        let entry_file_id = compiler
            .source_files
            .get_by_canonical_path(entry_file_path)
            .map(|identity| identity.file_id);

        let options = HeaderParseOptions {
            entry_file_id,
            project_path_resolver: compiler.project_path_resolver.clone(),
        };

        // Create one shared fork source for all file-preparation workers. Each scheduled chunk
        // gets a local table forked from this immutable base, so preparation never needs mutable
        // access to the module string table during tokenization or header parsing.
        let fork_source = compiler.string_table.fork_source();
        let base_len = fork_source.base_len();

        // Offsets are only relevant for the active module root, and there is exactly one root per
        // module. Imported and ordinary files produce zero const templates and runtime fragments, so
        // every file can safely start from offset zero without name collisions.
        let const_template_offset = 0usize;
        let runtime_fragment_offset = 0usize;

        let prepare_context = FrontendFilePrepareContext {
            source_files: &compiler.source_files,
            style_directives: &compiler.style_directives,
            external_package_registry: compiler.external_package_registry.as_ref(),
            entry_file_path,
            options: &options,
        };

        add_frontend_counter(FrontendCounter::FilePreparationInputFileCount, module.len());
        add_frontend_counter(
            FrontendCounter::FilePreparationInputByteCount,
            source_byte_count,
        );

        let (strategy, strategy_reason) = timed_frontend_substep(
            "file_prepare_strategy_selection_ms",
            "File preparation strategy selected in: ",
            || FilePreparationStrategy::selection_for_module(module.len(), source_byte_count),
        );
        record_file_preparation_strategy(strategy, strategy_reason);

        let preparation_chunks = timed_frontend_substep(
            "file_prepare_result_production_ms",
            "File preparation results produced in: ",
            || {
                Self::prepare_module_file_chunks(
                    module,
                    &fork_source,
                    &prepare_context,
                    const_template_offset,
                    runtime_fragment_offset,
                    strategy,
                )
            },
        );

        Self::merge_file_preparation_chunks(
            compiler,
            preparation_chunks,
            base_len,
            external_import_resolution_table,
            &options,
        )
    }

    /// Merge chunk-local string tables and aggregate prepared file outputs.
    ///
    /// WHAT: all scheduling strategies converge here after producing ordered chunk records.
    /// WHY: chunk-local workers may finish in any order, but the frontend's source identity,
    /// warning, diagnostic, and header order must follow the original module input order.
    fn merge_file_preparation_chunks(
        compiler: &mut CompilerFrontend,
        mut preparation_chunks: Vec<FilePreparationChunk>,
        base_len: usize,
        external_import_resolution_table: &ExternalImportResolutionTable,
        options: &HeaderParseOptions,
    ) -> Result<(Headers, Vec<CompilerDiagnostic>), CompilerMessages> {
        // Completion order is a scheduler detail. Merge order is the module input order encoded
        // by deterministic chunk indexes.
        timed_frontend_substep(
            "file_prepare_result_sort_ms",
            "File preparation results sorted in: ",
            || preparation_chunks.sort_by_key(|chunk| chunk.chunk_index),
        );

        let mut prepared_outputs = Vec::new();
        let mut warnings = Vec::new();
        let mut diagnostics = Vec::new();
        let mut const_fragment_source_count = 0usize;
        let mut runtime_fragment_source_count = 0usize;

        let prepared_file_capacity = preparation_chunks
            .iter()
            .map(|chunk| chunk.results.len())
            .sum();
        prepared_outputs.reserve(prepared_file_capacity);

        let mut expected_file_index = 0usize;
        for chunk in preparation_chunks {
            debug_assert_eq!(
                chunk.file_range.start, expected_file_index,
                "file preparation chunks must be merged in original source-file order"
            );
            expected_file_index = chunk.file_range.end;

            let remap = timed_frontend_substep(
                "file_prepare_string_table_delta_merge_ms",
                "File preparation string-table delta merged in: ",
                || {
                    compiler
                        .string_table
                        .merge_delta_from(&chunk.local_string_table, base_len)
                },
            );
            let remap_is_identity = remap.is_identity();
            add_frontend_counter(FrontendCounter::FilePreparationResultMergeCount, 1);
            if remap_is_identity {
                add_frontend_counter(FrontendCounter::FilePreparationIdentityRemapCount, 1);
            } else {
                add_frontend_counter(FrontendCounter::FilePreparationNonIdentityRemapCount, 1);
            }

            for (expected_chunk_file_index, prepared_file) in
                (chunk.file_range.start..).zip(chunk.results)
            {
                debug_assert_eq!(
                    prepared_file.file_index, expected_chunk_file_index,
                    "prepared file records must stay ordered inside each chunk"
                );

                match prepared_file.result {
                    Ok(mut output) => {
                        if output.const_template_count > 0 {
                            const_fragment_source_count += 1;
                        }
                        if output.runtime_fragment_count > 0 {
                            runtime_fragment_source_count += 1;
                        }
                        if !remap_is_identity {
                            add_frontend_counter(FrontendCounter::FilePrepareOutputRemapCalls, 1);
                            #[cfg(feature = "benchmark_counters")]
                            add_frontend_counter(
                                FrontendCounter::FilePrepareNonIdentityPayloadRemaps,
                                1,
                            );
                            timed_frontend_substep(
                                "file_prepare_payload_remap_ms",
                                "File preparation payload remapped in: ",
                                || output.remap_string_ids(&remap),
                            );
                        }
                        warnings.append(&mut output.warnings);
                        prepared_outputs.push(output);
                    }
                    Err(mut error) => {
                        if !remap_is_identity {
                            add_frontend_counter(FrontendCounter::FilePrepareErrorRemapCalls, 1);
                            #[cfg(feature = "benchmark_counters")]
                            add_frontend_counter(
                                FrontendCounter::FilePrepareNonIdentityPayloadRemaps,
                                1,
                            );
                            timed_frontend_substep(
                                "file_prepare_payload_remap_ms",
                                "File preparation payload remapped in: ",
                                || error.remap_string_ids(&remap),
                            );
                        }
                        warnings.extend(error.warnings);
                        diagnostics.push(*error.diagnostic);
                    }
                }
            }
        }

        debug_assert!(
            const_fragment_source_count <= 1,
            "only the active module root may contribute top-level const templates"
        );
        debug_assert!(
            runtime_fragment_source_count <= 1,
            "only the active module root may contribute runtime fragments"
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
        let headers = timed_frontend_substep(
            "file_prepare_parse_headers_aggregation_ms",
            "File preparation headers aggregated in: ",
            || {
                parse_headers(
                    prepared_outputs,
                    compiler.external_package_registry.as_ref(),
                    external_import_resolution_table,
                    options.project_path_resolver.as_ref(),
                    &mut compiler.string_table,
                )
            },
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

    fn prepare_module_file_chunks(
        module: &[InputFile],
        fork_source: &StringTableForkSource,
        prepare_context: &FrontendFilePrepareContext<'_>,
        const_template_offset: usize,
        runtime_fragment_offset: usize,
        strategy: FilePreparationStrategy,
    ) -> Vec<FilePreparationChunk> {
        match strategy {
            FilePreparationStrategy::Serial => {
                let plan = FilePreparationChunkPlan {
                    chunk_index: 0,
                    file_range: 0..module.len(),
                };
                vec![Self::prepare_module_file_chunk(
                    plan,
                    module,
                    fork_source,
                    prepare_context,
                    const_template_offset,
                    runtime_fragment_offset,
                )]
            }

            FilePreparationStrategy::ParallelPerFile => (0..module.len())
                .into_par_iter()
                .map(|index| {
                    let plan = FilePreparationChunkPlan {
                        chunk_index: index,
                        file_range: index..index + 1,
                    };
                    Self::prepare_module_file_chunk(
                        plan,
                        module,
                        fork_source,
                        prepare_context,
                        const_template_offset,
                        runtime_fragment_offset,
                    )
                })
                .collect(),

            FilePreparationStrategy::ParallelChunked => {
                let plans =
                    plan_file_preparation_chunks(module.len(), rayon::current_num_threads());
                plans
                    .into_par_iter()
                    .map(|plan| {
                        Self::prepare_module_file_chunk(
                            plan,
                            module,
                            fork_source,
                            prepare_context,
                            const_template_offset,
                            runtime_fragment_offset,
                        )
                    })
                    .collect()
            }
        }
    }

    fn prepare_module_file_chunk(
        plan: FilePreparationChunkPlan,
        module: &[InputFile],
        fork_source: &StringTableForkSource,
        prepare_context: &FrontendFilePrepareContext<'_>,
        const_template_offset: usize,
        runtime_fragment_offset: usize,
    ) -> FilePreparationChunk {
        let (mut local_string_table, _) = fork_source.fork_for_module().into_parts();
        let mut results = Vec::with_capacity(plan.file_range.len());

        for file_index in plan.file_range.clone() {
            let file = &module[file_index];
            let input = FrontendFilePrepareInput {
                source_code: &file.source_code,
                source_path: &file.source_path,
                source_kind: file.source_kind,
                const_template_offset,
                runtime_fragment_offset,
            };
            let result = CompilerFrontend::prepare_file_frontend_local(
                prepare_context,
                input,
                &mut local_string_table,
            );
            results.push(PreparedFileResult { file_index, result });
        }

        FilePreparationChunk {
            chunk_index: plan.chunk_index,
            file_range: plan.file_range,
            local_string_table,
            results,
        }
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
        capacity_estimate: FrontendArenaCapacityEstimate,
        warnings: &mut Vec<CompilerDiagnostic>,
    ) -> Result<Ast, CompilerMessages> {
        match compiler.headers_to_ast(
            sorted,
            entry_file_path,
            self.build_profile,
            capacity_estimate,
        ) {
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

fn plan_file_preparation_chunks(
    source_file_count: usize,
    worker_thread_count: usize,
) -> Vec<FilePreparationChunkPlan> {
    if source_file_count == 0 {
        return Vec::new();
    }

    let worker_thread_count = worker_thread_count.max(1);
    let target_chunk_count =
        worker_thread_count.saturating_mul(FILE_PREPARATION_TARGET_TASKS_PER_THREAD);
    let max_chunk_count_by_size = (source_file_count / FILE_PREPARATION_MIN_CHUNK_SIZE).max(1);
    let chunk_count = target_chunk_count
        .min(max_chunk_count_by_size)
        .min(source_file_count)
        .max(1);

    let base_chunk_size = source_file_count / chunk_count;
    let larger_chunk_count = source_file_count % chunk_count;

    let mut plans = Vec::with_capacity(chunk_count);
    let mut start_file_index = 0usize;
    for chunk_index in 0..chunk_count {
        let chunk_size = base_chunk_size + usize::from(chunk_index < larger_chunk_count);
        let end_file_index = start_file_index + chunk_size;
        plans.push(FilePreparationChunkPlan {
            chunk_index,
            file_range: start_file_index..end_file_index,
        });
        start_file_index = end_file_index;
    }

    plans
}

fn record_module_input_counters(module: &[InputFile]) -> usize {
    add_frontend_counter(FrontendCounter::ModuleCount, 1);
    add_frontend_counter(FrontendCounter::SourceFileCount, module.len());

    let source_byte_count = module
        .iter()
        .map(|input_file| input_file.source_code.len())
        .sum();
    add_frontend_counter(FrontendCounter::SourceByteCount, source_byte_count);
    source_byte_count
}

fn record_file_preparation_strategy(
    strategy: FilePreparationStrategy,
    reason: FilePreparationStrategyReason,
) {
    match strategy {
        FilePreparationStrategy::Serial => {
            add_frontend_counter(FrontendCounter::FilePreparationSerialModuleCount, 1);
        }

        FilePreparationStrategy::ParallelPerFile | FilePreparationStrategy::ParallelChunked => {
            add_frontend_counter(FrontendCounter::FilePreparationParallelModuleCount, 1);
        }
    }

    match reason {
        FilePreparationStrategyReason::SmallSerial => {
            add_frontend_counter(FrontendCounter::FilePreparationStrategySmallSerialCount, 1);
        }

        FilePreparationStrategyReason::ByteThresholdSerial => {
            add_frontend_counter(
                FrontendCounter::FilePreparationStrategyByteThresholdSerialCount,
                1,
            );
        }

        FilePreparationStrategyReason::MediumByteThresholdParallel => {
            add_frontend_counter(
                FrontendCounter::FilePreparationStrategyParallelPerFileCount,
                1,
            );
            add_frontend_counter(FrontendCounter::FilePreparationStrategyParallelCount, 1);
        }

        FilePreparationStrategyReason::LargeChunkedParallel => {
            add_frontend_counter(FrontendCounter::FilePreparationStrategyChunkedCount, 1);
            add_frontend_counter(FrontendCounter::FilePreparationStrategyParallelCount, 1);
        }
    }
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

fn record_frontend_capacity_estimate(
    source_file_count: usize,
    source_byte_count: usize,
    headers: &Headers,
) -> FrontendArenaCapacityEstimate {
    let const_fragment_count = headers.const_fragment_count;
    let capacity = FrontendArenaCapacityEstimate::new(
        source_file_count,
        source_byte_count,
        headers.token_stats,
        headers.header_stats,
        const_fragment_count,
        headers.entry_runtime_fragment_count,
    );

    // Phase 1 wires the scope-frame estimate because scope-frame arenas are the first typed arena
    // target. Phase 4 records actual frame allocation and arena capacity growth from the scope
    // arena owner; this site records only the policy estimate.
    add_frontend_counter(FrontendCounter::EstimatedScopeFrames, capacity.scope_frames);
    add_frontend_counter(
        FrontendCounter::CappedCapacityEstimates,
        capacity.capped_field_count,
    );

    capacity
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
        FrontendCounter::BorrowStatementVisitCount,
        report.stats.statements_analyzed,
    );
    add_frontend_counter(
        FrontendCounter::BorrowTerminatorVisitCount,
        report.stats.terminators_analyzed,
    );
    add_frontend_counter(
        FrontendCounter::BorrowWorklistIterationCount,
        report.stats.worklist_iterations,
    );
    add_frontend_counter(
        FrontendCounter::BorrowStateJoinCount,
        report.stats.state_joins,
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

/// Format a bounded, human-readable attribution label for one module's timing.
///
/// WHAT: combines the entry path, source file count, and source byte count into a
///      short label suitable for the concise timing summary's "slowest module" line.
/// WHY:  the label stays out of stable `BST_BENCH timing` lines; it is display-only
///       evidence so the summary can attribute the max sample without per-module listing.
fn module_timing_label(
    entry_file_path: &Path,
    source_file_count: usize,
    source_byte_count: usize,
) -> String {
    let file_word = if source_file_count == 1 {
        "file"
    } else {
        "files"
    };
    format!(
        "{} ({} {}, {:.1}KB)",
        entry_file_path.display(),
        source_file_count,
        file_word,
        source_byte_count as f64 / 1024.0,
    )
}

/// Record one frontend pipeline stage through the central `timers` substrate.
///
/// WHAT: wraps a `Result`-returning stage so its duration is always recorded,
///      regardless of success or error, with an optional module attribution label.
/// WHY:  Phase 4 migrates public frontend stage timings from the `detailed_timers`
///       macro to the central `timers` collector so concise `timers`-only builds
///       see project-level aggregates.  Human prose stays gated by `detailed_timers`
///       for verbose developer output; the xtask legacy parser reads that prose to
///       attribute legacy metric names until Phase 6 updates xtask ratio definitions.
fn timed_frontend_stage<T>(
    metric: &str,
    prose_label: &str,
    module_label: Option<&str>,
    stage: impl FnOnce() -> Result<T, CompilerMessages>,
) -> Result<T, CompilerMessages> {
    let start = crate::timing::start_pipeline_timing();
    let result = stage();
    crate::timing::record_started_pipeline_timing_with_label(metric, start, module_label);

    // Human prose stays gated by detailed_timers for verbose developer output.
    #[cfg(feature = "detailed_timers")]
    {
        if crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled() {
            saying::say!(prose_label, Green #start.elapsed());
        }
    }

    // Keep parameters visibly used when detailed_timers is off.
    let _ = prose_label;

    result
}

#[cfg(feature = "detailed_timers")]
fn timed_frontend_substep<T>(metric_name: &str, label: &str, substep: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = substep();
    benchmark_timer_log!(start, metric_name, label);
    result
}

#[cfg(not(feature = "detailed_timers"))]
fn timed_frontend_substep<T>(_metric_name: &str, _label: &str, substep: impl FnOnce() -> T) -> T {
    substep()
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
