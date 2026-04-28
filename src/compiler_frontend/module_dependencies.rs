//! Stage 3 dependency ordering for parsed Beanstalk headers.
//!
//! WHAT: topologically sorts top-level declaration headers by their strict dependency edges, then
//! appends `StartFunction` headers in source order. Finalizes the header-owned `ModuleSymbols` package:
//! declarations are built from the sorted headers and appended with builtin declarations.
//!
//! ## Stage contract
//!
//! **Strict-edges-only sort.** Dependency edges are structural: function signature type refs,
//! struct field type refs, constant declared-type refs. Initializer-expression symbols are NOT
//! edges; they are soft hints handled at AST body-parsing time.
//!
//! **`start` excluded from the graph.** `StartFunction` headers are not graph participants — they
//! have no dependents and cannot be imported. They are appended after the sorted top-level
//! headers so AST emission sees all top-level declarations before processing the start body.

use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::{
    Header, HeaderKind, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::source_libraries::mod_file::path_is_mod_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::{header_log, return_rule_error};
use std::collections::{HashMap, HashSet};

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
}

/// Tracks which modules are temporarily marked (in the current DFS stack)
/// and which have been permanently visited.
struct DependencyTracker {
    temp_mark: HashSet<InternedPath>,
    visited: HashSet<InternedPath>,
}

impl DependencyTracker {
    fn new(capacity: usize) -> Self {
        DependencyTracker {
            temp_mark: HashSet::with_capacity(capacity),
            visited: HashSet::with_capacity(capacity),
        }
    }
}

/// Topologically sort headers and finalize the header-owned module symbol package.
///
/// WHAT: sorts top-level declaration headers (non-start) by their strict dependency edges, then
/// appends `StartFunction` headers in source order. Builds the `declarations` Vec in sorted order
/// and appends builtin declarations.
///
/// WHY: `StartFunction` is excluded from the dependency graph — it is build-system-only and
/// cannot be imported by other headers. All other headers are sorted by strict structural edges
/// (signature types, field types, declared constant types) so AST sees dependencies first.
pub fn resolve_module_dependencies(
    parsed: Headers,
    string_table: &mut StringTable,
) -> Result<SortedHeaders, Vec<CompilerError>> {
    let Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count,
        mut module_symbols,
    } = parsed;

    // Partition: StartFunction and facade (#mod.bst) headers are appended last, not sorted.
    // WHY: start is build-system-only and has no dependents; facades only consume dependencies
    // from other files and do not expose symbols to the rest of the module, so they must not
    // participate in cycle detection or strict-edge traversal.
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

    let mut graph: HashMap<InternedPath, Header> = HashMap::with_capacity(top_level_headers.len());
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
        .collect::<HashMap<_, _>>();

    // Perform topological sort on strict-edge graph only.
    let mut tracker = DependencyTracker::new(graph.len());
    let mut sorted: Vec<Header> = Vec::with_capacity(graph.len());

    for path in &ordered_paths {
        if !tracker.visited.contains(path)
            && let Err(error) = visit_node(
                path,
                &mut tracker,
                &graph,
                &order_lookup,
                &mut sorted,
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
    })
}

/// DFS visit for one module node, pushing clones into `sorted`.
fn visit_node(
    node_path: &InternedPath,
    tracker: &mut DependencyTracker,
    graph: &HashMap<InternedPath, Header>,
    order_lookup: &HashMap<InternedPath, usize>,
    sorted: &mut Vec<Header>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let Some(resolved_path) = resolve_graph_path(node_path, graph, string_table) else {
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
            format!("Circular dependency detected at {}", path_str),
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
            return Err(CompilerError::compiler_error(format!(
                "Dependency ordering resolved '{}' but it was missing from the graph.",
                resolved_path.to_portable_string(string_table)
            )));
        };

        // mark temporarily
        tracker.temp_mark.insert(resolved_path.to_owned());

        // Recurse on strict dependency edges only.
        // WHY: strict edges are structural (signature types, field types, declared constant types).
        // Body / initializer expression references are soft and excluded from the graph.
        let mut strict_imports = header.dependencies.iter().cloned().collect::<Vec<_>>();
        strict_imports.sort_by(|left, right| {
            let left_order = resolve_graph_path(left, graph, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);
            let right_order = resolve_graph_path(right, graph, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);

            left_order.cmp(&right_order).then_with(|| {
                left.to_portable_string(string_table)
                    .cmp(&right.to_portable_string(string_table))
            })
        });

        for import in strict_imports {
            if resolve_graph_path(&import, graph, string_table).is_none()
                && is_same_file_symbol_hint(&import, &header.source_file)
            {
                // Same-file named-type edges are only ordering hints while header parsing is
                // still discovering the file. If the target never materializes as a header, let
                // later type resolution emit the user-facing "Unknown type" diagnostic.
                continue;
            }

            visit_node(&import, tracker, graph, order_lookup, sorted, string_table)?;
        }

        // when children are done, push this node (clone of context)
        sorted.push(header.clone());

        // un-mark temp, mark visited
        tracker.temp_mark.remove(&resolved_path);
        tracker.visited.insert(resolved_path);
    }

    Ok(())
}

fn resolve_graph_path(
    requested_path: &InternedPath,
    graph: &HashMap<InternedPath, Header>,
    string_table: &StringTable,
) -> Option<InternedPath> {
    if graph.contains_key(requested_path) {
        return Some(requested_path.to_owned());
    }

    let normalized_requested = normalize_relative_dependency_path(requested_path, string_table);

    let exact_header_path_matches = graph
        .keys()
        .filter(|candidate| {
            exact_path_matches_candidate(candidate, requested_path, string_table)
                || normalized_requested.as_ref().is_some_and(|normalized| {
                    exact_path_matches_candidate(candidate, normalized, string_table)
                })
        })
        .cloned()
        .collect::<Vec<_>>();

    match exact_header_path_matches.as_slice() {
        [single] => return Some(single.to_owned()),
        [] => {}
        _ => return None,
    }

    let header_path_matches = graph
        .keys()
        .filter(|candidate| {
            path_matches_candidate(candidate, requested_path, string_table)
                || normalized_requested.as_ref().is_some_and(|normalized| {
                    path_matches_candidate(candidate, normalized, string_table)
                })
        })
        .cloned()
        .collect::<Vec<_>>();

    match header_path_matches.as_slice() {
        [single] => return Some(single.to_owned()),
        [] => {}
        _ => return None,
    }

    let mut exact_source_file_matches = graph
        .iter()
        .filter(|(_, header)| {
            exact_path_matches_candidate(&header.source_file, requested_path, string_table)
                || normalized_requested.as_ref().is_some_and(|normalized| {
                    exact_path_matches_candidate(&header.source_file, normalized, string_table)
                })
        })
        .map(|(path, header)| {
            (
                path.to_owned(),
                matches!(header.kind, HeaderKind::StartFunction),
            )
        })
        .collect::<Vec<_>>();

    if !exact_source_file_matches.is_empty() {
        let start_function_matches = exact_source_file_matches
            .iter()
            .filter(|(_, is_start)| *is_start)
            .map(|(path, _)| path.to_owned())
            .collect::<Vec<_>>();

        if start_function_matches.len() == 1 {
            return Some(start_function_matches[0].to_owned());
        }
        if start_function_matches.len() > 1 {
            return None;
        }

        if exact_source_file_matches.len() == 1 {
            return exact_source_file_matches.pop().map(|(path, _)| path);
        }

        return None;
    }

    let mut source_file_matches = graph
        .iter()
        .filter(|(_, header)| {
            path_matches_candidate(&header.source_file, requested_path, string_table)
                || normalized_requested.as_ref().is_some_and(|normalized| {
                    path_matches_candidate(&header.source_file, normalized, string_table)
                })
        })
        .map(|(path, header)| {
            (
                path.to_owned(),
                matches!(header.kind, HeaderKind::StartFunction),
            )
        })
        .collect::<Vec<_>>();

    if source_file_matches.is_empty() {
        return None;
    }

    let start_function_matches = source_file_matches
        .iter()
        .filter(|(_, is_start)| *is_start)
        .map(|(path, _)| path.to_owned())
        .collect::<Vec<_>>();

    if start_function_matches.len() == 1 {
        return Some(start_function_matches[0].to_owned());
    }
    if start_function_matches.len() > 1 {
        return None;
    }

    if source_file_matches.len() == 1 {
        return source_file_matches.pop().map(|(path, _)| path);
    }

    None
}

fn is_same_file_symbol_hint(path: &InternedPath, source_file: &InternedPath) -> bool {
    path.parent().as_ref() == Some(source_file)
}

/// Checks whether a header belongs to a library facade (`#mod.bst`).
///
/// WHAT: facade files only consume dependencies from other module files and do not expose
/// symbols to the rest of the module, so they should be excluded from dependency sorting.
fn is_facade_header(header: &Header, string_table: &StringTable) -> bool {
    path_is_mod_file(&header.source_file, string_table)
}

fn exact_path_matches_candidate(
    candidate: &InternedPath,
    requested: &InternedPath,
    string_table: &StringTable,
) -> bool {
    components_match_with_optional_bst_extension(
        candidate.as_components(),
        requested.as_components(),
        string_table,
    )
}

fn path_matches_candidate(
    candidate: &InternedPath,
    requested: &InternedPath,
    string_table: &StringTable,
) -> bool {
    candidate.ends_with(requested)
        || suffix_matches_with_optional_bst_extension(candidate, requested, string_table)
}

fn normalize_relative_dependency_path(
    requested: &InternedPath,
    string_table: &StringTable,
) -> Option<InternedPath> {
    let components = requested.as_components();
    let mut start = 0usize;

    while start < components.len() {
        let segment = string_table.resolve(components[start]);
        if segment == "." || segment == ".." {
            start += 1;
            continue;
        }
        break;
    }

    if start == 0 || start >= components.len() {
        return None;
    }

    Some(InternedPath::from_components(components[start..].to_vec()))
}

fn suffix_matches_with_optional_bst_extension(
    candidate: &InternedPath,
    requested: &InternedPath,
    string_table: &StringTable,
) -> bool {
    if requested.len() > candidate.len() {
        return false;
    }

    let candidate_components = candidate.as_components();
    let requested_components = requested.as_components();
    let start_index = candidate_components.len() - requested_components.len();

    components_match_with_optional_bst_extension(
        &candidate_components[start_index..],
        requested_components,
        string_table,
    )
}

fn components_match_with_optional_bst_extension(
    candidate_components: &[crate::compiler_frontend::symbols::string_interning::StringId],
    requested_components: &[crate::compiler_frontend::symbols::string_interning::StringId],
    string_table: &StringTable,
) -> bool {
    if candidate_components.len() != requested_components.len() {
        return false;
    }

    candidate_components
        .iter()
        .zip(requested_components.iter())
        .all(|(candidate_component, requested_component)| {
            if candidate_component == requested_component {
                return true;
            }

            let candidate_str = string_table.resolve(*candidate_component);
            let requested_str = string_table.resolve(*requested_component);

            candidate_str.strip_suffix(".bst") == Some(requested_str)
                || requested_str.strip_suffix(".bst") == Some(candidate_str)
        })
}

#[cfg(test)]
#[path = "tests/module_dependencies_tests.rs"]
mod module_dependencies_tests;
