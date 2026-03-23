//! Stage 3 dependency ordering for parsed Beanstalk headers.
//!
//! This pass topologically sorts header definitions after header parsing so AST construction sees
//! import, constant, and soft struct-default dependencies in a deterministic order.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::{header_log, return_rule_error};
use std::collections::{HashMap, HashSet};

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

/// Given a list of, possibly errored, `TokenContext`
/// builds a graph of successful ones and topologically sorts them.
pub fn resolve_module_dependencies(
    headers: Vec<Header>,
    string_table: &mut StringTable,
) -> Result<Vec<Header>, Vec<CompilerError>> {
    let mut graph: HashMap<InternedPath, Header> = HashMap::with_capacity(headers.len());
    let mut errors: Vec<CompilerError> = Vec::with_capacity(headers.len());
    let mut ordered_paths: Vec<InternedPath> = Vec::with_capacity(headers.len());

    // Build graph or collect errors
    for header in headers {
        header_log!(header);
        ordered_paths.push(header.tokens.src_path.to_owned());
        graph.insert(header.tokens.src_path.to_owned(), header);
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let order_lookup = ordered_paths
        .iter()
        .enumerate()
        .map(|(index, path)| (path.to_owned(), index))
        .collect::<HashMap<_, _>>();

    // Perform topological sort
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
        Err(errors)
    } else {
        Ok(sorted)
    }
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
            ErrorLocation::default(),
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
            ErrorLocation::default(),
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

        // recurse on strict imports first
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
            visit_node(&import, tracker, graph, order_lookup, sorted, string_table)?;
        }

        // Soft edges: struct default-value dependencies.
        // WHY: resolved when possible but never fail sorting — AST validation owns those errors.
        if let HeaderKind::Struct { metadata } = &header.kind {
            let soft_edges = collect_struct_default_soft_edges(
                &metadata.default_value_dependencies,
                &resolved_path,
                graph,
                order_lookup,
                string_table,
            );
            for dependency in soft_edges {
                visit_node(&dependency, tracker, graph, order_lookup, sorted, string_table)?;
            }
        }

        // Soft edges: constant symbol dependencies.
        // WHY: same policy as struct defaults — order when resolvable, never block on failure.
        if let HeaderKind::Constant { metadata } = &header.kind {
            let soft_edges = collect_constant_symbol_soft_edges(
                &metadata.symbol_dependencies,
                &resolved_path,
                graph,
                order_lookup,
                string_table,
            );
            for dependency in soft_edges {
                visit_node(&dependency, tracker, graph, order_lookup, sorted, string_table)?;
            }
        }

        // when children are done, push this node (clone of context)
        sorted.push(header.clone());

        // un-mark temp, mark visited
        tracker.temp_mark.remove(&resolved_path);
        tracker.visited.insert(resolved_path);
    }

    Ok(())
}

/// Collect soft sort edges for a struct's default-expression dependencies.
///
/// WHY: only Constant headers are valid soft targets here; unresolved candidates are silently
/// ignored so dependency sorting never fails on a missing struct default dependency.
fn collect_struct_default_soft_edges(
    dependencies: &HashSet<InternedPath>,
    resolved_self: &InternedPath,
    graph: &HashMap<InternedPath, Header>,
    order_lookup: &HashMap<InternedPath, usize>,
    string_table: &StringTable,
) -> Vec<InternedPath> {
    let mut soft_edges = dependencies
        .iter()
        .filter_map(|dep| resolve_graph_path(dep, graph, string_table))
        .filter(|dep| {
            matches!(
                graph.get(dep).map(|h| &h.kind),
                Some(HeaderKind::Constant { .. })
            )
        })
        .filter(|dep| dep != resolved_self)
        .collect::<Vec<_>>();
    sort_and_dedup_soft_edges(&mut soft_edges, order_lookup, string_table);
    soft_edges
}

/// Collect soft sort edges for a constant declaration's symbol dependencies.
///
/// WHY: Struct and Constant headers are valid soft targets; unresolved candidates are silently
/// ignored so dependency sorting never fails on a missing constant symbol reference.
fn collect_constant_symbol_soft_edges(
    dependencies: &HashSet<InternedPath>,
    resolved_self: &InternedPath,
    graph: &HashMap<InternedPath, Header>,
    order_lookup: &HashMap<InternedPath, usize>,
    string_table: &StringTable,
) -> Vec<InternedPath> {
    let mut soft_edges = dependencies
        .iter()
        .filter_map(|dep| resolve_graph_path(dep, graph, string_table))
        .filter(|dep| {
            matches!(
                graph.get(dep).map(|h| &h.kind),
                Some(HeaderKind::Struct { .. }) | Some(HeaderKind::Constant { .. })
            )
        })
        .filter(|dep| dep != resolved_self)
        .collect::<Vec<_>>();
    sort_and_dedup_soft_edges(&mut soft_edges, order_lookup, string_table);
    soft_edges
}

fn sort_and_dedup_soft_edges(
    edges: &mut Vec<InternedPath>,
    order_lookup: &HashMap<InternedPath, usize>,
    string_table: &StringTable,
) {
    edges.sort_by(|left, right| {
        let left_order = order_lookup.get(left).copied().unwrap_or(usize::MAX);
        let right_order = order_lookup.get(right).copied().unwrap_or(usize::MAX);
        left_order.cmp(&right_order).then_with(|| {
            left.to_portable_string(string_table)
                .cmp(&right.to_portable_string(string_table))
        })
    });
    edges.dedup();
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
    candidate_components: &[crate::compiler_frontend::string_interning::StringId],
    requested_components: &[crate::compiler_frontend::string_interning::StringId],
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
