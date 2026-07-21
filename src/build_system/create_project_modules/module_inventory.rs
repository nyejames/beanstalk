//! Directory-project module inventory assembly.
//!
//! WHAT: turns the canonical project module graph's normal entry modules into `DiscoveredModule`
//! records containing all transitively reachable input files.
//! WHY: module inventory is the Stage 0 bridge between the structural graph and parallel frontend
//! compilation. Entry root paths and deterministic entry order come from the graph's compile
//! waves so entry classification has one owner; root setup and source-backed package validation
//! live in sibling modules.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::import_scanning::ScannedImportSource;
use super::module_identity::ModuleId;
use super::prepared_source::PreparedSourceInput;
use super::project_module_graph::ProjectModuleGraph;
use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};
use super::reachable_file_discovery::{
    ExternalImportDiscoveryState, ProviderFreeDiscoveryFailed, ProviderFreeProjectInventory,
    ReachableSourceInventory, assemble_input_files_from_inventory, classify_provider_free_project,
    collect_reachable_input_files, discover_reachable_source_files_provider_free,
};

/// Minimum number of entry modules before the provider-free path uses Rayon.
///
/// WHY: a single module pays the fork/merge overhead without any cross-module work to overlap,
///      so it stays serial. Multi-entry directory builds are the case the parallel path targets.
const PROVIDER_FREE_PARALLEL_MIN_MODULES: usize = 2;

/// Entry point and all collected source files for one discovered module.
pub(crate) struct DiscoveredModule {
    pub(crate) entry_point: PathBuf,
    pub(crate) input_files: Vec<PreparedSourceInput>,
}

/// Discovers every normal entry module in the directory project and its reachable dependencies.
///
/// Entry root paths and deterministic entry order are derived from the project module graph's
/// compile waves, filtered to normal entry modules. Support roots and the project package facade
/// are never entries. With no dependency edges inserted yet, compile waves collapse to a single
/// wave in deterministic `ModuleId` order, so entry order matches the prior path-sorted
/// entry-candidate order. A defensive graph cycle surfaces through the existing
/// `CompilerMessages`/string-table diagnostic boundary without panicking.
pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    project_module_graph: &ProjectModuleGraph,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let entry_points = normal_entry_root_paths_from_graph(project_module_graph, string_table)?;

    if entry_points.is_empty() {
        return Err(config_diagnostic_messages(
            config,
            "entry_root",
            InvalidConfigReason::NoRootModuleEntries {
                entry_root: path_id(project_path_resolver.entry_root(), string_table),
            },
            string_table,
        ));
    }

    // Conservative gate: only take the provider-free parallel path when the entire reachable
    // import graph contains no provider-backed imports and no unsupported non-Beanstalk
    // extensions. This keeps provider cache/resolution table mutations on the serial path.
    // WHY: classification itself reads and tokenizes every reachable Beanstalk source file and
    //      retains the complete scan cache. It records `provider_capable_required` and skips the
    //      external edge when a provider-backed or unsupported package import needs the serial
    //      owner, but it never aborts and discards that cache. The serial fallback then reuses the
    //      retained lexical data for every already-scanned `.bst` so the lexer never runs twice.
    //      Skip classification for the common single-entry case because that path stays serial
    //      provider-capable anyway.
    let provider_free_inventory = if entry_points.len() >= PROVIDER_FREE_PARALLEL_MIN_MODULES {
        classify_provider_free_project(
            &entry_points,
            project_path_resolver,
            style_directives,
            &*external_imports.external_packages,
            string_table,
        )
        .map_err(|error| error.into_messages(string_table))?
    } else {
        ProviderFreeProjectInventory::provider_capable_required()
    };

    if !provider_free_inventory.provider_capable_required {
        let provider_free_modules = discover_modules_provider_free_parallel(
            &entry_points,
            project_path_resolver,
            style_directives,
            &*external_imports.external_packages,
            &provider_free_inventory,
            string_table,
        );

        match provider_free_modules {
            Ok(modules) => return Ok(modules),
            Err(ProviderFreeDiscoveryFailed) => {
                // Worker-local diagnostics cannot be rendered on the parent string table. Retry on
                // the serial provider-capable path so the existing Stage 0 diagnostic owner reports
                // the real filesystem/import failure with stable path identity. Reuse the complete
                // classification cache so the lexer never runs twice for the same source.
                return discover_modules_serial_provider_capable(
                    entry_points,
                    project_path_resolver,
                    style_directives,
                    external_imports,
                    Some(&provider_free_inventory.source_cache),
                    string_table,
                );
            }
        }
    }

    discover_modules_serial_provider_capable(
        entry_points,
        project_path_resolver,
        style_directives,
        external_imports,
        Some(&provider_free_inventory.source_cache),
        string_table,
    )
}

/// Deterministic normal entry root paths from the project module graph's compile waves.
///
/// Flattens the graph's compile waves in dependency order and keeps only normal entry modules,
/// mapping each to its canonical root file. Support roots and the project package facade are
/// excluded because they never appear in `entry_modules`. A graph cycle is an internal compiler
/// failure and is surfaced through the `CompilerMessages`/string-table boundary without panicking.
fn normal_entry_root_paths_from_graph(
    project_module_graph: &ProjectModuleGraph,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, CompilerMessages> {
    let waves = project_module_graph
        .compile_waves()
        .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

    let entry_modules: BTreeSet<ModuleId> = project_module_graph
        .entry_modules()
        .iter()
        .copied()
        .collect();

    Ok(waves
        .iter()
        .flat_map(|wave| wave.iter().copied())
        .filter(|module_id| entry_modules.contains(module_id))
        .map(|module_id| {
            project_module_graph
                .node(module_id)
                .root_file()
                .to_path_buf()
        })
        .collect())
}

/// Serial provider-capable fallback.
///
/// WHAT: the original Stage 0 module loop. It mutates `ExternalImportDiscoveryState` and the
///       shared `StringTable`, so it is kept serial and is used whenever the project is not
///       proven provider-free.
fn discover_modules_serial_provider_capable(
    entry_points: Vec<PathBuf>,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    classification_cache: Option<&FxHashMap<PathBuf, ScannedImportSource>>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let mut modules = Vec::with_capacity(entry_points.len());

    for entry_point in entry_points {
        let input_files = collect_reachable_input_files(
            &entry_point,
            project_path_resolver,
            style_directives,
            external_imports,
            classification_cache,
            string_table,
        )?;

        modules.push(DiscoveredModule {
            entry_point,
            input_files,
        });
    }

    Ok(modules)
}

/// Parallel provider-free module discovery.
///
/// WHAT: discovers each module's reachable files in a separate Rayon worker using a worker-local
///       `StringTable`; the shared `StringTable` is only used again when assembling
///       `PreparedSourceInput` values on the main thread.
/// WHY: provider-free BFS is embarrassingly parallel across entry points and does not need the
///      mutable provider state that makes provider-capable discovery serial.
fn discover_modules_provider_free_parallel(
    entry_points: &[PathBuf],
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &crate::compiler_frontend::external_packages::ExternalPackageRegistry,
    provider_free_inventory: &ProviderFreeProjectInventory,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, ProviderFreeDiscoveryFailed> {
    // Run provider-free BFS for each entry point in parallel. Each worker forks a local
    // `StringTable` from the parent so classification's retained tokens (parent-valid StringIds)
    // stay interpretable without re-tokenizing. Workers only return `ReachableSourceInventory`
    // carrying source text plus retained tokens whose StringIds are parent-valid, so no
    // worker-local IDs need cross-thread interpretation or remapping.
    let fork_source = string_table.fork_source();
    let mut indexed_inventories: Vec<(usize, ReachableSourceInventory)> = entry_points
        .par_iter()
        .enumerate()
        .map(|(index, entry_path)| {
            let mut local_string_table = fork_source.fork_for_module().into_parts().0;
            let inventory = discover_reachable_source_files_provider_free(
                entry_path,
                project_path_resolver,
                style_directives,
                external_packages,
                &provider_free_inventory.source_cache,
                &mut local_string_table,
            )
            .map_err(|_| ProviderFreeDiscoveryFailed)?;

            Ok((index, inventory))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Deterministic ordering: sort by the original entry-point index regardless of completion
    // order, preserving module order by entry path.
    indexed_inventories.sort_by_key(|(index, _)| *index);

    let mut modules = Vec::with_capacity(entry_points.len());
    for (index, inventory) in indexed_inventories {
        let input_files = assemble_input_files_from_inventory(inventory, string_table)
            .map_err(|_| ProviderFreeDiscoveryFailed)?;
        modules.push(DiscoveredModule {
            entry_point: entry_points[index].clone(),
            input_files,
        });
    }

    Ok(modules)
}
