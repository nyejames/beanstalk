// CURRENTLY REMOVED FROM COMPILER
// Dependency resolution will have to happen at the module level.
// Declarations will be parsed first in the tokenizer now.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::headers::parse_file_headers::Header;
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
        if !tracker.visited.contains(path) {
            if let Err(e) = visit_node(
                path,
                &mut tracker,
                &graph,
                &order_lookup,
                &mut sorted,
                string_table,
            ) {
                errors.push(e);
            }
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
                node_path.to_string(string_table)
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
        let path_str: &'static str =
            Box::leak(resolved_path.to_string(string_table).into_boxed_str());
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
        let header = graph
            .get(&resolved_path)
            .expect("Resolved dependency path should exist in graph");

        // mark temporarily
        tracker.temp_mark.insert(resolved_path.to_owned());

        // recurse on all imports
        let mut imports = header.dependencies.iter().cloned().collect::<Vec<_>>();
        imports.sort_by(|left, right| {
            let left_order = resolve_graph_path(left, graph, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);
            let right_order = resolve_graph_path(right, graph, string_table)
                .and_then(|path| order_lookup.get(&path).copied())
                .unwrap_or(usize::MAX);

            left_order.cmp(&right_order).then_with(|| {
                left.to_string(string_table)
                    .cmp(&right.to_string(string_table))
            })
        });

        for import in imports {
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

    let mut matches = graph.keys().filter(|candidate| {
        candidate.ends_with(requested_path)
            || suffix_matches_with_optional_bst_extension(candidate, requested_path, string_table)
    });

    let first_match = matches.next()?.to_owned();
    if matches.next().is_some() {
        return None;
    }

    Some(first_match)
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

    candidate_components[start_index..]
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
