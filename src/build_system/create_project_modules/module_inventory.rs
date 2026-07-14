//! Directory-project module inventory assembly.
//!
//! WHAT: turns discovered root entry files into `DiscoveredModule` records containing all
//! transitively reachable input files.
//! WHY: module inventory is the Stage 0 bridge between filesystem discovery and parallel
//! frontend compilation; root setup and source-backed package validation live in sibling modules.

use crate::build_system::build::InputFile;

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use rayon::prelude::*;

use std::path::PathBuf;

use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};
use super::reachable_file_discovery::{
    ExternalImportDiscoveryState, ProviderFreeDiscoveryFailed, ProviderFreeProjectInventory,
    ReachableSourceInventory, assemble_input_files_from_inventory, classify_provider_free_project,
    collect_reachable_input_files, discover_reachable_source_files_provider_free,
};
use super::source_tree_index::SourceTreeIndex;

/// Minimum number of entry modules before the provider-free path uses Rayon.
///
/// WHY: a single module pays the fork/merge overhead without any cross-module work to overlap,
///      so it stays serial. Multi-entry directory builds are the case the parallel path targets.
const PROVIDER_FREE_PARALLEL_MIN_MODULES: usize = 2;

/// Entry point and all collected source files for one discovered module.
pub(crate) struct DiscoveredModule {
    pub(crate) entry_point: PathBuf,
    pub(crate) input_files: Vec<InputFile>,
}

/// Scans the directory project for all root entry files and their reachable dependencies.
pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    source_tree_index: &SourceTreeIndex,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let entry_points = source_tree_index.entry_candidates().to_vec();

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
    // WHY: classification itself reads Beanstalk source files; skip it for the common single-entry
    //      case because that path stays serial provider-capable anyway.
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
        None
    };

    if let Some(provider_free_inventory) = provider_free_inventory {
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
                // the real filesystem/import failure with stable path identity.
            }
        }
    }

    discover_modules_serial_provider_capable(
        entry_points,
        project_path_resolver,
        style_directives,
        external_imports,
        string_table,
    )
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
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let mut modules = Vec::with_capacity(entry_points.len());

    for entry_point in entry_points {
        let input_files = collect_reachable_input_files(
            &entry_point,
            project_path_resolver,
            style_directives,
            external_imports,
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
///       `StringTable`; the shared `StringTable` is only used again when assembling `InputFile`
///       values on the main thread.
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
    // Run provider-free BFS for each entry point in parallel. Each worker owns a fresh
    // `StringTable` and only returns `ReachableSourceInventory` (paths + source text), so no
    // interned IDs need cross-thread interpretation or remapping.
    let mut indexed_inventories: Vec<(usize, ReachableSourceInventory)> = entry_points
        .par_iter()
        .enumerate()
        .map(|(index, entry_path)| {
            let mut local_string_table = StringTable::new();
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
