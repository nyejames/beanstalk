//! Directory-project module inventory assembly.
//!
//! WHAT: turns the canonical project module graph's normal entry modules into `DiscoveredModule`
//! records containing all transitively reachable input files.
//! WHY: module inventory is the Stage 0 bridge between the structural graph and parallel frontend
//! compilation. Entry root paths and deterministic entry order come from the graph's compile
//! waves so entry classification has one owner; root setup and source-backed package validation
//! live in sibling modules.

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::import_scanning::ScannedImportSource;
use super::module_identity::ModuleId;
use super::prepared_source::PreparedSourceInput;
use super::project_module_graph::ProjectModuleGraph;
use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};
use super::reachable_file_discovery::{
    CollectedReachableInputs, ExternalImportDiscoveryState, LocalStructuralDependencyFact,
    ProviderFreeDiscoveryFailed, ProviderFreeProjectInventory, ReachableSourceInventory,
    assemble_input_files_from_inventory, classify_provider_free_project,
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
/// Entry root files are seeded from the graph's normal entry modules in deterministic `ModuleId`
/// order. Reachable-file discovery retains one local structural dependency fact per cross-module
/// import resolution; after discovery completes the facts are merged into the graph as
/// provider-before-consumer edges, and the returned modules are ordered by the populated
/// graph's compile waves (filtered to normal entry modules). That order is the deterministic
/// inventory/result position only; the directory compiler still feeds the returned vector to a
/// Rayon batch, so actual dependency-wave semantic scheduling is a later Phase 5 slice. Support
/// roots and the project package facade are never entries. A defensive graph cycle, a missing
/// project-local root or a graph/inventory disagreement surfaces through the existing
/// `CompilerMessages`/string-table boundary without panicking.
pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    project_module_graph: &mut ProjectModuleGraph,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let entry_points = normal_entry_root_paths_in_module_id_order(project_module_graph);

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

    let (mut modules, dependency_facts) = if !provider_free_inventory.provider_capable_required {
        match discover_modules_provider_free_parallel(
            &entry_points,
            project_path_resolver,
            style_directives,
            &*external_imports.external_packages,
            &provider_free_inventory,
            string_table,
        ) {
            Ok(outcome) => outcome,
            Err(ProviderFreeDiscoveryFailed) => {
                // Worker-local diagnostics cannot be rendered on the parent string table. Retry on
                // the serial provider-capable path so the existing Stage 0 diagnostic owner reports
                // the real filesystem/import failure with stable path identity. Reuse the complete
                // classification cache so the lexer never runs twice for the same source.
                discover_modules_serial_provider_capable(
                    &entry_points,
                    project_path_resolver,
                    style_directives,
                    external_imports,
                    Some(&provider_free_inventory.source_cache),
                    string_table,
                )?
            }
        }
    } else {
        discover_modules_serial_provider_capable(
            &entry_points,
            project_path_resolver,
            style_directives,
            external_imports,
            Some(&provider_free_inventory.source_cache),
            string_table,
        )?
    };

    // Merge the retained local structural dependency facts into the graph as
    // provider-before-consumer edges before compile waves are computed. Edges are idempotent, so
    // duplicate observations across entry closures collapse without changing the graph.
    merge_local_structural_dependencies(project_module_graph, &dependency_facts, string_table)?;

    // Order the discovered modules by the now-populated graph's compile waves so providers precede
    // consumers in the returned inventory order. Discovery seeded entries in `ModuleId` order;
    // this reorders the result to the dependency-ordered wave order without re-running discovery.
    // The directory compiler still feeds this vector to a Rayon batch, so Phase 5b establishes
    // deterministic dependency-ordered inventory position only; actual dependency-wave semantic
    // scheduling is a later Phase 5 slice.
    order_discovered_modules_by_compile_waves(project_module_graph, &mut modules, string_table)
}

/// Deterministic normal entry root files in `ModuleId` order, for discovery seeding.
///
/// Maps the graph's normal entry modules to their canonical root files in `ModuleId` order.
/// Support roots and the project package facade are excluded because they never appear in
/// `entry_modules`. Compile waves are not consulted here: dependency edges are inserted only after
/// discovery completes, so seeding uses the stable identity order.
fn normal_entry_root_paths_in_module_id_order(
    project_module_graph: &ProjectModuleGraph,
) -> Vec<PathBuf> {
    project_module_graph
        .entry_modules()
        .iter()
        .map(|module_id| {
            project_module_graph
                .node(*module_id)
                .root_file()
                .to_path_buf()
        })
        .collect()
}

/// Merge retained local structural dependency facts into the graph as provider-before-consumer
/// edges.
///
/// WHAT: maps each fact's canonical consumer and provider roots to `ModuleId` through the graph,
///       then inserts a provider-before-consumer edge that also retains the authored source
///       location. Duplicate observations are idempotent for the edge and never overwrite the
///       retained location; source locations are never used for edge identity.
/// WHY: the graph owns the canonical-root-to-`ModuleId` mapping and the edge adjacency, so the
///      merge has one owner. Facts are sorted by canonical root pair before insertion so the
///      retained location and merge order are deterministic and independent of Rayon completion
///      order. A fact root is derived from `ProjectPathResolver::module_root_for_file`, whose
///      table holds normal project roots only, so every fact root must already be a graph node.
///      Any absent root is therefore an internal invariant failure surfaced as a `CompilerError`
///      rather than a panic or a silent skip; there is no source-package alternate policy.
fn merge_local_structural_dependencies(
    project_module_graph: &mut ProjectModuleGraph,
    dependency_facts: &[LocalStructuralDependencyFact],
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    if dependency_facts.is_empty() {
        return Ok(());
    }

    let mut ordered_facts = dependency_facts.to_vec();
    ordered_facts.sort_by(|left, right| {
        left.consumer_root
            .cmp(&right.consumer_root)
            .then_with(|| left.provider_root.cmp(&right.provider_root))
    });

    for fact in ordered_facts {
        // Fact roots come from `ProjectPathResolver::module_root_for_file`, whose table holds
        // normal project roots only, so each root must resolve to a graph node. An absent root is
        // a proven internal invariant failure, not a user-facing condition.
        let consumer_id = project_module_graph
            .module_id_for_root_directory(&fact.consumer_root)
            .ok_or_else(|| {
                missing_dependency_root_error(
                    &fact.consumer_root,
                    &fact.provider_root,
                    &fact.consumer_root,
                    string_table,
                )
            })?;
        let provider_id = project_module_graph
            .module_id_for_root_directory(&fact.provider_root)
            .ok_or_else(|| {
                missing_dependency_root_error(
                    &fact.consumer_root,
                    &fact.provider_root,
                    &fact.provider_root,
                    string_table,
                )
            })?;

        project_module_graph
            .add_local_structural_dependency_edge(provider_id, consumer_id, fact.authored_location)
            .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;
    }

    Ok(())
}

/// Build the internal `CompilerError` for a project-local dependency-fact root that is absent
/// from the project module graph.
///
/// Reaching this helper means a project-local root was expected in the graph but is missing,
/// which is a proven invariant violation rather than a user-facing failure.
fn missing_dependency_root_error(
    consumer_root: &Path,
    provider_root: &Path,
    missing_root: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    CompilerMessages::from_error_ref(
        CompilerError::compiler_error(format!(
            "Local structural dependency fact references module root {missing_root:?} (consumer {consumer_root:?}, provider {provider_root:?}) absent from the project module graph"
        )),
        string_table,
    )
}

/// Build the internal `CompilerError` for a disagreement between the project module graph's
/// normal entry set and the discovered module inventories.
///
/// Reaching this helper means the graph and discovery disagree on which normal entry roots exist,
/// which is a proven invariant violation rather than a user-facing failure.
fn graph_inventory_mismatch_error(
    reason: String,
    string_table: &mut StringTable,
) -> CompilerMessages {
    CompilerMessages::from_error_ref(CompilerError::compiler_error(reason), string_table)
}

/// Reorder discovered modules to match the populated graph's compile-wave order.
///
/// WHAT: flattens the graph's compile waves, keeps only normal entry modules, and reorders the
///       discovered modules so providers precede consumers in the returned inventory order.
///       Modules are keyed by their canonical entry root file, which is the graph node's
///       `root_file` for each entry module.
/// WHY: discovery seeds entries in `ModuleId` order and collects dependency facts; the graph
///      inserts edges from those facts, so the dependency-ordered wave order is only known after
///      the merge. The graph and discovery must agree exactly on the normal entry set: every
///      graph entry root needs one matching discovered module and vice versa. Duplicate entry
///      paths, missing graph entries and leftover inventories are all internal invariant failures
///      surfaced through the `CompilerMessages`/string-table boundary. A graph cycle is the same
///      kind of internal failure reported by `compile_waves`.
fn order_discovered_modules_by_compile_waves(
    project_module_graph: &ProjectModuleGraph,
    modules: &mut Vec<DiscoveredModule>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let waves = project_module_graph
        .compile_waves()
        .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

    let entry_modules: BTreeSet<ModuleId> = project_module_graph
        .entry_modules()
        .iter()
        .copied()
        .collect();

    let ordered_entry_root_files: Vec<PathBuf> = waves
        .iter()
        .flat_map(|wave| wave.iter().copied())
        .filter(|module_id| entry_modules.contains(module_id))
        .map(|module_id| {
            project_module_graph
                .node(module_id)
                .root_file()
                .to_path_buf()
        })
        .collect();

    // Index discovered modules by their canonical entry root file. A duplicate entry point means
    // two inventories claim the same graph node, which breaks the one-to-one correspondence the
    // wave order depends on; report it as an internal failure instead of silently dropping one.
    let mut module_by_entry: FxHashMap<PathBuf, DiscoveredModule> = FxHashMap::default();
    for module in modules.drain(..) {
        let entry_point = module.entry_point.clone();
        if module_by_entry
            .insert(entry_point.clone(), module)
            .is_some()
        {
            return Err(graph_inventory_mismatch_error(
                format!(
                    "Module discovery produced duplicate entry root files for {entry_point:?}; the project module graph expects one discovered module per normal entry root"
                ),
                string_table,
            ));
        }
    }

    // Consume modules in dependency-ordered wave order. A graph entry root with no matching
    // discovered module means discovery and the graph disagree on the normal entry set.
    let mut ordered = Vec::with_capacity(ordered_entry_root_files.len());
    for entry_root_file in &ordered_entry_root_files {
        let module = match module_by_entry.remove(entry_root_file) {
            Some(module) => module,
            None => {
                return Err(graph_inventory_mismatch_error(
                    format!(
                        "The project module graph lists normal entry root {entry_root_file:?} that has no matching discovered module inventory"
                    ),
                    string_table,
                ));
            }
        };
        ordered.push(module);
    }

    // Any remaining inventory has no graph entry root, so discovery returned a module the graph
    // does not classify as a normal entry.
    if let Some(leftover) = module_by_entry.keys().next() {
        return Err(graph_inventory_mismatch_error(
            format!(
                "Module discovery returned an inventory for entry root {leftover:?} that the project module graph does not classify as a normal entry"
            ),
            string_table,
        ));
    }

    Ok(ordered)
}

/// Serial provider-capable fallback.
///
/// WHAT: the original Stage 0 module loop. It mutates `ExternalImportDiscoveryState` and the
///       shared `StringTable`, so it is kept serial and is used whenever the project is not
///       proven provider-free. It also retains the local structural dependency facts observed
///       during each entry's traversal so the graph merge can insert provider-before-consumer
///       edges after discovery.
fn discover_modules_serial_provider_capable(
    entry_points: &[PathBuf],
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    classification_cache: Option<&FxHashMap<PathBuf, ScannedImportSource>>,
    string_table: &mut StringTable,
) -> Result<(Vec<DiscoveredModule>, Vec<LocalStructuralDependencyFact>), CompilerMessages> {
    let mut modules = Vec::with_capacity(entry_points.len());
    let mut dependency_facts = Vec::new();

    for entry_point in entry_points {
        let CollectedReachableInputs {
            input_files,
            dependency_facts: entry_facts,
        } = collect_reachable_input_files(
            entry_point,
            project_path_resolver,
            style_directives,
            external_imports,
            classification_cache,
            string_table,
        )?;

        modules.push(DiscoveredModule {
            entry_point: entry_point.clone(),
            input_files,
        });
        dependency_facts.extend(entry_facts);
    }

    Ok((modules, dependency_facts))
}

/// Parallel provider-free module discovery.
///
/// WHAT: discovers each module's reachable files in a separate Rayon worker using a worker-local
///       `StringTable`; the shared `StringTable` is only used again when assembling
///       `PreparedSourceInput` values on the main thread. Workers also return the local
///       structural dependency facts they observe; the facts carry parent-valid string IDs and
///       canonical `PathBuf` roots, so they cross threads without remapping.
/// WHY: provider-free BFS is embarrassingly parallel across entry points and does not need the
///      mutable provider state that makes provider-capable discovery serial.
fn discover_modules_provider_free_parallel(
    entry_points: &[PathBuf],
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_packages: &crate::compiler_frontend::external_packages::ExternalPackageRegistry,
    provider_free_inventory: &ProviderFreeProjectInventory,
    string_table: &mut StringTable,
) -> Result<(Vec<DiscoveredModule>, Vec<LocalStructuralDependencyFact>), ProviderFreeDiscoveryFailed>
{
    // Run provider-free BFS for each entry point in parallel. Each worker forks a local
    // `StringTable` from the parent so classification's retained tokens (parent-valid StringIds)
    // stay interpretable without re-tokenizing. Workers only return the inventory and dependency
    // facts carrying source text plus retained tokens whose StringIds are parent-valid, so no
    // worker-local IDs need cross-thread interpretation or remapping.
    let fork_source = string_table.fork_source();
    let mut indexed_outcomes: Vec<(
        usize,
        ReachableSourceInventory,
        Vec<LocalStructuralDependencyFact>,
    )> = entry_points
        .par_iter()
        .enumerate()
        .map(|(index, entry_path)| {
            let mut local_string_table = fork_source.fork_for_module().into_parts().0;
            let discovery = discover_reachable_source_files_provider_free(
                entry_path,
                project_path_resolver,
                style_directives,
                external_packages,
                &provider_free_inventory.source_cache,
                &mut local_string_table,
            )
            .map_err(|_| ProviderFreeDiscoveryFailed)?;

            Ok((index, discovery.inventory, discovery.dependency_facts))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Deterministic ordering: sort by the original entry-point index regardless of completion
    // order, preserving module order by entry path and a deterministic fact merge order.
    indexed_outcomes.sort_by_key(|(index, _, _)| *index);

    let mut modules = Vec::with_capacity(entry_points.len());
    let mut dependency_facts = Vec::new();
    for (index, inventory, entry_facts) in indexed_outcomes {
        let input_files = assemble_input_files_from_inventory(inventory, string_table)
            .map_err(|_| ProviderFreeDiscoveryFailed)?;
        modules.push(DiscoveredModule {
            entry_point: entry_points[index].clone(),
            input_files,
        });
        dependency_facts.extend(entry_facts);
    }

    Ok((modules, dependency_facts))
}
