//! HIR statement lowering regression tests.
//!
//! WHAT: checks how statement-level AST nodes become HIR blocks, statements, and terminators.
//! WHY: statement lowering owns most CFG construction and benefits from targeted regression coverage.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_nodes::{HirPattern, HirTerminator};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    fresh_returns, function_node, make_test_variable, node, param, test_location,
};

use crate::compiler_frontend::hir::hir_builder::{
    assert_no_placeholder_terminators, build_ast, lower_ast,
};

#[test]
fn non_unit_function_with_terminal_match_default_does_not_report_fallthrough() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let chooser = super::symbol("choose_match", &mut string_table);
    let x = super::symbol("x", &mut string_table);

    let chooser_fn = function_node(
        chooser,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(10))],
            returns: fresh_returns(vec![DataType::Int]),
        },
        vec![node(
            NodeKind::Match(
                Expression::reference(
                    x,
                    DataType::Int,
                    test_location(11),
                    Ownership::ImmutableReference,
                ),
                vec![MatchArm {
                    condition: Expression::int(1, test_location(11), Ownership::ImmutableOwned),
                    guard: None,
                    body: vec![node(
                        NodeKind::Return(vec![Expression::int(
                            1,
                            test_location(11),
                            Ownership::ImmutableOwned,
                        )]),
                        test_location(11),
                    )],
                }],
                Some(vec![node(
                    NodeKind::Return(vec![Expression::int(
                        2,
                        test_location(12),
                        Ownership::ImmutableOwned,
                    )]),
                    test_location(12),
                )]),
            ),
            test_location(11),
        )],
        test_location(10),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn, chooser_fn], entry_path);
    let module = lower_ast(ast, &mut string_table)
        .expect("all-terminal match arms should not trigger fallthrough");

    let chooser_block = &module.blocks[module.functions[1].entry.0 as usize];
    assert!(matches!(
        chooser_block.terminator,
        HirTerminator::Match { .. }
    ));
    assert_no_placeholder_terminators(&module);
}

#[test]
fn lowers_match_with_literal_arms_and_synthesized_wildcard_default() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let match_node = node(
        NodeKind::Match(
            Expression::reference(
                x.clone(),
                DataType::Int,
                test_location(3),
                Ownership::ImmutableReference,
            ),
            vec![
                MatchArm {
                    condition: Expression::int(1, test_location(3), Ownership::ImmutableOwned),
                    guard: None,
                    body: vec![node(
                        NodeKind::Rvalue(Expression::int(
                            9,
                            test_location(3),
                            Ownership::ImmutableOwned,
                        )),
                        test_location(3),
                    )],
                },
                MatchArm {
                    condition: Expression::int(2, test_location(3), Ownership::ImmutableOwned),
                    guard: None,
                    body: vec![node(
                        NodeKind::Rvalue(Expression::int(
                            8,
                            test_location(3),
                            Ownership::ImmutableOwned,
                        )),
                        test_location(3),
                    )],
                },
            ],
            None,
        ),
        test_location(3),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![match_node],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let arms = match &entry_block.terminator {
        HirTerminator::Match { arms, .. } => arms,
        _ => panic!("expected match terminator"),
    };

    assert_eq!(arms.len(), 3);
    assert!(matches!(arms[0].pattern, HirPattern::Literal(_)));
    assert!(matches!(arms[1].pattern, HirPattern::Literal(_)));
    assert!(matches!(arms[2].pattern, HirPattern::Wildcard));
}

#[test]
fn lowers_match_with_guarded_arm_into_hir_guard_expression() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let match_node = node(
        NodeKind::Match(
            Expression::reference(
                x.clone(),
                DataType::Int,
                test_location(3),
                Ownership::ImmutableReference,
            ),
            vec![MatchArm {
                condition: Expression::int(1, test_location(3), Ownership::ImmutableOwned),
                guard: Some(Expression::bool(
                    true,
                    test_location(3),
                    Ownership::ImmutableOwned,
                )),
                body: vec![node(
                    NodeKind::Rvalue(Expression::int(
                        9,
                        test_location(3),
                        Ownership::ImmutableOwned,
                    )),
                    test_location(3),
                )],
            }],
            Some(vec![node(
                NodeKind::Rvalue(Expression::int(
                    8,
                    test_location(4),
                    Ownership::ImmutableOwned,
                )),
                test_location(4),
            )]),
        ),
        test_location(3),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![match_node],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let arms = match &entry_block.terminator {
        HirTerminator::Match { arms, .. } => arms,
        _ => panic!("expected match terminator"),
    };

    assert!(
        arms[0].guard.is_some(),
        "first arm should preserve the lowered guard expression"
    );
    assert!(
        arms[1].guard.is_none(),
        "synthesized wildcard arm should not carry a guard"
    );
}

#[test]
fn match_guard_rejects_lowering_when_guard_emits_prelude_statements() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);
    let io_path = super::symbol("io", &mut string_table);

    let guarded_arm = MatchArm {
        condition: Expression::int(1, test_location(3), Ownership::ImmutableOwned),
        guard: Some(Expression::host_function_call(
            io_path,
            vec![Expression::bool(
                true,
                test_location(3),
                Ownership::ImmutableOwned,
            )],
            vec![DataType::None],
            test_location(3),
        )),
        body: vec![],
    };

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![node(
            NodeKind::Match(
                Expression::reference(
                    x,
                    DataType::Int,
                    test_location(3),
                    Ownership::ImmutableReference,
                ),
                vec![guarded_arm],
                Some(vec![]),
            ),
            test_location(3),
        )],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table)
        .expect_err("guard expressions with preludes should fail HIR lowering");

    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(
        err.errors[0]
            .msg
            .contains("Match arm guard lowering produced side-effect statements"),
        "unexpected error message: {}",
        err.errors[0].msg
    );
}

#[test]
fn match_rejects_non_literal_pattern_expressions() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![node(
            NodeKind::Match(
                Expression::reference(
                    x.clone(),
                    DataType::Int,
                    test_location(3),
                    Ownership::ImmutableReference,
                ),
                vec![MatchArm {
                    condition: Expression::reference(
                        x,
                        DataType::Int,
                        test_location(3),
                        Ownership::ImmutableReference,
                    ),
                    guard: None,
                    body: vec![],
                }],
                None,
            ),
            test_location(3),
        )],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table)
        .expect_err("non-literal match pattern should fail HIR lowering");

    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(
        err.errors[0]
            .msg
            .contains("Match arm patterns must be compile-time literals"),
        "unexpected error message: {}",
        err.errors[0].msg
    );
}

#[test]
fn break_outside_loop_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Break, test_location(2))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("break outside loop should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("active loop context"));
}

#[test]
fn continue_outside_loop_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Continue, test_location(2))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("continue outside loop should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("active loop context"));
}

#[test]
fn top_level_return_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let top_level_return = node(NodeKind::Return(vec![]), test_location(2));

    let ast = build_ast(vec![start_fn, top_level_return], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("top-level return should fail");

    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("Top-level return"));
}

#[test]
fn enforces_non_unit_fallthrough_and_unit_implicit_return() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let non_unit_name = super::symbol("non_unit", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let non_unit_fn = function_node(
        non_unit_name,
        FunctionSignature {
            parameters: vec![],
            returns: fresh_returns(vec![DataType::Int]),
        },
        vec![],
        test_location(2),
    );

    let ast_err = build_ast(vec![start_fn.clone(), non_unit_fn], entry_path.clone());
    let err = lower_ast(ast_err, &mut string_table).expect_err("non-unit fallthrough should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("fall through"));

    let ast_ok = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast_ok, &mut string_table).expect("unit fallthrough should succeed");
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    assert!(matches!(entry_block.terminator, HirTerminator::Return(_)));
}

#[test]
fn side_table_maps_statement_and_terminator_locations() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let decl_loc = test_location(4);
    let ret_loc = test_location(5);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x,
                    Expression::int(1, decl_loc.clone(), Ownership::ImmutableOwned),
                )),
                decl_loc.clone(),
            ),
            node(NodeKind::Return(vec![]), ret_loc.clone()),
        ],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let decl_mappings = module.side_table.hir_locations_for_ast(&decl_loc);
    assert!(!decl_mappings.is_empty());

    let ret_mappings = module.side_table.hir_locations_for_ast(&ret_loc);
    assert!(!ret_mappings.is_empty());
}
