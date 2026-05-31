//! Semantic classification for resolved source declarations.
//!
//! WHAT: records the executable role of each top-level declaration after AST environment
//! construction has resolved signatures and nominal type identity.
//! WHY: body parsing needs to decide whether a visible source declaration is callable,
//! constructible, or value-like without inspecting diagnostic-only `DataType` spelling.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::module_ast::environment::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::interned_path::InternedPath;
use rustc_hash::FxHashMap;

/// Source-level role assigned to a resolved top-level declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclarationSemanticKind {
    Function,
    Struct,
    Choice,
    Constant,
    Value,
}

/// Immutable path-indexed classification table shared by body emission contexts.
#[derive(Debug)]
pub(crate) struct DeclarationSemanticTable {
    by_path: FxHashMap<InternedPath, DeclarationSemanticKind>,
}

impl DeclarationSemanticTable {
    pub(crate) fn empty() -> Self {
        Self {
            by_path: FxHashMap::default(),
        }
    }

    pub(crate) fn from_environment(
        declaration_table: &TopLevelDeclarationTable,
        resolved_function_signatures_by_path: &FxHashMap<InternedPath, ResolvedFunctionSignature>,
        nominal_type_ids_by_path: &FxHashMap<InternedPath, TypeId>,
        type_environment: &TypeEnvironment,
    ) -> Self {
        let mut by_path = FxHashMap::default();

        for declaration in declaration_table.iter() {
            let kind = classify_declaration(
                declaration,
                resolved_function_signatures_by_path,
                nominal_type_ids_by_path,
                type_environment,
            );
            by_path.insert(declaration.id.clone(), kind);
        }

        Self { by_path }
    }

    pub(crate) fn kind_for_path(&self, path: &InternedPath) -> Option<DeclarationSemanticKind> {
        self.by_path.get(path).copied()
    }
}

fn classify_declaration(
    declaration: &Declaration,
    resolved_function_signatures_by_path: &FxHashMap<InternedPath, ResolvedFunctionSignature>,
    nominal_type_ids_by_path: &FxHashMap<InternedPath, TypeId>,
    type_environment: &TypeEnvironment,
) -> DeclarationSemanticKind {
    if resolved_function_signatures_by_path.contains_key(&declaration.id) {
        return DeclarationSemanticKind::Function;
    }

    if let Some(type_id) = nominal_type_ids_by_path.get(&declaration.id) {
        return match type_environment.get(*type_id) {
            Some(TypeDefinition::Struct(..)) => DeclarationSemanticKind::Struct,
            Some(TypeDefinition::Choice(..)) => DeclarationSemanticKind::Choice,
            _ => DeclarationSemanticKind::Value,
        };
    }

    if declaration.value.is_compile_time_constant() {
        DeclarationSemanticKind::Constant
    } else {
        DeclarationSemanticKind::Value
    }
}
