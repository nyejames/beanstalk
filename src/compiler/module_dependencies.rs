use crate::compiler::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::tokens::{TextLocation, TokenContext};
use crate::return_compiler_error;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// Helper struct to track module dependencies
struct ModuleDependencies {
    graph: HashMap<PathBuf, HashSet<PathBuf>>, // module -> dependencies
    visited: HashSet<PathBuf>,
    temp_mark: HashSet<PathBuf>,
    sorted: Vec<PathBuf>,
}

impl ModuleDependencies {
    // Creates a graph of which modules are requesting imports from other modules
    fn new(tokenized_modules: &[TokenContext]) -> Self {
        // Build dependency graph
        let mut graph: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        for module in tokenized_modules {
            graph.insert(module.src_path.to_owned(), module.imports.to_owned());
        }

        ModuleDependencies {
            graph,
            visited: HashSet::new(),
            temp_mark: HashSet::new(),
            sorted: Vec::new(),
        }
    }

    // Perform topological sort
    fn sort(mut self) -> Result<Vec<PathBuf>, CompileError> {
        let nodes: Vec<_> = self.graph.keys().cloned().collect();
        for node in nodes {
            if !self.visited.contains(&node) {
                self.visit_node(&node)?;
            }
        }
        Ok(self.sorted)
    }

    // Depth-first search for a single node
    fn visit_node(&mut self, node: &PathBuf) -> Result<(), CompileError> {
        if self.temp_mark.contains(node) {
            return_compiler_error!(
                "Circular dependency detected inside: {}",
                node.to_str().unwrap()
            )
        }

        if !self.visited.contains(node) {
            self.temp_mark.insert(node.clone());

            if let Some(deps) = self.graph.get(node).cloned() {
                for dep in deps {
                    self.visit_node(&dep)?;
                }
            }

            self.temp_mark.remove(node);
            self.visited.insert(node.clone());
            self.sorted.push(node.clone());
        }

        Ok(())
    }
}

pub fn resolve_module_dependencies(
    modules: &[TemplateModule],
) -> Result<(Vec<OutputModule>, Vec<Arg>), CompileError> {
    let mut tokenised_modules: Vec<OutputModule> = Vec::new();
    let mut project_exports = Vec::new();

    // First build dependency graph and get sorted order
    let deps = ModuleDependencies::new(modules);
    let sorted_paths = deps.sort()?;

    // Process modules in dependency order
    for path in sorted_paths {
        let module = modules.iter().find(|m| m.source_path == path).unwrap();
        let mut imports = HashMap::new();

        // Validate and collect imports
        for import_path in &module.import_requests {
            // Find the module that exports this import
            for other_module in modules {
                if let Some(tokens) = other_module.exports.get(import_path) {
                    imports.insert(import_path.clone(), tokens.clone());
                    break;
                }
            }
        }

        // Add module's exports to project_exports
        for (export_name, ..) in &module.exports {
            // TODO: Convert tokens to Arg with proper data type inference
            project_exports.push(Arg {
                name: export_name.clone(),
                data_type: DataType::Inferred(false), // TODO: Infer proper type
                value: ExpressionKind::None,
            });
        }

        tokenised_modules.push(OutputModule::new(
            module.output_path.to_owned(),
            module.tokens.to_owned(),
            module.source_path.to_owned(),
        ));
    }

    Ok((tokenised_modules, project_exports))
}
