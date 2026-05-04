//! Stage 3 dependency ordering for parsed Beanstalk headers.
//!
//! WHAT: topologically sorts top-level declaration headers by their header-provided dependency edges,
//! then appends `StartFunction` headers in source order. Finalizes the header-owned `ModuleSymbols` package:
//! declarations are built from the sorted headers and appended with builtin declarations.
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

use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::headers::import_environment::HeaderImportEnvironment;
use crate::compiler_frontend::headers::module_symbols::{FacadeExportEntry, ModuleSymbols};
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::source_libraries::mod_file::path_is_mod_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::{header_log, return_rule_error};
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
) -> Result<SortedHeaders, Vec<CompilerError>> {
    let Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count,
        mut module_symbols,
        import_environment,
    } = parsed;

    // Partition: StartFunction and facade (#mod.bst) headers are appended last, not sorted.
    // WHY: start is build-system-only and has no dependents; facades only consume dependencies
    // from other files and do not expose symbols to the rest of the module, so they must not
    // participate in cycle detection or dependency-edge traversal.
    let mut facade_headers: Vec<Header> = Vec::new();
    let mut start_headers: Vec<Header> = Vec::new();
    let top_level_headers: Vec<Header> = headers
        .into_iter()
        .filter_map(|h| {
            if matches!(h.kind, HeaderKind::StartFunction) {
                start_headers.push(h);
                None
            } else if is_facade_header(&h, string_table) {
                facade_headers.push(h);
                None
            } else {
                Some(h)
            }
        })
        .collect();

    let mut graph: FxHashMap<InternedPath, Header> =
        FxHashMap::with_capacity_and_hasher(top_level_headers.len(), Default::default());
    let mut errors: Vec<CompilerError> = Vec::with_capacity(top_level_headers.len());
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
            errors.push(error);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Append facade headers and start headers after the sorted top-level headers.
    // WHY: facades only consume dependencies and start sees all declarations, so both must come last.
    sorted.extend(facade_headers);
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
fn visit_node(
    node_path: &InternedPath,
    tracker: &mut DependencyTracker,
    graph: &FxHashMap<InternedPath, Header>,
    order_lookup: &FxHashMap<InternedPath, usize>,
    sorted: &mut Vec<Header>,
    facade_exports: &FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let Some(resolved_path) = resolve_graph_path(node_path, graph, facade_exports, string_table)
    else {
        return_rule_error!(
            format!(
                "Missing import target '{}'. Could not resolve this dependency in the current module.",
                node_path.to_portable_string(string_table)
            ),
            SourceLocation::default(),
            {
                CompilationStage => "Dependency Resolution",
                PrimarySuggestion => "Import an existing file/symbol path or fix the import path spelling",
            }
        );
    };

    // cycle?
    if tracker.temp_mark.contains(&resolved_path) {
        let path_str = resolved_path.to_portable_string(string_table);
        return_rule_error!(
            format!("Circular declaration dependency detected at {}", path_str),
            SourceLocation::default(),
            {
                CompilationStage => "Dependency Resolution",
                PrimarySuggestion => "Refactor shared code into a separate module to break the cycle",
                SuggestedLocation => path_str,
            }
        )
    }

    // only proceed if not already permanently marked
    if !tracker.visited.contains(&resolved_path) {
        let Some(header) = graph.get(&resolved_path) else {
            // Facade-declared symbols are resolved by import binding but excluded from the graph.
            // They have no inter-file strict dependencies that affect ordering, so skip safely.
            if is_facade_path(&resolved_path, facade_exports, string_table) {
                tracker.visited.insert(resolved_path);
                return Ok(());
            }
            return Err(CompilerError::compiler_error(format!(
                "Dependency ordering resolved '{}' but it was missing from the graph.",
                resolved_path.to_portable_string(string_table)
            )));
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

    // Facade fallback: cross-library imports resolve to facade-declared symbols that are
    // excluded from the graph. Return the path so visit_node can skip them safely.
    if is_facade_path(requested_path, facade_exports, string_table) {
        return Some(requested_path.to_owned());
    }

    None
}

/// Checks whether a path refers to a symbol exported by a module facade.
///
/// WHAT: facade files declare symbols that are visible to cross-library importers. When the
/// facade header is excluded from the dependency graph, consumer dependencies on facade symbols
/// still need to resolve successfully so dependency-edge traversal does not fail.
fn is_facade_path(
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

/// Checks whether a header belongs to a module facade (`#mod.bst`).
///
/// WHAT: facade files only consume dependencies from other module files and do not expose
/// symbols to the rest of the module, so they should be excluded from dependency sorting.
fn is_facade_header(header: &Header, string_table: &StringTable) -> bool {
    path_is_mod_file(&header.source_file, string_table)
}

#[cfg(test)]
#[path = "tests/module_dependencies_tests.rs"]
mod module_dependencies_tests;
