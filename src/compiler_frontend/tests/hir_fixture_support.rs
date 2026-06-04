//! HIR fixture support for frontend unit tests.
//!
//! WHAT: wraps synthetic AST-to-HIR lowering used by HIR and borrow-checker tests.
//! WHY: these helpers sit at the HIR boundary and must not depend on borrow validation.

use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::hir::hir_builder::lower_module;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    crate::compiler_frontend::hir::hir_builder::build_ast(nodes, entry_path)
}

pub(crate) fn entry_and_start(string_table: &mut StringTable) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    (entry_path, start_name)
}

pub(crate) fn lower_hir(ast: Ast, string_table: &mut StringTable) -> HirModule {
    let (module, _) = lower_module(ast, string_table, PathStringFormatConfig::default())
        .expect("HIR lowering should succeed");
    module
}
