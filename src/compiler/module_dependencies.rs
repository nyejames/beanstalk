// CURRENTLY REMOVED FROM COMPILER
// Dependency resolution will have to happen at the module level.
// Declarations will be parsed first in the tokenizer now.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::parse_file_headers::Header;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;
use crate::return_rule_error;
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
) -> Result<Vec<Header>, Vec<CompileError>> {
    let mut graph: HashMap<InternedPath, Header> = HashMap::with_capacity(headers.len());
    let mut errors: Vec<CompileError> = Vec::with_capacity(headers.len());

    // Build graph or collect errors
    for header in headers {
        graph.insert(header.path.to_owned(), header);
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Perform topological sort
    let mut tracker = DependencyTracker::new(graph.len());
    let mut sorted: Vec<Header> = Vec::with_capacity(graph.len());

    for path in graph.keys() {
        if !tracker.visited.contains(path) {
            if let Err(e) = visit_node(path, &mut tracker, &graph, &mut sorted, string_table) {
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
    sorted: &mut Vec<Header>,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    // cycle?
    if tracker.temp_mark.contains(node_path) {
        return_rule_error!(
            string_table,
            TextLocation::default(),
            "Circular dependency detected at {}",
            node_path.to_string(string_table),
        )
    }

    // only proceed if not already permanently marked
    if !tracker.visited.contains(node_path) {
        // mark temporarily
        tracker.temp_mark.insert(node_path.to_owned());

        // recurse on all imports
        if let Some(header) = graph.get(node_path) {
            for import in &header.dependencies {
                visit_node(import, tracker, graph, sorted, string_table)?;
            }
            // when children are done, push this node (clone of context)
            sorted.push(header.clone());
        }

        // un-mark temp, mark visited
        tracker.temp_mark.remove(node_path);
        tracker.visited.insert(node_path.to_owned());
    }

    Ok(())
}
