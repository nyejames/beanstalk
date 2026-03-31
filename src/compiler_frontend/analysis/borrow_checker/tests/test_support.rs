//! Borrow-checker test fixtures and lowering helpers.
//!
//! WHAT: builds small AST/HIR programs and host registries used across borrow-checker tests.
//! WHY: centralizing fixture construction keeps each test focused on the aliasing rule it is
//! exercising instead of repeating setup noise.

use crate::compiler_frontend::analysis::borrow_checker::{BorrowCheckReport, check_borrows};
use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::host_functions::{
    HostRegistry,
    test_support::{
        TestHostAbiType as HostAbiType, TestHostAccessKind as HostAccessKind,
        TestHostReturnAlias as HostReturnAlias, register_test_host_function,
    },
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;
pub(crate) use crate::compiler_frontend::test_support::test_project_path_resolver;
use crate::compiler_frontend::tokenizer::tokens::CharPosition;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

pub(crate) fn location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120, // Arbitrary number
        },
    }
}

pub(crate) fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

pub(crate) fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

pub(crate) fn make_test_variable(id: InternedPath, value: Expression) -> Declaration {
    Declaration { id, value }
}

pub(crate) fn param(
    id: InternedPath,
    data_type: DataType,
    mutable: bool,
    location: SourceLocation,
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
    location: SourceLocation,
) -> Expression {
    Expression::reference(name, data_type, location, Ownership::ImmutableReference)
}

pub(crate) fn assignment_target(
    name: InternedPath,
    data_type: DataType,
    location: SourceLocation,
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
    location: SourceLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        start_template_items: vec![],
        rendered_path_usages: vec![],
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
    HirBuilder::new(
        string_table,
        PathStringFormatConfig::default(),
        test_project_path_resolver(),
    )
    .build_hir_module(ast)
    .expect("HIR lowering should succeed")
}

pub(crate) fn default_host_registry(_string_table: &mut StringTable) -> HostRegistry {
    HostRegistry::new()
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
        .map(|access_kind| (DataType::Int, access_kind))
        .collect::<Vec<_>>();
    register_test_host_function(registry, name, parameters, return_alias, return_type)
        .expect("host function registration should succeed");
}

pub(crate) fn run_borrow_checker(
    module: &crate::compiler_frontend::hir::hir_nodes::HirModule,
    host_registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    check_borrows(module, host_registry, string_table)
}
