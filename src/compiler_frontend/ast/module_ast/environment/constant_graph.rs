//! Constant dependency ordering for AST environment construction.
//!
//! WHAT: builds a lightweight graph from header-owned constant initializer reference hints.
//! WHY: constants can then be resolved once in dependency order. Expression parsing remains the
//! semantic authority.

use super::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::declaration_shell::InitializerReference;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use rustc_hash::{FxHashMap, FxHashSet};

pub(in crate::compiler_frontend::ast) struct ConstantDependencyGraph<'headers> {
    nodes: FxHashMap<InternedPath, ConstantNode<'headers>>,
    declaration_order: FxHashMap<InternedPath, usize>,
    constants_by_name: FxHashMap<StringId, Vec<InternedPath>>,
}

struct ConstantNode<'headers> {
    header: &'headers Header,
    source_order: usize,
    dependencies: FxHashSet<InternedPath>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
}

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    pub(in crate::compiler_frontend::ast) fn ordered_constant_headers<'headers>(
        &self,
        sorted_headers: &'headers [Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<Vec<&'headers Header>, CompilerMessages> {
        ConstantDependencyGraph::new(sorted_headers)
            .and_then(|mut graph| {
                graph.build_edges(self, file_import_bindings, string_table)?;
                graph.ordered_headers(string_table)
            })
            .map_err(|error| self.error_messages(error, string_table))
    }
}

impl<'headers> ConstantDependencyGraph<'headers> {
    fn new(sorted_headers: &'headers [Header]) -> Result<Self, CompilerError> {
        let mut nodes = FxHashMap::default();
        let mut declaration_order = FxHashMap::default();
        let mut constants_by_name: FxHashMap<StringId, Vec<InternedPath>> = FxHashMap::default();

        for (index, header) in sorted_headers.iter().enumerate() {
            let HeaderKind::Constant { source_order, .. } = &header.kind else {
                continue;
            };

            let path = header.tokens.src_path.clone();
            if let Some(name) = path.name() {
                constants_by_name
                    .entry(name)
                    .or_default()
                    .push(path.clone());
            }
            declaration_order.insert(path.clone(), index);
            nodes.insert(
                path,
                ConstantNode {
                    header,
                    source_order: *source_order,
                    dependencies: FxHashSet::default(),
                },
            );
        }

        Ok(Self {
            nodes,
            declaration_order,
            constants_by_name,
        })
    }

    fn build_edges(
        &mut self,
        builder: &AstModuleEnvironmentBuilder<'_, '_>,
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        let constant_paths = self.nodes.keys().cloned().collect::<Vec<_>>();

        for constant_path in constant_paths {
            let (header, source_order) = {
                let node = self.nodes.get(&constant_path).ok_or_else(|| {
                    CompilerError::compiler_error(
                        "Constant graph lost a node while building dependency edges.",
                    )
                })?;
                (node.header, node.source_order)
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();

            let HeaderKind::Constant { declaration, .. } = &header.kind else {
                continue;
            };

            for reference in &declaration.initializer_references {
                let Some(dependency) = self.resolve_reference_dependency(
                    &constant_path,
                    header,
                    source_order,
                    reference,
                    &bindings,
                    builder,
                    string_table,
                )?
                else {
                    continue;
                };

                if let Some(node) = self.nodes.get_mut(&constant_path)
                    && node.dependencies.insert(dependency)
                {
                    add_ast_counter(AstCounter::ConstantDependencyEdges, 1);
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_reference_dependency(
        &self,
        constant_path: &InternedPath,
        header: &Header,
        source_order: usize,
        reference: &InitializerReference,
        bindings: &FileImportBindings,
        builder: &AstModuleEnvironmentBuilder<'_, '_>,
        string_table: &mut StringTable,
    ) -> Result<Option<InternedPath>, CompilerError> {
        if let Some(symbol_id) = bindings.visible_external_symbols.get(&reference.name) {
            if matches!(symbol_id, ExternalSymbolId::Constant(_)) {
                return Ok(None);
            }
            return Ok(None);
        }

        if bindings.visible_type_aliases.contains_key(&reference.name) {
            return Ok(None);
        }

        let Some(target_path) = bindings.visible_source_bindings.get(&reference.name) else {
            if self.constants_by_name.contains_key(&reference.name) {
                return Err(not_visible_constant_error(reference, string_table));
            }
            return Err(unknown_constant_error(reference, string_table));
        };

        if !self.nodes.contains_key(target_path) {
            if reference_allowed_as_nominal_constructor(reference, target_path, builder) {
                return Ok(None);
            }
            return Err(non_constant_reference_error(reference, string_table));
        }

        let target_node = self.nodes.get(target_path).ok_or_else(|| {
            CompilerError::compiler_error("Constant graph dependency target was missing.")
        })?;

        if target_node.header.source_file == header.source_file {
            if target_path == constant_path {
                return Ok(Some(target_path.clone()));
            }
            if target_node.source_order > source_order {
                return Err(same_file_forward_reference_error(
                    constant_path,
                    target_path,
                    reference,
                    string_table,
                ));
            }
        }

        Ok(Some(target_path.clone()))
    }

    fn ordered_headers(
        &self,
        string_table: &mut StringTable,
    ) -> Result<Vec<&'headers Header>, CompilerError> {
        increment_ast_counter(AstCounter::ConstantTopologicalSortCount);

        let mut states = FxHashMap::default();
        let mut ordered_paths = Vec::with_capacity(self.nodes.len());
        let mut paths = self.nodes.keys().cloned().collect::<Vec<_>>();
        paths.sort_by_key(|path| {
            self.declaration_order
                .get(path)
                .copied()
                .unwrap_or(usize::MAX)
        });

        for path in paths {
            self.visit(&path, &mut states, &mut ordered_paths, string_table)?;
        }

        ordered_paths
            .iter()
            .map(|path| {
                self.nodes.get(path).map(|node| node.header).ok_or_else(|| {
                    CompilerError::compiler_error(
                        "Constant graph produced an ordered path without a node.",
                    )
                })
            })
            .collect()
    }

    fn visit(
        &self,
        path: &InternedPath,
        states: &mut FxHashMap<InternedPath, VisitState>,
        ordered_paths: &mut Vec<InternedPath>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        match states.get(path) {
            Some(VisitState::Visited) => return Ok(()),
            Some(VisitState::Visiting) => {
                let location = self
                    .nodes
                    .get(path)
                    .map(|node| node.header.name_location.clone())
                    .unwrap_or_default();
                return Err(constant_cycle_error(path, location, string_table));
            }
            None => {}
        }

        states.insert(path.clone(), VisitState::Visiting);

        let node = self.nodes.get(path).ok_or_else(|| {
            CompilerError::compiler_error("Constant graph visit reached a missing node.")
        })?;
        let mut dependencies = node.dependencies.iter().cloned().collect::<Vec<_>>();
        dependencies.sort_by_key(|path| {
            self.declaration_order
                .get(path)
                .copied()
                .unwrap_or(usize::MAX)
        });

        for dependency in dependencies {
            self.visit(&dependency, states, ordered_paths, string_table)?;
        }

        states.insert(path.clone(), VisitState::Visited);
        ordered_paths.push(path.clone());

        Ok(())
    }
}

fn reference_allowed_as_nominal_constructor(
    reference: &InitializerReference,
    target_path: &InternedPath,
    builder: &AstModuleEnvironmentBuilder<'_, '_>,
) -> bool {
    let Some(declaration) = builder.declaration_table.get_by_path(target_path) else {
        return false;
    };

    matches!(
        (
            &declaration.value.data_type,
            reference.followed_by_call,
            reference.followed_by_choice_namespace
        ),
        (DataType::Struct { .. }, true, _) | (DataType::Choices { .. }, _, true)
    )
}

fn unknown_constant_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!("Unknown constant reference '{name}' in constant initializer."),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "AST Constant Dependency Graph".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Declare the constant before use in this file, or import an exported constant with this name.".into(),
    );
    error
}

fn not_visible_constant_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!("Constant '{name}' is not visible in this file."),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "AST Constant Dependency Graph".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Import the exported constant before using it in this constant initializer.".into(),
    );
    error
}

fn non_constant_reference_error(
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let name = string_table.resolve(reference.name).to_owned();
    let mut error = CompilerError::new_rule_error(
        format!(
            "Constants can only reference other constants. '{name}' resolves to a non-constant value."
        ),
        reference.location.clone(),
    );
    error.new_metadata_entry(ErrorMetaDataKey::VariableName, name);
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "AST Constant Dependency Graph".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Only reference constants in constant declarations and const templates.".into(),
    );
    error
}

fn same_file_forward_reference_error(
    constant_path: &InternedPath,
    target_path: &InternedPath,
    reference: &InitializerReference,
    string_table: &StringTable,
) -> CompilerError {
    let current_name = constant_path.name_str(string_table).unwrap_or("<constant>");
    let target_name = target_path.name_str(string_table).unwrap_or("<constant>");
    let mut error = CompilerError::new_rule_error(
        format!(
            "Constant '{current_name}' cannot reference same-file constant '{target_name}' before it is declared."
        ),
        reference.location.clone(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "AST Constant Dependency Graph".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Move the referenced constant above this declaration, or import it from another file."
            .into(),
    );
    error
}

fn constant_cycle_error(
    path: &InternedPath,
    location: crate::compiler_frontend::compiler_errors::SourceLocation,
    string_table: &StringTable,
) -> CompilerError {
    let path_string = path.to_portable_string(string_table);
    let mut error = CompilerError::new_rule_error(
        format!("Constant cycle detected involving '{path_string}'."),
        location,
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        "AST Constant Dependency Graph".into(),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        "Break the constant cycle by making one value independent or computing it at runtime."
            .into(),
    );
    error
}
