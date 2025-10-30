// CURRENTLY REMOVED FROM COMPILER
// Dependency resolution will have to happen at the module level.
// Declarations will be parsed first in the tokenizer now.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::parse_file_headers::Header;
use crate::return_rule_error;
use std::collections::{HashMap, HashSet};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

/// Tracks which modules are temporarily marked (in the current DFS stack)
/// and which have been permanently visited.
struct DependencyTracker {
    temp_mark: HashSet<String>,
    visited: HashSet<String>,
}

impl DependencyTracker {
    fn new(capacity: usize) -> Self {
        DependencyTracker {
            temp_mark: HashSet::with_capacity(capacity),
            visited: HashSet::with_capacity(capacity),
        }
    }
}

/// Given a list of (possibly errored) `TokenContext`
/// builds a graph of successful ones and topologically sorts them.
pub fn resolve_module_dependencies(headers: Vec<Header>) -> Result<Vec<Header>, Vec<CompileError>> {
    let mut graph: HashMap<String, Header> = HashMap::with_capacity(headers.len());
    let mut errors: Vec<CompileError> = Vec::with_capacity(headers.len());

    // Build graph or collect errors
    for header in headers {
        graph.insert(header.name.to_owned(), header);
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Perform topological sort
    let mut tracker = DependencyTracker::new(graph.len());
    let mut sorted: Vec<Header> = Vec::with_capacity(graph.len());

    for name in graph.keys() {
        if !tracker.visited.contains(name) {
            if let Err(e) = visit_node(name, &mut tracker, &graph, &mut sorted) {
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
    node_path: &String,
    tracker: &mut DependencyTracker,
    graph: &HashMap<String, Header>,
    sorted: &mut Vec<Header>,
) -> Result<(), CompileError> {
    // cycle?
    if tracker.temp_mark.contains(node_path) {
        return_rule_error!(
            TextLocation::default(),
            "Circular dependency detected at {}",
            node_path
        )
    }

    // only proceed if not already permanently marked
    if !tracker.visited.contains(node_path) {
        // mark temporarily
        tracker.temp_mark.insert(node_path.to_owned());

        // recurse on all imports
        if let Some(header) = graph.get(node_path) {
            for import in &header.dependencies {
                visit_node(import, tracker, graph, sorted)?;
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
