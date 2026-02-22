#![cfg(test)]

use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    assignment_target, build_ast, default_host_registry, entry_and_start, function_node, location,
    lower_hir, node, reference_expr, run_borrow_checker, symbol, var,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpression, HirExpressionKind, HirNodeId, HirStatement, HirStatementKind, HirValueId,
    ValueKind,
};
use crate::compiler_frontend::string_interning::StringTable;

#[test]
fn if_branch_local_alias_does_not_escape_merge() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

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
                NodeKind::If(
                    Expression::bool(true, location(2), Ownership::ImmutableOwned),
                    vec![node(
                        NodeKind::VariableDeclaration(var(
                            y,
                            reference_expr(x.clone(), DataType::Int, location(3)),
                        )),
                        location(3),
                    )],
                    Some(vec![]),
                ),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(4))),
                    value: Expression::int(2, location(4), Ownership::ImmutableOwned),
                },
                location(4),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("branch-local alias should not be visible after merge");
}

#[test]
fn match_arm_local_alias_does_not_escape_merge() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let arm = MatchArm {
        condition: Expression::int(1, location(3), Ownership::ImmutableOwned),
        body: vec![node(
            NodeKind::VariableDeclaration(var(
                y,
                reference_expr(x.clone(), DataType::Int, location(4)),
            )),
            location(4),
        )],
    };

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
                NodeKind::Match(
                    Expression::int(1, location(2), Ownership::ImmutableOwned),
                    vec![arm],
                    None,
                ),
                location(2),
            ),
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

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("match-arm local alias should not be visible after merge");
}

#[test]
fn while_body_local_alias_does_not_escape_exit() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

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
                NodeKind::WhileLoop(
                    Expression::bool(false, location(2), Ownership::ImmutableOwned),
                    vec![node(
                        NodeKind::VariableDeclaration(var(
                            y,
                            reference_expr(x.clone(), DataType::Int, location(3)),
                        )),
                        location(3),
                    )],
                ),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(4))),
                    value: Expression::int(2, location(4), Ownership::ImmutableOwned),
                },
                location(4),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("while-body local alias should not be visible in exit block");
}

#[test]
fn dead_local_access_reports_borrow_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

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
                NodeKind::If(
                    Expression::bool(true, location(2), Ownership::ImmutableOwned),
                    vec![node(
                        NodeKind::VariableDeclaration(var(
                            y.clone(),
                            reference_expr(x.clone(), DataType::Int, location(3)),
                        )),
                        location(3),
                    )],
                    Some(vec![]),
                ),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(4))),
                    value: Expression::int(2, location(4), Ownership::ImmutableOwned),
                },
                location(4),
            ),
        ],
        location(1),
    );

    let mut hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);

    let start = &hir.functions[hir.start_function.0 as usize];
    let entry = &hir.blocks[start.entry.0 as usize];
    let (then_block, _) = match &entry.terminator {
        crate::compiler_frontend::hir::hir_nodes::HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (*then_block, *else_block),
        other => panic!("expected if terminator, found {:?}", other),
    };

    let merge_block = match &hir.blocks[then_block.0 as usize].terminator {
        crate::compiler_frontend::hir::hir_nodes::HirTerminator::Jump { target, .. } => *target,
        other => panic!("expected then jump, found {:?}", other),
    };

    let then_local = hir.blocks[then_block.0 as usize]
        .locals
        .iter()
        .find_map(|local| {
            hir.side_table
                .resolve_local_name(local.id, &string_table)
                .filter(|name| *name == y.name_str(&string_table).unwrap_or_default())
                .map(|_| local.clone())
        })
        .expect("then local should exist");

    let synthetic_value = HirExpression {
        id: HirValueId(77_001),
        kind: HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
            then_local.id,
        )),
        ty: then_local.ty,
        value_kind: ValueKind::Place,
        region: hir.blocks[merge_block.0 as usize].region,
    };
    let synthetic_statement = HirStatement {
        id: HirNodeId(77_000),
        kind: HirStatementKind::Expr(synthetic_value),
        location: location(100),
    };
    hir.blocks[merge_block.0 as usize]
        .statements
        .insert(0, synthetic_statement.clone());
    hir.side_table
        .map_statement(&synthetic_statement.location, &synthetic_statement);
    hir.side_table.map_value(
        &synthetic_statement.location,
        HirValueId(77_001),
        &synthetic_statement.location,
    );

    let error = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect_err("dead local access should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(
        error
            .msg
            .contains("before initialization or after scope end")
    );
}
