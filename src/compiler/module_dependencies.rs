use crate::compiler::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::{TextLocation, TokenContext};
use crate::return_rule_error;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Tracks which modules are temporarily marked (in the current DFS stack)
/// and which have been permanently visited.
struct DependencyTracker {
    temp_mark: HashSet<PathBuf>,
    visited: HashSet<PathBuf>,
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
pub fn resolve_module_dependencies(
    modules: Vec<Result<TokenContext, CompileError>>,
) -> Result<Vec<TokenContext>, Vec<CompileError>> {
    let mut graph: HashMap<PathBuf, TokenContext> = HashMap::with_capacity(modules.len());
    let mut errs: Vec<CompileError> = Vec::new();

    // Build graph or collect errors
    for m in modules {
        match m {
            Ok(ctx) => {
                graph.insert(ctx.src_path.clone(), ctx);
            }
            Err(e) => {
                errs.push(e);
            }
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    // Perform topological sort
    let mut tracker = DependencyTracker::new(graph.len());
    let mut sorted: Vec<TokenContext> = Vec::with_capacity(graph.len());
    let mut cycle_errs: Vec<CompileError> = Vec::new();

    for path in graph.keys() {
        if !tracker.visited.contains(path) {
            if let Err(e) = visit_node(path, &mut tracker, &graph, &mut sorted) {
                cycle_errs.push(e);
            }
        }
    }

    if !cycle_errs.is_empty() {
        Err(cycle_errs)
    } else {
        Ok(sorted)
    }
}

/// DFS visit for one module node, pushing clones into `sorted`.
fn visit_node(
    node_path: &Path,
    tracker: &mut DependencyTracker,
    graph: &HashMap<PathBuf, TokenContext>,
    sorted: &mut Vec<TokenContext>,
) -> Result<(), CompileError> {
    // cycle?
    if tracker.temp_mark.contains(node_path) {
        return_rule_error!(
            TextLocation::default(),
            "Circular dependency detected at {}",
            node_path.display()
        )
    }

    // only proceed if not already permanently marked
    if !tracker.visited.contains(node_path) {
        // mark temporarily
        tracker.temp_mark.insert(node_path.to_path_buf());

        // recurse on all imports
        if let Some(ctx) = graph.get(node_path) {
            for import in &ctx.imports {
                visit_node(import, tracker, graph, sorted)?;
            }
            // when children are done, push this node (clone of context)
            sorted.push(ctx.clone());
        }

        // un-mark temp, mark visited
        tracker.temp_mark.remove(node_path);
        tracker.visited.insert(node_path.to_path_buf());
    }

    Ok(())
}
