#![cfg(test)]

use crate::backends::function_registry::{
    HostAbiType, HostAccessKind, HostFunctionDef, HostParameter, HostRegistry, HostReturnAlias,
};
use crate::compiler_frontend::analysis::borrow_checker::{BorrowCheckReport, check_borrows};
use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, TextLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

pub(crate) fn location(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

pub(crate) fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

pub(crate) fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

pub(crate) fn var(id: InternedPath, value: Expression) -> Declaration {
    Declaration { id, value }
}

pub(crate) fn param(
    id: InternedPath,
    data_type: DataType,
    mutable: bool,
    location: TextLocation,
) -> Declaration {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Declaration {
        id,
        value: Expression::new(ExpressionKind::None, location, data_type, ownership),
    }
}

pub(crate) fn reference_expr(
    name: InternedPath,
    data_type: DataType,
    location: TextLocation,
) -> Expression {
    Expression::reference(name, data_type, location, Ownership::ImmutableReference)
}

pub(crate) fn assignment_target(
    name: InternedPath,
    data_type: DataType,
    location: TextLocation,
) -> AstNode {
    node(
        NodeKind::Rvalue(Expression::reference(
            name,
            data_type,
            location.clone(),
            Ownership::MutableReference,
        )),
        location,
    )
}

pub(crate) fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: TextLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        warnings: vec![],
    }
}

pub(crate) fn entry_and_start(string_table: &mut StringTable) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    (entry_path, start_name)
}

pub(crate) fn lower_hir(
    ast: Ast,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::hir::hir_nodes::HirModule {
    HirBuilder::new(string_table)
        .build_hir_module(ast)
        .expect("HIR lowering should succeed")
}

pub(crate) fn default_host_registry(string_table: &mut StringTable) -> HostRegistry {
    HostRegistry::new(string_table)
}

pub(crate) fn register_host_function(
    registry: &mut HostRegistry,
    name: &'static str,
    param_access: Vec<HostAccessKind>,
    return_alias: HostReturnAlias,
    return_type: HostAbiType,
) {
    let parameters = param_access
        .into_iter()
        .map(|access_kind| HostParameter {
            language_type: DataType::Int,
            abi_type: HostAbiType::I32,
            access_kind,
        })
        .collect::<Vec<_>>();

    registry
        .register_function(HostFunctionDef {
            name,
            parameters,
            return_type,
            return_alias,
            ownership: Ownership::ImmutableReference,
            error_handling: crate::backends::function_registry::ErrorHandling::None,
            description: format!("test host function {name}"),
        })
        .expect("host function registration should succeed");
}

pub(crate) fn run_borrow_checker(
    module: &crate::compiler_frontend::hir::hir_nodes::HirModule,
    host_registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    check_borrows(module, host_registry, string_table)
}
