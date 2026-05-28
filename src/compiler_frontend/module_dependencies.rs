//! Stage 3 dependency ordering for parsed Beanstalk headers.
//!
//! WHAT: topologically sorts top-level declaration headers by their header-provided dependency edges,
//! then appends `StartFunction` headers in source order. Finalizes the header-owned
//! `ModuleSymbols` package: declarations are built from the sorted headers and appended with
//! builtin declarations.
//!
//! ## Stage contract
//!
//! Dependency edges are header-provided top-level declaration dependencies.
//! They include type-surface dependencies and constant initializer dependencies.
//! Executable function/start body references remain excluded.
//!
//! **`start` excluded from the graph.** `StartFunction` headers are not graph participants — they
//! have no dependents and cannot be imported. They are appended after the sorted top-level
//! headers so AST emission sees all top-level declarations before processing the start body.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType, SourceLocation};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticBag};
use crate::compiler_frontend::headers::import_environment::HeaderImportEnvironment;
use crate::compiler_frontend::headers::module_symbols::{FacadeExportEntry, ModuleSymbols};
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::header_log;
use rustc_hash::{FxHashMap, FxHashSet};

/// Dependency-sorted module headers with a finalized symbol package.
///
/// WHAT: the output of `resolve_module_dependencies` — sorted headers and a `ModuleSymbols`
/// whose `declarations` Vec is in dependency order and includes builtin declarations.
/// WHY: all downstream stages (AST construction, build orchestration) consume this single bundle
/// without re-sorting or re-packaging the top-level symbol data.
#[derive(Debug)]
pub(crate) struct SortedHeaders {
    pub(crate) headers: Vec<Header>,
    pub(crate) top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub(crate) entry_runtime_fragment_count: usize,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
}

/// Tracks which modules are temporarily marked (in the current DFS stack)
/// and which have been permanently visited.
struct DependencyTracker {
    temp_mark: FxHashSet<InternedPath>,
    visited: FxHashSet<InternedPath>,
}

impl DependencyTracker {
    fn new(capacity: usize) -> Self {
        DependencyTracker {
            temp_mark: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
            visited: FxHashSet::with_capacity_and_hasher(capacity, Default::default()),
        }
    }
}

/// Topologically sort headers and finalize the header-owned module symbol package.
///
/// WHAT: sorts top-level declaration headers (non-start) by their header-provided dependency edges,
/// then appends `StartFunction` headers in source order. Builds the `declarations` Vec in sorted order
/// and appends builtin declarations.
///
/// WHY: `StartFunction` is excluded from the dependency graph — it is build-system-only and
/// cannot be imported by other headers. All other headers are sorted by header-provided edges
/// (type surfaces and constant initializer references) so AST sees dependencies first.
pub fn resolve_module_dependencies(
    parsed: Headers,
    string_table: &mut StringTable,
) -> Result<SortedHeaders, DiagnosticBag> {
    let Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count,
        mut module_symbols,
        import_environment,
    } = parsed;

    // Partition: StartFunction headers are appended last, not sorted.
    // WHY: start is build-system-only and has no dependents. Facade declarations remain graph
    // participants because other modules can depend on their public constants and type surfaces;
    // header import visibility, not dependency sorting, owns whether those declarations are visible.
    let mut start_headers: Vec<Header> = Vec::new();
    let top_level_headers: Vec<Header> = headers
        .into_iter()
        .filter_map(|h| {
            if matches!(h.kind, HeaderKind::StartFunction) {
                start_headers.push(h);
                None
            } else {
                Some(h)
            }
        })
        .collect();

    let mut graph: FxHashMap<InternedPath, Header> =
        FxHashMap::with_capacity_and_hasher(top_level_headers.len(), Default::default());
    let mut diagnostic_bag = DiagnosticBag::new();
    let mut ordered_paths: Vec<InternedPath> = Vec::with_capacity(top_level_headers.len());

    // Build graph
    for header in top_level_headers {
        header_log!(header);
        ordered_paths.push(header.tokens.src_path.to_owned());
        graph.insert(header.tokens.src_path.to_owned(), header);
    }

    let order_lookup = ordered_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.to_owned(), index))
        .collect::<FxHashMap<_, _>>();

    // Perform topological sort on header-provided dependency edges.
    let mut tracker = DependencyTracker::new(graph.len());
    let mut sorted: Vec<Header> = Vec::with_capacity(graph.len());

    let facade_exports = &module_symbols.facade_exports;
    for path in &ordered_paths {
        if !tracker.visited.contains(path)
            && let Err(error) = visit_node(
                path,
                &mut tracker,
                &graph,
                &order_lookup,
                &mut sorted,
                facade_exports,
                string_table,
            )
        {
            diagnostic_bag.push(error);
        }
    }

    if diagnostic_bag.has_errors() {
        return Err(diagnostic_bag);
    }

    // Append start headers after the sorted top-level headers.
    // WHY: start sees all declarations and cannot be imported, so it must come last.
    sorted.extend(start_headers);

    // Build the complete sorted declaration placeholder list from the topologically
    // ordered headers and append builtins.
    // WHY: declarations must be in sorted order so AST passes see dependencies before dependents.
    module_symbols.build_sorted_declarations(&sorted, string_table);

    Ok(SortedHeaders {
        headers: sorted,
        top_level_const_fragments,
        entry_runtime_fragment_count,
        module_symbols,
        import_environment,
    })
}

/// DFS visit for one module node, pushing clones into `sorted`.
#[allow(clippy::result_large_err)]
fn visit_node(
    node_path: &InternedPath,
    tracker: &mut DependencyTracker,
    graph: &FxHashMap<InternedPath, Header>,
    order_lookup: &FxHashMap<InternedPath, usize>,
    sorted: &mut Vec<Header>,
    facade_exports: &FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let Some(resolved_path) = resolve_graph_path(node_path, graph, facade_exports, string_table)
    else {
        return Err(CompilerDiagnostic::missing_import_target(
            node_path.to_owned(),
            SourceLocation::default(),
        ));
    };

    // cycle?
    if tracker.temp_mark.contains(&resolved_path) {
        return Err(CompilerDiagnostic::circular_dependency(
            resolved_path,
            SourceLocation::default(),
        ));
    }

    // only proceed if not already permanently marked
    if !tracker.visited.contains(&resolved_path) {
        let Some(header) = graph.get(&resolved_path) else {
            // Source-library public import paths can resolve to facade-declared symbols whose
            // authored header path differs from the public prefix path. They have no additional
            // ordering edge here beyond the facade entry itself.
            if is_source_library_facade_export_path(&resolved_path, facade_exports, string_table) {
                tracker.visited.insert(resolved_path);
                return Ok(());
            }
            return Err(CompilerError::new(
                format!(
                    "Dependency ordering resolved '{}' but it was missing from the graph.",
                    resolved_path.to_portable_string(string_table)
                ),
                SourceLocation::default(),
                ErrorType::Compiler,
            )
            .into());
        };

        // mark temporarily
        tracker.temp_mark.insert(resolved_path.to_owned());

        // Recurse on header-provided dependency edges.
        // WHY: edges include type surfaces and constant initializer references.
        // Executable body references are excluded.
        let mut strict_imports = header.dependencies.iter().cloned().collect::<Vec<_>>();
        strict_imports.sort_by(|left, right| {
            let left_order = resolve_graph_path(left, graph, facade_exports, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);
            let right_order = resolve_graph_path(right, graph, facade_exports, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);

            left_order.cmp(&right_order).then_with(|| {
                left.to_portable_string(string_table)
                    .cmp(&right.to_portable_string(string_table))
            })
        });

        for import in strict_imports {
            if resolve_graph_path(&import, graph, facade_exports, string_table).is_none()
                && is_same_file_symbol_hint(&import, &header.source_file)
            {
                // Same-file named-type edges are only ordering hints while header parsing is
                // still discovering the file. If the target never materializes as a header, let
                // later type resolution emit the user-facing "Unknown type" diagnostic.
                continue;
            }

            visit_node(
                &import,
                tracker,
                graph,
                order_lookup,
                sorted,
                facade_exports,
                string_table,
            )?;
        }

        // when children are done, push this node (clone of context)
        sorted.push(header.clone());

        // un-mark temp, mark visited
        tracker.temp_mark.remove(&resolved_path);
        tracker.visited.insert(resolved_path);
    }

    Ok(())
}

/// Resolves a requested dependency path to a graph key.
///
/// WHAT: performs a canonical graph key lookup.
/// WHY: header edge producers (type dependencies, constant initializer dependencies, import
/// dependencies) all emit canonical paths, so only exact graph membership and the facade
/// fallback are needed.
fn resolve_graph_path(
    requested_path: &InternedPath,
    graph: &FxHashMap<InternedPath, Header>,
    facade_exports: &FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    string_table: &StringTable,
) -> Option<InternedPath> {
    if graph.contains_key(requested_path) {
        return Some(requested_path.to_owned());
    }

    // Facade fallback: source-library imports can use a public prefix path that differs from the
    // concrete `#mod.bst` header path. Return the path so visit_node can skip the public edge.
    if is_source_library_facade_export_path(requested_path, facade_exports, string_table) {
        return Some(requested_path.to_owned());
    }

    None
}

/// Checks whether a path refers to a symbol exported by a module facade.
///
/// WHAT: source-library facade files declare symbols that are visible through the library prefix.
/// WHY: dependency edges can use the public prefix spelling even when the concrete graph key is
/// the authored `#mod.bst` declaration path, so these public edges are accepted as facade edges
/// rather than reported as missing imports.
fn is_source_library_facade_export_path(
    path: &InternedPath,
    facade_exports: &FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    string_table: &StringTable,
) -> bool {
    let components = path.as_components();
    if components.is_empty() {
        return false;
    }

    let first_component = string_table.resolve(components[0]);
    let Some(export_name) = path.name() else {
        return false;
    };

    if let Some(entries) = facade_exports.get(first_component) {
        for entry in entries {
            if entry.export_name == export_name {
                return true;
            }
        }
    }

    false
}

fn is_same_file_symbol_hint(path: &InternedPath, source_file: &InternedPath) -> bool {
    path.parent().as_ref() == Some(source_file)
}

#[cfg(test)]
#[path = "tests/module_dependencies_tests.rs"]
mod module_dependencies_tests;
