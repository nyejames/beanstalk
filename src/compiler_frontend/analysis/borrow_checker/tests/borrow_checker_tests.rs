#![cfg(test)]

use crate::backends::function_registry::CallTarget;
use crate::build_system::build::Module;
use crate::build_system::create_project_modules::ExternalImport;
use crate::compiler_frontend::CompilerFrontend;
use crate::compiler_frontend::analysis::borrow_checker::{BorrowCheckReport, check_borrows};
use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::HirStatementKind;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::projects::settings::{Config, IMPLICIT_START_FUNC_NAME};

fn location(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn symbol(
    name: &str,
    string_table: &mut crate::compiler_frontend::string_interning::StringTable,
) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

fn var(id: InternedPath, value: Expression) -> Var {
    Var { id, value }
}

fn param(id: InternedPath, data_type: DataType, mutable: bool, location: TextLocation) -> Var {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Var {
        id,
        value: Expression::new(ExpressionKind::None, location, data_type, ownership),
    }
}

fn reference_expr(name: InternedPath, data_type: DataType, location: TextLocation) -> Expression {
    Expression::reference(name, data_type, location, Ownership::ImmutableReference)
}

fn assignment_target(name: InternedPath, data_type: DataType, location: TextLocation) -> AstNode {
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

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: TextLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        warnings: vec![],
    }
}

fn entry_and_start(
    string_table: &mut crate::compiler_frontend::string_interning::StringTable,
) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    (entry_path, start_name)
}

fn lower_hir(
    ast: Ast,
    string_table: &mut crate::compiler_frontend::string_interning::StringTable,
) -> crate::compiler_frontend::hir::hir_nodes::HirModule {
    HirBuilder::new(string_table)
        .build_hir_module(ast)
        .expect("HIR lowering should succeed")
}

fn run_borrow_checker(
    module: &crate::compiler_frontend::hir::hir_nodes::HirModule,
    string_table: &crate::compiler_frontend::string_interning::StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    check_borrows(module, string_table)
}

#[test]
fn shared_reads_across_aliases_are_accepted() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y.clone(),
                    reference_expr(x.clone(), DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::Rvalue(reference_expr(x, DataType::Int, location(3))),
                location(3),
            ),
            node(
                NodeKind::Rvalue(reference_expr(y, DataType::Int, location(4))),
                location(4),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let report = run_borrow_checker(&hir, &string_table).expect("borrow checking should pass");
    assert!(report.stats.functions_analyzed >= 1);
    assert!(report.analysis.total_state_snapshots() >= 1);
}

#[test]
fn mutable_access_is_rejected_when_alias_is_live() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    reference_expr(x.clone(), DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(3))),
                    value: Expression::int(2, location(3), Ownership::ImmutableOwned),
                },
                location(3),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("alias conflict should fail borrow checking");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn two_mutable_args_to_same_root_are_rejected() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let mutate2 = symbol("mutate2", &mut string_table);
    let a = symbol("a", &mut string_table);
    let b = symbol("b", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        mutate2.clone(),
        FunctionSignature {
            parameters: vec![
                param(a, DataType::Int, true, location(1)),
                param(b, DataType::Int, true, location(1)),
            ],
            returns: vec![],
        },
        vec![],
        location(1),
    );

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(2), Ownership::MutableOwned),
                )),
                location(2),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mutate2,
                    args: vec![
                        reference_expr(x.clone(), DataType::Int, location(3)),
                        reference_expr(x, DataType::Int, location(3)),
                    ],
                    returns: vec![],
                    location: location(3),
                },
                location(3),
            ),
        ],
        location(2),
    );

    let ast = build_ast(vec![callee, start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("overlapping mutable call args should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("evaluation sequence"));
}

#[test]
fn shared_then_mutable_overlap_in_call_args_is_rejected() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let borrow_then_mut = symbol("borrow_then_mut", &mut string_table);
    let a = symbol("a", &mut string_table);
    let b = symbol("b", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        borrow_then_mut.clone(),
        FunctionSignature {
            parameters: vec![
                param(a, DataType::Int, false, location(1)),
                param(b, DataType::Int, true, location(1)),
            ],
            returns: vec![],
        },
        vec![],
        location(1),
    );

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(2), Ownership::MutableOwned),
                )),
                location(2),
            ),
            node(
                NodeKind::FunctionCall {
                    name: borrow_then_mut,
                    args: vec![
                        reference_expr(x.clone(), DataType::Int, location(3)),
                        reference_expr(x, DataType::Int, location(3)),
                    ],
                    returns: vec![],
                    location: location(3),
                },
                location(3),
            ),
        ],
        location(2),
    );

    let ast = build_ast(vec![callee, start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("shared/mutable overlap in call args should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("overlapping"));
}

#[test]
fn immutable_local_initialization_is_allowed_but_reassignment_is_rejected() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::ImmutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(2))),
                    value: Expression::int(2, location(2), Ownership::ImmutableOwned),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("reassigning immutable local should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("immutable local"));
}

#[test]
fn alias_view_assignment_is_write_through_and_conflicts() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);
    let z = symbol("z", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y.clone(),
                    reference_expr(x.clone(), DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    z,
                    reference_expr(x.clone(), DataType::Int, location(3)),
                )),
                location(3),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(y, DataType::Int, location(4))),
                    value: Expression::int(7, location(4), Ownership::ImmutableOwned),
                },
                location(4),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("alias-view assignment should be treated as write-through conflict");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn user_call_param_mutability_is_derived_from_signature() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let mut_only = symbol("mut_only", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        mut_only.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, true, location(1))],
            returns: vec![],
        },
        vec![],
        location(1),
    );

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(2), Ownership::ImmutableOwned),
                )),
                location(2),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut_only,
                    args: vec![reference_expr(x, DataType::Int, location(3))],
                    returns: vec![],
                    location: location(3),
                },
                location(3),
            ),
        ],
        location(2),
    );

    let ast = build_ast(vec![callee, start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("mutable parameter call should require mutable argument root");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("immutable local"));
}

#[test]
fn host_call_arguments_are_shared_only() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let alloc = symbol("alloc", &mut string_table);
    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::ImmutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: alloc,
                    args: vec![reference_expr(x, DataType::Int, location(2))],
                    returns: vec![],
                    location: location(2),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    run_borrow_checker(&hir, &string_table)
        .expect("host call args should be treated as shared-only and pass");
}

#[test]
fn unresolved_user_call_target_returns_borrow_error() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let helper = symbol("helper", &mut string_table);

    let helper_fn = function_node(
        helper.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        location(1),
    );

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::FunctionCall {
                name: helper,
                args: vec![],
                returns: vec![],
                location: location(2),
            },
            location(2),
        )],
        location(2),
    );

    let ast = build_ast(vec![helper_fn, start], entry_path);
    let mut hir = lower_hir(ast, &mut string_table);

    let start_fn = &hir.functions[hir.start_function.0 as usize];
    let entry_block = &mut hir.blocks[start_fn.entry.0 as usize];
    let missing = symbol("missing_target", &mut string_table);

    for statement in &mut entry_block.statements {
        if let HirStatementKind::Call { target, .. } = &mut statement.kind {
            *target = CallTarget::UserFunction(missing.clone());
        }
    }

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("unresolved user call should fail borrow checking");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("could not resolve user call target"));
}

#[test]
fn cfg_merge_preserves_aliases_conservatively() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let if_node = node(
        NodeKind::If(
            Expression::bool(true, location(3), Ownership::ImmutableOwned),
            vec![node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(y.clone(), DataType::Int, location(4))),
                    value: reference_expr(x.clone(), DataType::Int, location(4)),
                },
                location(4),
            )],
            None,
        ),
        location(3),
    );

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y.clone(),
                    Expression::int(0, location(2), Ownership::MutableOwned),
                )),
                location(2),
            ),
            if_node,
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(5))),
                    value: Expression::int(2, location(5), Ownership::ImmutableOwned),
                },
                location(5),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let error = run_borrow_checker(&hir, &string_table)
        .expect_err("merge should conservatively keep alias possibility");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn compiler_frontend_check_borrows_reports_failure() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    reference_expr(x.clone(), DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(3))),
                    value: Expression::int(2, location(3), Ownership::ImmutableOwned),
                },
                location(3),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let hir = lower_hir(ast, &mut string_table);

    let config = Config::default();
    let frontend = CompilerFrontend::new(&config, string_table);
    let messages = frontend
        .check_borrows(&hir)
        .expect_err("borrow checking should fail");

    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.error_type == ErrorType::BorrowChecker)
    );
}

#[test]
fn module_can_store_successful_borrow_analysis_report() {
    let mut string_table = crate::compiler_frontend::string_interning::StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let counter = symbol("counter", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    counter.clone(),
                    Expression::int(0, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(
                        counter.clone(),
                        DataType::Int,
                        location(2),
                    )),
                    value: Expression::int(1, location(2), Ownership::ImmutableOwned),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let hir = lower_hir(ast, &mut string_table);
    let borrow_analysis =
        run_borrow_checker(&hir, &string_table).expect("borrow checking should pass");

    let module = Module {
        folder_name: "test".to_string(),
        entry_point: std::path::PathBuf::from("main.bst"),
        hir,
        borrow_analysis,
        required_module_imports: Vec::<ExternalImport>::new(),
        exported_functions: Vec::new(),
        warnings: Vec::new(),
        string_table,
    };

    assert!(module.borrow_analysis.stats.functions_analyzed >= 1);
    assert!(module.borrow_analysis.analysis.total_state_snapshots() >= 1);
}
