//! BFS traversal over Beanstalk import graphs to find all reachable source files.
//!
//! Given an entry `.bst` file, walks its import declarations transitively to build the complete
//! set of source files that belong to a module. Also assembles `InputFile` payloads from those
//! paths for downstream compilation stages.
// Stage 0 deliberately returns full diagnostic/infrastructure payloads in `SourceDiscoveryError`
// so import discovery does not erase source locations or downgrade filesystem failures.

use crate::build_system::build::InputFile;

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::instrumentation::{FrontendCounter, add_frontend_counter};
use crate::compiler_frontend::paths::path_normalization::join_and_normalize_path;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::source_libraries::root_file::MOD_FILE_NAME;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::SourceFileKind;
use crate::libraries::external_import_providers::cache::ExternalImportCacheKey;
use crate::libraries::external_import_providers::cache::ExternalImportProviderCache;
use crate::libraries::external_import_providers::provider::{
    ExternalImportProvider, ExternalImportProviderContext, ExternalImportRequest,
};
use crate::libraries::external_import_providers::registry::ExternalImportProviderRegistry;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::import_scanning::{
    ScannedImportSource, scan_imports_from_source, scan_imports_with_source,
};
use super::source_discovery_error::SourceDiscoveryError;
use super::source_loading::{extract_source_code, read_source_code, source_read_error};

/// Record a reachable-discovery stage timing through the central `timers` substrate.
///
/// WHAT: delegates to `timing::record_started_pipeline_timing`, which stores the
///      observation in the active collection scope and emits the stable
///      `BST_BENCH timing` line when the output mode permits.
/// WHY:  reachable-file discovery uses dotted `stage0.reachable_discovery.*` metric
///      names. The start token is zero-sized when `timers` is off, so regular builds
///      do not read clocks for instrumentation-only measurements.
fn log_stage_timing(metric: &str, start: crate::timing::PipelineTimingStart) {
    crate::timing::record_started_pipeline_timing(metric, start);
}

/// Minimum cache-miss count before Stage 0 uses Rayon for raw source loading.
///
/// The threshold keeps tiny projects and mostly-cached modules on the cheaper serial path while
/// still letting markdown-heavy modules overlap independent filesystem reads.
pub(super) const STAGE0_PARALLEL_SOURCE_LOAD_MIN_FILES: usize = 8;

/// Mutable external-import state shared across Stage 0 reachable-file discovery.
///
/// WHAT: groups provider metadata, the external package registry, and build-scoped provider
/// cache/table state.
/// WHY: Stage 0 needs to mutate provider results while walking imports, but callers should not
/// thread four closely related provider arguments through every discovery function.
pub(crate) struct ExternalImportDiscoveryState<'a> {
    pub(super) external_packages: &'a mut ExternalPackageRegistry,
    pub(super) providers: &'a ExternalImportProviderRegistry,
    pub(super) cache: &'a mut ExternalImportProviderCache,
    pub(super) resolution_table: &'a mut ExternalImportResolutionTable,
}

/// A reachable source file plus the source kind selected by import resolution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ReachableSourceFile {
    pub(super) path: PathBuf,
    pub(super) kind: SourceFileKind,
}

/// Stage 0 inventory produced by reachable-file discovery.
///
/// WHAT: owns the deterministic input-file list plus source text already read while scanning
///       Beanstalk imports.
/// WHY: source loading policy belongs to Stage 0. Header parsing and later frontend stages should
///      continue to receive plain `InputFile` values without knowing whether text came from the
///      import-scan cache or a later raw file read.
pub(super) struct ReachableSourceInventory {
    pub(super) files: Vec<ReachableSourceFile>,
    source_cache: FxHashMap<PathBuf, String>,
}

/// Source text proven provider-free during the serial classification pass.
///
/// WHAT: keeps the Beanstalk source read while proving whether provider-free parallel discovery is
///       safe.
/// WHY: the provider-free workers still need to tokenize imports per entry module, but they should
///      not undo Phase 3's source-reuse work by re-reading every classified `.bst` file.
pub(super) struct ProviderFreeProjectInventory {
    pub(super) source_cache: FxHashMap<PathBuf, String>,
}

struct MissingSourceFile {
    input_index: usize,
    source_file: ReachableSourceFile,
}

struct LoadedMissingSourceFile {
    input_index: usize,
    source_file: ReachableSourceFile,
    source_code: String,
}

struct SourceReadFailure {
    input_index: usize,
    path: PathBuf,
    error: std::io::Error,
}

// -------------------------
//  Public API
// -------------------------

/// Collect all reachable source files for a given entry point and load their content.
pub(super) fn collect_reachable_input_files(
    entry_path: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<InputFile>, CompilerMessages> {
    let total_start = crate::timing::start_pipeline_timing();

    // 1. Traverse the import graph to find all paths.
    let inventory = match discover_reachable_source_files(
        entry_path,
        project_path_resolver,
        style_directives,
        external_imports,
        string_table,
    ) {
        Ok(files) => files,
        Err(error) => {
            log_stage_timing("stage0.reachable_discovery.total", total_start);
            return Err(error.into_messages(string_table));
        }
    };

    let result = assemble_input_files_from_inventory(inventory, string_table);
    log_stage_timing("stage0.reachable_discovery.total", total_start);
    result
}

/// Assemble `InputFile` values from a deterministic Stage 0 inventory.
///
/// WHAT: reuses source text cached during import scanning and loads remaining files through the
///       serial/parallel cache-miss path.
/// WHY: inventory assembly is the same whether discovery was provider-capable or provider-free,
///      so it is shared between both paths to keep ordering and loading policy in one place.
pub(super) fn assemble_input_files_from_inventory(
    inventory: ReachableSourceInventory,
    string_table: &mut StringTable,
) -> Result<Vec<InputFile>, CompilerMessages> {
    let input_file_count = inventory.files.len();
    let mut input_slots: Vec<Option<InputFile>> = (0..input_file_count).map(|_| None).collect();
    let mut missing_sources = Vec::new();
    let mut source_cache = inventory.source_cache;

    for (input_index, source_file) in inventory.files.into_iter().enumerate() {
        if let Some(source_code) = source_cache.remove(&source_file.path) {
            add_frontend_counter(FrontendCounter::Stage0SourceCacheHitCount, 1);

            input_slots[input_index] = Some(InputFile {
                source_code,
                source_path: source_file.path,
                source_kind: source_file.kind,
            });
        } else {
            add_frontend_counter(FrontendCounter::Stage0SourceCacheMissCount, 1);

            missing_sources.push(MissingSourceFile {
                input_index,
                source_file,
            });
        }
    }

    let source_load_start = crate::timing::start_pipeline_timing();
    let loaded_missing_sources = match load_missing_sources(missing_sources, string_table) {
        Ok(loaded_missing_sources) => loaded_missing_sources,
        Err(messages) => {
            log_stage_timing("stage0.reachable_discovery.source_load", source_load_start);
            return Err(messages);
        }
    };
    log_stage_timing("stage0.reachable_discovery.source_load", source_load_start);
    for loaded in loaded_missing_sources {
        add_frontend_counter(
            FrontendCounter::Stage0SourceBytesLoaded,
            loaded.source_code.len(),
        );

        input_slots[loaded.input_index] = Some(InputFile {
            source_code: loaded.source_code,
            source_path: loaded.source_file.path,
            source_kind: loaded.source_file.kind,
        });
    }

    // All slots are either cache hits or successful miss loads. Keep the join explicit so input
    // order stays tied to the deterministic inventory order even when miss loading used Rayon.
    let mut input_files = Vec::with_capacity(input_file_count);
    for slot in input_slots {
        let Some(input_file) = slot else {
            let error = CompilerError::compiler_error(
                "Stage 0 source inventory slot was empty after successful loading",
            );
            return Err(CompilerMessages::from_error_ref(error, string_table));
        };

        input_files.push(input_file);
    }

    Ok(input_files)
}

// -------------------------
//  Reachable Discovery
// -------------------------

/// Origin of source text used during shared reachable-file traversal.
///
/// WHAT: distinguishes Beanstalk source read from disk from source reused from the provider-free
///       classification cache.
/// WHY: Stage 0 counters should only count bytes loaded during the current traversal; reused text
///      was already counted when it was first read.
#[derive(Debug, PartialEq)]
enum SourceScanOrigin {
    FreshRead,
    ReusedFromCache,
}

/// Action a traversal policy wants the shared BFS to take for one import path.
enum ImportPolicyAction {
    /// Do not follow this import.
    Skip,
    /// Resolve and queue the import as a normal local Beanstalk import.
    QueueLocal,
    /// Stop the whole traversal and report that the project is not provider-free.
    AbortClassification,
}

/// Stage 0 import policy that customizes the shared reachable-file traversal.
///
/// WHAT: the three discovery paths (provider-capable serial, provider-free classification,
///       provider-free worker) differ only in how they react to external package imports and
///       whether they can reuse classified source text. This enum keeps those differences explicit
///       while letting the shared BFS own queue handling, canonicalization, and local queuing.
enum ImportPolicy<'a, 'b> {
    /// Full provider-capable path. Mutates provider cache and resolution tables.
    Capable(&'a mut ExternalImportDiscoveryState<'b>),
    /// Conservative pre-scan that proves a directory build has no provider-backed imports.
    FreeClassification(&'a ExternalPackageRegistry),
    /// Provider-free worker path that reuses source text proved safe by classification.
    FreeWorker {
        external_packages: &'a ExternalPackageRegistry,
        project_source_cache: &'a FxHashMap<PathBuf, String>,
    },
}

impl<'a, 'b> ImportPolicy<'a, 'b> {
    /// Scan imports for the current Beanstalk file, reusing classified source text when available.
    fn scan_imports(
        &self,
        canonical_file: &Path,
        style_directives: &StyleDirectiveRegistry,
        string_table: &mut StringTable,
    ) -> Result<(ScannedImportSource, SourceScanOrigin), SourceDiscoveryError> {
        match self {
            ImportPolicy::FreeWorker {
                project_source_cache,
                ..
            } => {
                if let Some(source_code) = project_source_cache.get(canonical_file) {
                    let scanned_source = scan_imports_from_source(
                        canonical_file,
                        source_code.to_owned(),
                        style_directives,
                        string_table,
                    )?;
                    return Ok((scanned_source, SourceScanOrigin::ReusedFromCache));
                }

                let scanned_source =
                    scan_imports_with_source(canonical_file, style_directives, string_table)?;
                Ok((scanned_source, SourceScanOrigin::FreshRead))
            }
            _ => {
                let scanned_source =
                    scan_imports_with_source(canonical_file, style_directives, string_table)?;
                Ok((scanned_source, SourceScanOrigin::FreshRead))
            }
        }
    }

    /// Decide how to handle one import path.
    fn handle_import(
        &mut self,
        import_path: &InternedPath,
        canonical_file: &Path,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<ImportPolicyAction, SourceDiscoveryError> {
        match self {
            ImportPolicy::Capable(state) => handle_provider_capable_import(
                import_path,
                canonical_file,
                project_path_resolver,
                state,
                string_table,
            ),
            ImportPolicy::FreeClassification(external_packages) => {
                handle_provider_free_classification_import(
                    import_path,
                    canonical_file,
                    external_packages,
                    string_table,
                )
            }
            ImportPolicy::FreeWorker {
                external_packages, ..
            } => handle_provider_free_worker_import(
                import_path,
                canonical_file,
                external_packages,
                string_table,
            ),
        }
    }
}

/// Shared BFS over import declarations with a policy-controlled import handler.
///
/// WHAT: follows each Beanstalk file's declared imports, resolves them to canonical typed source
///       files, and returns the full ordered set of files reachable from the entry points.
/// WHY: queue seeding, canonicalization, visited-set handling, facade queuing, Markdown skipping,
///      import scanning, source-cache insertion, and local import queueing are identical across all
///      Stage 0 discovery paths. Keeping them in one place prevents the provider-capable and
///      provider-free paths from drifting.
fn traverse_reachable_source_files(
    entry_paths: &[PathBuf],
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    policy: &mut ImportPolicy<'_, '_>,
    string_table: &mut StringTable,
) -> Result<Option<ReachableSourceInventory>, SourceDiscoveryError> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut source_cache = FxHashMap::default();
    let mut imports_scanned: usize = 0;

    // Seed with entry points in deterministic order.
    for entry_path in entry_paths {
        queue.push_back(ReachableSourceFile {
            path: entry_path.clone(),
            kind: SourceFileKind::Beanstalk,
        });
    }

    // Seed all source library facade files so authored facade declarations are available.
    // WHY: imports may directly resolve to a target file after Stage 0 path scanning, but the
    // facade still needs to be compiled so its public declaration surface can be checked later.
    for facade_path in project_path_resolver.facade_files().values() {
        queue.push_back(ReachableSourceFile {
            path: facade_path.clone(),
            kind: SourceFileKind::Beanstalk,
        });
    }

    while let Some(next_file) = queue.pop_front() {
        let canonical_file = fs::canonicalize(&next_file.path).map_err(|error| {
            CompilerError::file_error(
                &next_file.path,
                format!("Failed to canonicalize module file path: {error}"),
                string_table,
            )
        })?;
        let reachable_file = ReachableSourceFile {
            path: canonical_file.clone(),
            kind: next_file.kind,
        };

        if !reachable.insert(reachable_file.clone()) {
            continue;
        }

        match next_file.kind {
            SourceFileKind::Beandown => {
                // Beandown is a Beanstalk template body with a small compile-time scope, so the
                // same-directory facade may supply visible constants. Plain Markdown is raw
                // content and has no Beanstalk scope; facades still re-export it normally because
                // the facade file itself is scanned as ordinary Beanstalk source.
                queue_same_directory_facade_for_beandown(&canonical_file, &reachable, &mut queue);
                continue;
            }
            SourceFileKind::PlainMarkdown => {
                // Markdown files are importless content assets. They are carried forward for
                // header-stage preparation but are never scanned for imports.
                continue;
            }
            SourceFileKind::Beanstalk => {}
        }

        let import_scan_start = crate::timing::start_pipeline_timing();
        let (scanned_source, scan_origin) =
            match policy.scan_imports(&canonical_file, style_directives, string_table) {
                Ok(scanned_source) => scanned_source,
                Err(error) => {
                    log_stage_timing("stage0.reachable_discovery.import_scan", import_scan_start);
                    return Err(error);
                }
            };
        log_stage_timing("stage0.reachable_discovery.import_scan", import_scan_start);

        if scan_origin == SourceScanOrigin::FreshRead {
            add_frontend_counter(
                FrontendCounter::Stage0SourceBytesLoaded,
                scanned_source.source_code.len(),
            );
        }

        let import_paths = scanned_source.import_paths;
        imports_scanned += import_paths.len();
        source_cache.insert(canonical_file.clone(), scanned_source.source_code);

        for import_path in &import_paths {
            let action = policy.handle_import(
                import_path,
                &canonical_file,
                project_path_resolver,
                string_table,
            )?;

            match action {
                ImportPolicyAction::Skip => continue,
                ImportPolicyAction::QueueLocal => {
                    let import_resolve_start = crate::timing::start_pipeline_timing();
                    let result = resolve_and_queue_local_import(
                        import_path,
                        &canonical_file,
                        project_path_resolver,
                        string_table,
                        &reachable,
                        &mut queue,
                    );
                    log_stage_timing(
                        "stage0.reachable_discovery.import_resolve",
                        import_resolve_start,
                    );
                    result?;
                }
                ImportPolicyAction::AbortClassification => return Ok(None),
            }
        }
    }

    // Record concise counters for the completed traversal. Counters are only
    // recorded when `benchmark_counters` is active, and reach stdout only when
    // `BST_COUNTERS` requests it (summary/full).
    crate::timing::record_counter(
        "stage0.reachable_discovery.reachable_files",
        reachable.len() as f64,
    );
    crate::timing::record_counter(
        "stage0.reachable_discovery.imports_scanned",
        imports_scanned as f64,
    );

    Ok(Some(ReachableSourceInventory {
        files: reachable.into_iter().collect(),
        source_cache,
    }))
}

/// BFS over import declarations starting from `entry_point`, preserving source kind.
///
/// WHAT: follows each Beanstalk file's declared imports, resolves them to canonical typed source
/// files, and returns the full ordered set of files reachable from the entry point.
/// WHY: source kind belongs to Stage 0 input discovery. Builder-supported content assets can be
///      loaded and carried forward without being treated as Beanstalk module roots.
pub(super) fn discover_reachable_source_files(
    entry_point: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<ReachableSourceInventory, SourceDiscoveryError> {
    let mut policy = ImportPolicy::Capable(external_imports);
    let inventory = traverse_reachable_source_files(
        &[entry_point.to_path_buf()],
        project_path_resolver,
        style_directives,
        &mut policy,
        string_table,
    )?;

    inventory.ok_or_else(|| {
        // Provider-capable traversal never aborts classification, so this is unreachable.
        SourceDiscoveryError::from(CompilerError::compiler_error(
            "Provider-capable reachable-file traversal aborted unexpectedly",
        ))
    })
}

/// Resolve a normal Beanstalk import to a filesystem path and enqueue reachable files.
///
/// WHAT: handles cross-module facade queuing and implementation-file discovery for an import that
///       is not provider-backed and not a virtual/unsupported package import.
/// WHY: this logic is identical between the provider-capable and provider-free discovery paths;
///      extracting it prevents the two BFS implementations from drifting.
fn resolve_and_queue_local_import(
    import_path: &InternedPath,
    canonical_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
    reachable: &BTreeSet<ReachableSourceFile>,
    queue: &mut VecDeque<ReachableSourceFile>,
) -> Result<(), SourceDiscoveryError> {
    let resolved = project_path_resolver
        .resolve_import_to_source_file_with_facade_fallback(
            import_path,
            canonical_file,
            string_table,
        )
        .map_err(SourceDiscoveryError::from)?;

    // Ensure target module root facades are compiled for cross-module imports.
    // WHY: when an import resolves to an implementation file in another module root,
    //      the facade must be available so AST can validate boundary enforcement.
    if let Some(importer_root) = project_path_resolver.module_root_for_file(canonical_file)
        && let Some(target_root) = project_path_resolver.module_root_for_file(&resolved.path)
        && importer_root != target_root
        && let Some(facade_path) = project_path_resolver
            .module_root_facades()
            .get(&target_root)
        && !reachable.contains(&ReachableSourceFile {
            path: facade_path.clone(),
            kind: SourceFileKind::Beanstalk,
        })
    {
        queue.push_back(ReachableSourceFile {
            path: facade_path.clone(),
            kind: SourceFileKind::Beanstalk,
        });
    }

    // Queue the resolved implementation file if not already visited.
    let resolved_source_file = resolved_source_file(&resolved.path, resolved.kind);
    if !reachable.contains(&resolved_source_file) {
        queue.push_back(resolved_source_file);
    }

    Ok(())
}

fn handle_provider_capable_import(
    import_path: &InternedPath,
    canonical_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<ImportPolicyAction, SourceDiscoveryError> {
    // Skip virtual package imports — AST resolution handles those.
    if external_imports
        .external_packages
        .is_virtual_package_import(import_path, string_table)
    {
        return Ok(ImportPolicyAction::Skip);
    }

    // Check for unsupported builder-specific core packages.
    if let Some(package_path) = external_imports
        .external_packages
        .unsupported_known_package_import(import_path, string_table)
    {
        return Err(SourceDiscoveryError::from(
            unsupported_builder_package_error(canonical_file, package_path, string_table),
        ));
    }

    // Detect provider-backed import prefixes (e.g. `./drawing.js` from
    //    `@./drawing.js/draw` or `@./drawing.js`).
    //    If a provider supports the extension, resolve the prefix, call the provider,
    //    and register the result. Do not add external files to the Beanstalk input list.
    if let Some((prefix_path, prefix_str, extension)) =
        provider_backed_import_prefix(import_path, string_table)
    {
        if let Some(provider) = external_imports.providers.find_by_extension(extension) {
            let provider_imports_start = crate::timing::start_pipeline_timing();
            let result = resolve_provider_backed_import(
                ProviderBackedImportRequest {
                    importer_canonical_path: canonical_file,
                    import_path,
                    prefix_path: &prefix_path,
                    raw_prefix: &prefix_str,
                    provider,
                    project_path_resolver,
                },
                external_imports,
                string_table,
            );
            log_stage_timing(
                "stage0.reachable_discovery.provider_imports",
                provider_imports_start,
            );
            result?;
            crate::timing::record_counter("stage0.reachable_discovery.provider_imports", 1.0);
            return Ok(ImportPolicyAction::Skip);
        }

        // No provider registered for this extension — report unsupported extension.
        let extension_owned = extension.to_owned();
        return Err(SourceDiscoveryError::from(
            unsupported_external_extension_error(
                canonical_file,
                import_path,
                &extension_owned,
                string_table,
            ),
        ));
    }

    Ok(ImportPolicyAction::QueueLocal)
}

// -------------------------
//  Provider-free discovery
// -------------------------

/// Conservative pre-scan that proves a directory build has no reachable provider-backed imports.
///
/// WHAT: walks the same import graph as discovery and returns cached source text only when every
///       reachable import is either a virtual package import, an extensionless Beanstalk import,
///       or a builder-supported source kind (e.g. registered `.bd`/`.md`).
/// WHY: the provider-capable discovery path mutates `ExternalImportDiscoveryState`; proving the
///      project is provider-free before forking lets multi-entry module discovery run in Rayon
///      workers that never touch provider registry deltas.
pub(super) fn classify_provider_free_project(
    entry_points: &[PathBuf],
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<Option<ProviderFreeProjectInventory>, SourceDiscoveryError> {
    let total_start = crate::timing::start_pipeline_timing();
    let mut policy = ImportPolicy::FreeClassification(external_packages);
    let inventory = match traverse_reachable_source_files(
        entry_points,
        project_path_resolver,
        style_directives,
        &mut policy,
        string_table,
    ) {
        Ok(inventory) => inventory,
        Err(error) => {
            log_stage_timing("stage0.reachable_discovery.total", total_start);
            return Err(error);
        }
    };

    log_stage_timing("stage0.reachable_discovery.total", total_start);
    Ok(inventory.map(|reachable| ProviderFreeProjectInventory {
        source_cache: reachable.source_cache,
    }))
}

fn handle_provider_free_classification_import(
    import_path: &InternedPath,
    _canonical_file: &Path,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<ImportPolicyAction, SourceDiscoveryError> {
    if external_packages.is_virtual_package_import(import_path, string_table) {
        return Ok(ImportPolicyAction::Skip);
    }

    if external_packages
        .unsupported_known_package_import(import_path, string_table)
        .is_some()
    {
        // The existing serial path reports this diagnostic with full context.
        return Ok(ImportPolicyAction::AbortClassification);
    }

    if provider_backed_import_prefix(import_path, string_table).is_some() {
        // Registered provider extensions need the serial provider-capable path so the
        // provider is called and the resolution table is populated. Unsupported non-Beanstalk
        // extensions also fall back so the existing diagnostic shape is preserved.
        return Ok(ImportPolicyAction::AbortClassification);
    }

    Ok(ImportPolicyAction::QueueLocal)
}

/// Marker error returned from Rayon workers when provider-free discovery fails.
///
/// WHAT: workers use worker-local `StringTable` values, so their diagnostics are not interpretable
///       on the main thread. Instead of exposing interned IDs cross-thread, the worker reports
///       failure and the caller falls back to the serial provider-capable path.
#[derive(Debug)]
pub(super) struct ProviderFreeDiscoveryFailed;

/// Provider-free BFS over one module's import graph.
///
/// WHAT: shares the same traversal mechanics as provider-capable discovery but skips all
///       provider-backed import handling and reuses source text proved safe by classification.
/// WHY: this function is safe to call inside Rayon workers because it only needs an immutable
///      `ExternalPackageRegistry` and a worker-local `StringTable`.
pub(super) fn discover_reachable_source_files_provider_free(
    entry_point: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &ExternalPackageRegistry,
    project_source_cache: &FxHashMap<PathBuf, String>,
    string_table: &mut StringTable,
) -> Result<ReachableSourceInventory, SourceDiscoveryError> {
    let total_start = crate::timing::start_pipeline_timing();

    let mut policy = ImportPolicy::FreeWorker {
        external_packages,
        project_source_cache,
    };
    let inventory = match traverse_reachable_source_files(
        &[entry_point.to_path_buf()],
        project_path_resolver,
        style_directives,
        &mut policy,
        string_table,
    ) {
        Ok(inventory) => inventory,
        Err(error) => {
            log_stage_timing("stage0.reachable_discovery.total", total_start);
            return Err(error);
        }
    };

    let result = inventory.ok_or_else(|| {
        // Worker traversal never aborts classification, so this is unreachable.
        SourceDiscoveryError::from(CompilerError::compiler_error(
            "Provider-free worker reachable-file traversal aborted unexpectedly",
        ))
    });

    log_stage_timing("stage0.reachable_discovery.total", total_start);
    result
}

fn handle_provider_free_worker_import(
    import_path: &InternedPath,
    canonical_file: &Path,
    external_packages: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<ImportPolicyAction, SourceDiscoveryError> {
    if external_packages.is_virtual_package_import(import_path, string_table) {
        return Ok(ImportPolicyAction::Skip);
    }

    // Defensive check: classification should already have rejected unsupported packages.
    if let Some(package_path) =
        external_packages.unsupported_known_package_import(import_path, string_table)
    {
        return Err(SourceDiscoveryError::from(
            unsupported_builder_package_error(canonical_file, package_path, string_table),
        ));
    }

    Ok(ImportPolicyAction::QueueLocal)
}

fn load_missing_sources(
    missing_sources: Vec<MissingSourceFile>,
    string_table: &mut StringTable,
) -> Result<Vec<LoadedMissingSourceFile>, CompilerMessages> {
    if missing_sources.is_empty() {
        return Ok(Vec::new());
    }

    if missing_sources.len() < STAGE0_PARALLEL_SOURCE_LOAD_MIN_FILES {
        add_frontend_counter(
            FrontendCounter::Stage0SerialSourceLoadCount,
            missing_sources.len(),
        );
        return load_missing_sources_serial(missing_sources, string_table);
    }

    add_frontend_counter(
        FrontendCounter::Stage0ParallelSourceLoadCount,
        missing_sources.len(),
    );
    load_missing_sources_parallel(missing_sources, string_table)
}

fn load_missing_sources_serial(
    missing_sources: Vec<MissingSourceFile>,
    string_table: &mut StringTable,
) -> Result<Vec<LoadedMissingSourceFile>, CompilerMessages> {
    let mut loaded_sources = Vec::with_capacity(missing_sources.len());

    for missing in missing_sources {
        let source_code = match extract_source_code(&missing.source_file.path, string_table) {
            Ok(source_code) => source_code,
            Err(error) => return Err(SourceDiscoveryError::from(error).into_messages(string_table)),
        };

        loaded_sources.push(LoadedMissingSourceFile {
            input_index: missing.input_index,
            source_file: missing.source_file,
            source_code,
        });
    }

    Ok(loaded_sources)
}

fn load_missing_sources_parallel(
    missing_sources: Vec<MissingSourceFile>,
    string_table: &mut StringTable,
) -> Result<Vec<LoadedMissingSourceFile>, CompilerMessages> {
    let mut loaded_sources = missing_sources
        .into_par_iter()
        .map(
            |missing| match read_source_code(&missing.source_file.path) {
                Ok(source_code) => Ok(LoadedMissingSourceFile {
                    input_index: missing.input_index,
                    source_file: missing.source_file,
                    source_code,
                }),
                Err(error) => Err(SourceReadFailure {
                    input_index: missing.input_index,
                    path: missing.source_file.path,
                    error,
                }),
            },
        )
        .collect::<Vec<_>>();

    loaded_sources.sort_by_key(|result| match result {
        Ok(loaded) => loaded.input_index,
        Err(failure) => failure.input_index,
    });

    let mut ordered_loaded_sources = Vec::with_capacity(loaded_sources.len());
    for loaded in loaded_sources {
        match loaded {
            Ok(loaded) => ordered_loaded_sources.push(loaded),
            Err(failure) => {
                let error = source_read_error(&failure.path, failure.error, string_table);
                return Err(SourceDiscoveryError::from(error).into_messages(string_table));
            }
        }
    }

    Ok(ordered_loaded_sources)
}

#[cfg(test)]
pub(super) fn load_missing_source_path_for_test(
    source_path: PathBuf,
    source_kind: SourceFileKind,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let missing_sources = vec![MissingSourceFile {
        input_index: 0,
        source_file: ReachableSourceFile {
            path: source_path,
            kind: source_kind,
        },
    }];

    load_missing_sources(missing_sources, string_table).map(|_| ())
}

#[cfg(test)]
pub(super) fn load_missing_source_paths_for_test(
    source_paths: Vec<PathBuf>,
    source_kind: SourceFileKind,
    string_table: &mut StringTable,
) -> Result<Vec<crate::build_system::build::InputFile>, CompilerMessages> {
    let missing_sources = source_paths
        .into_iter()
        .enumerate()
        .map(|(input_index, source_path)| MissingSourceFile {
            input_index,
            source_file: ReachableSourceFile {
                path: source_path,
                kind: source_kind,
            },
        })
        .collect();

    load_missing_sources(missing_sources, string_table).map(|loaded_sources| {
        loaded_sources
            .into_iter()
            .map(|loaded| crate::build_system::build::InputFile {
                source_code: loaded.source_code,
                source_path: loaded.source_file.path,
                source_kind: loaded.source_file.kind,
            })
            .collect()
    })
}

fn resolved_source_file(path: &Path, kind: SourceFileKind) -> ReachableSourceFile {
    ReachableSourceFile {
        path: path.to_path_buf(),
        kind,
    }
}

fn queue_same_directory_facade_for_beandown(
    beandown_path: &Path,
    reachable: &BTreeSet<ReachableSourceFile>,
    queue: &mut VecDeque<ReachableSourceFile>,
) {
    let Some(directory) = beandown_path.parent() else {
        return;
    };

    let facade_path = directory.join(MOD_FILE_NAME);
    if !facade_path.is_file() {
        return;
    }

    let facade_source_file = ReachableSourceFile {
        path: fs::canonicalize(&facade_path).unwrap_or(facade_path),
        kind: SourceFileKind::Beanstalk,
    };
    if !reachable.contains(&facade_source_file) {
        queue.push_back(facade_source_file);
    }
}

// -------------------------
//  Provider-backed import resolution
// -------------------------

/// Scans the components of an import path and returns the first file prefix whose final component
/// has an explicit non-`.bst` extension.
///
/// WHAT: for grouped syntax such as `import @./drawing.js { draw }` the tokenized path is
/// `@./drawing.js/draw`; this helper extracts the prefix `./drawing.js` and the extension `js`.
/// For a bare namespace import such as `import @./helper.js` the path is `@./helper.js`; the
/// prefix is `./helper.js`.
/// WHY: provider resolution must happen for the file prefix, while any remaining components are
/// symbol names to be resolved inside the provider-created package.
fn provider_backed_import_prefix<'a>(
    import_path: &InternedPath,
    string_table: &'a StringTable,
) -> Option<(InternedPath, String, &'a str)> {
    let components = import_path.as_components();
    if components.is_empty() {
        return None;
    }

    // Walk components to find the provider-owned file segment. Any later path components are
    // grouped-import symbol names, not filesystem path segments.
    for (index, component) in components.iter().enumerate() {
        let segment = string_table.resolve(*component);
        let path = Path::new(segment);
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if SourceFileKind::from_extension(extension).is_some() {
            continue;
        }

        let prefix_components = components[..=index].to_vec();
        let prefix_path = InternedPath::from_components(prefix_components);
        let prefix_str = prefix_path.to_portable_string(string_table);
        return Some((prefix_path, prefix_str, extension));
    }

    None
}

struct ProviderBackedImportRequest<'a> {
    importer_canonical_path: &'a Path,
    import_path: &'a InternedPath,
    prefix_path: &'a InternedPath,
    raw_prefix: &'a str,
    provider: &'a std::sync::Arc<dyn ExternalImportProvider>,
    project_path_resolver: &'a ProjectPathResolver,
}

/// Resolves a provider-backed import prefix to a canonical filesystem path, checks the build cache,
/// calls the provider if needed, and records the result in the resolution table and package registry.
fn resolve_provider_backed_import(
    request: ProviderBackedImportRequest<'_>,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<(), SourceDiscoveryError> {
    // Resolve the prefix to a canonical filesystem path without .bst extension or facade fallback.
    let canonical_source_path = resolve_provider_prefix_to_canonical_path(
        request.prefix_path,
        request.importer_canonical_path,
        request.project_path_resolver,
        string_table,
    )?;

    // Enforce module/library boundaries for provider-backed imports.
    // A file may only directly import a .js file that lives in the same module,
    // source library, or default entry-root area.
    check_provider_import_module_boundary(
        request.importer_canonical_path,
        &canonical_source_path,
        request.import_path,
        request.project_path_resolver,
        string_table,
    )?;

    // Build the cache key.
    let cache_key = ExternalImportCacheKey {
        canonical_source_path: canonical_source_path.clone(),
        provider_kind: request.provider.kind(),
    };

    // Use cached result when available.
    if let Some(cached) = external_imports.cache.get(&cache_key) {
        let source_file_logical = source_file_logical_path(
            request.importer_canonical_path,
            request.project_path_resolver,
            string_table,
        )?;
        let import_prefix_logical = source_file_logical_path(
            &canonical_source_path,
            request.project_path_resolver,
            string_table,
        )?;
        insert_external_import_resolution(
            external_imports.resolution_table,
            source_file_logical,
            request.raw_prefix,
            import_prefix_logical,
            cached.clone(),
        );
        return Ok(());
    }

    // Build the provider request.
    let provider_request = ExternalImportRequest {
        import_path: request.import_path.to_portable_string(string_table),
        canonical_source_path: canonical_source_path.clone(),
        source_location:
            crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
                request.importer_canonical_path,
                string_table,
            ),
    };

    let result = {
        let mut context = ExternalImportProviderContext {
            package_registry: external_imports.external_packages,
            cache: external_imports.cache,
            string_table,
        };

        request
            .provider
            .resolve_external_import(provider_request, &mut context)
            .map_err(SourceDiscoveryError::from)?
    };

    if let Some(resolved) = result {
        external_imports.cache.insert(cache_key, resolved.clone());

        let source_file_logical = source_file_logical_path(
            request.importer_canonical_path,
            request.project_path_resolver,
            string_table,
        )?;
        let import_prefix_logical = source_file_logical_path(
            &canonical_source_path,
            request.project_path_resolver,
            string_table,
        )?;
        insert_external_import_resolution(
            external_imports.resolution_table,
            source_file_logical,
            request.raw_prefix,
            import_prefix_logical,
            resolved,
        );
    }

    Ok(())
}

fn insert_external_import_resolution(
    external_import_resolution_table: &mut ExternalImportResolutionTable,
    source_file_logical: String,
    raw_import_prefix: &str,
    logical_import_prefix: String,
    resolved: crate::libraries::external_import_providers::provider::ResolvedExternalImport,
) {
    external_import_resolution_table.insert(
        source_file_logical.clone(),
        logical_import_prefix.clone(),
        resolved.clone(),
    );

    if raw_import_prefix != logical_import_prefix {
        external_import_resolution_table.insert(source_file_logical, raw_import_prefix, resolved);
    }
}

/// Resolves a provider import prefix to a canonical filesystem path without appending `.bst` or
/// using facade fallback.
///
/// WHAT: reuses the normal base/boundary/case rules from `ProjectPathResolver` but skips the
/// `.bst` extension logic and facade fallback used for Beanstalk source imports.
fn resolve_provider_prefix_to_canonical_path(
    prefix_path: &InternedPath,
    importer_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<PathBuf, SourceDiscoveryError> {
    let (base_kind, filesystem_base) = project_path_resolver
        .resolve_path_base_for_provider(prefix_path, importer_file, string_table)
        .map_err(SourceDiscoveryError::from)?;

    let normalized = join_and_normalize_path(&filesystem_base, prefix_path, string_table);

    let canonical = fs::canonicalize(&normalized)
        .map_err(|error| {
            CompilerError::file_error(
                importer_file,
                format!(
                    "Failed to canonicalize external import prefix '{}': {error}",
                    normalized.display()
                ),
                string_table,
            )
        })
        .map_err(SourceDiscoveryError::from)?;

    crate::compiler_frontend::paths::import_resolution::validate_import_boundary(
        &canonical,
        &base_kind,
        &filesystem_base,
        prefix_path,
        importer_file,
        string_table,
    )
    .map_err(SourceDiscoveryError::from)?;

    Ok(canonical)
}

/// Derives the portable logical path for a canonical source file.
fn source_file_logical_path(
    canonical_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<String, SourceDiscoveryError> {
    let logical = project_path_resolver
        .logical_path_for_canonical_file(canonical_file, string_table)
        .map_err(SourceDiscoveryError::from)?;
    Ok(logical.to_string_lossy().replace('\\', "/"))
}

// -------------------------
//  Provider import boundary check
// -------------------------

/// Enforce that a provider-backed import does not cross a module or source-library boundary.
///
/// WHAT: .js files are private implementation details of the module or library that owns them.
///       Cross-module or cross-library .js imports bypass the facade and are rejected.
/// WHY: provider-backed imports must obey the same visibility boundaries as .bst source imports.
fn check_provider_import_module_boundary(
    importer_file: &Path,
    target_file: &Path,
    import_path: &InternedPath,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), SourceDiscoveryError> {
    let importer_container = provider_import_container(project_path_resolver, importer_file);
    let target_container = provider_import_container(project_path_resolver, target_file);

    if importer_container != target_container {
        let location = SourceLocation::from_path(importer_file, string_table);
        return Err(SourceDiscoveryError::from(
            CompilerDiagnostic::cross_module_import_not_exported(import_path.clone(), location),
        ));
    }

    Ok(())
}

/// Determine the boundary "container" of a file for provider import checks.
///
/// WHAT: returns the module root, source library root, or entry root that contains the file.
/// WHY: two files in the same container may freely import each other's .js files.
fn provider_import_container(
    project_path_resolver: &ProjectPathResolver,
    file: &Path,
) -> Option<PathBuf> {
    // Module roots are the most specific boundaries.
    if let Some(root) = project_path_resolver.module_root_for_file(file) {
        return Some(root);
    }

    // Source libraries are the next boundary.
    for root in project_path_resolver.source_library_roots().values() {
        if file.starts_with(root) {
            return Some(root.clone());
        }
    }

    // Everything under the entry root belongs to the default module.
    if file.starts_with(project_path_resolver.entry_root()) {
        return Some(project_path_resolver.entry_root().to_path_buf());
    }

    None
}

// -------------------------
//  Diagnostic Helpers
// -------------------------

fn unsupported_builder_package_error(
    importer: &Path,
    package_path: &str,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let package_path_id = string_table.intern(package_path);
    let location =
        crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
            importer,
            string_table,
        );
    CompilerDiagnostic::unsupported_builder_package(package_path_id, location)
}

fn unsupported_external_extension_error(
    importer: &Path,
    import_path: &InternedPath,
    extension: &str,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let extension_id = string_table.intern(extension);
    let location =
        crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
            importer,
            string_table,
        );
    CompilerDiagnostic::unsupported_external_extension(import_path.clone(), extension_id, location)
}
