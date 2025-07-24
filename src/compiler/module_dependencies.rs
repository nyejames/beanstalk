use crate::compiler::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::{TextLocation, TokenContext};
use crate::return_rule_error;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// Helper struct to track module dependencies
pub struct ModuleDependencies<'a> {
    graph: HashMap<PathBuf, TokenContext>, // module src, module
    visited: HashSet<&'a Path>,
    temp_mark: HashSet<&'a Path>,
    sorted: Vec<TokenContext>,
}

impl ModuleDependencies<'_> {
    // Creates a graph of which modules are requesting imports from other modules
    fn new(tokenized_modules: Vec<Result<TokenContext, CompileError>>) -> Result<Self, Vec<CompileError>> {
        // Build dependency graph
        // And remove any errored modules
        let number_of_modules = tokenized_modules.len();

        let mut graph: HashMap<PathBuf, TokenContext> = HashMap::with_capacity(number_of_modules);
        let mut errors: Vec<CompileError> = Vec::new();
        for module in tokenized_modules {
            match module {
                Ok(module) => {
                    graph.insert(module.src_path.to_owned(), module);
                }
                Err(e) => {
                    errors.push(e)
                }
            }
        }

        Ok(ModuleDependencies {
            graph,
            visited: HashSet::with_capacity(number_of_modules),
            temp_mark: HashSet::with_capacity(number_of_modules),
            sorted: Vec::with_capacity(number_of_modules),
        })
    }

    // Topological sort
    fn sort(mut self) -> Result<Vec<TokenContext>, Vec<CompileError>> {
        let mut errors = Vec::new();

        for (path, module) in self.graph.iter_mut() {
            if !self.visited.contains(&path) {
                match self.visit_node(module.to_owned()) {
                    Ok(_) => {}
                    Err(e) => {
                        errors.push(e);
                    }
                }
            }
        }

        Ok(self.sorted)
    }

    // Depth-first search for a single node
    fn visit_node(&mut self, module: TokenContext) -> Result<(), CompileError> {
        let node_path = &module.src_path;
        if self.temp_mark.contains(&node_path) {
            return_rule_error!(
                TextLocation::default(),

                // TODO: More detail for how to circumvent this
                "Circular dependency detected inside: {}",
                node_path.to_str().unwrap()
            )
        }

        if !self.visited.contains(node_path) {
            self.temp_mark.insert(node_path);

            if let Some(deps) = self.graph.get(&node_path) {
                for dep in deps {
                    self.visit_node(dep)?;
                }
            }

            self.temp_mark.remove(&node_path);
            self.visited.insert(&node_path);
            self.sorted.push(module.to_owned());
        }

        Ok(())
    }
}

pub fn resolve_module_dependencies(
    modules: Vec<Result<TokenContext, CompileError>>,
) -> Result<Vec<TokenContext>, Vec<CompileError>> {

    // First build dependency graph and get sorted order
    let deps = match ModuleDependencies::new(modules) {
        Ok(mods) => mods,
        Err(errors) => return Err(errors),
    };

    let sorted_modules = deps.sort()?;

    Ok(sorted_modules)
}
