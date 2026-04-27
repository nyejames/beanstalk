//! Borrow-checker fact-generation regression tests.
//!
//! WHAT: checks the low-level facts emitted for borrows, moves, assignments, and returns.
//! WHY: these facts are the borrow checker's source of truth, so targeted tests catch drift
//! before it reaches higher-level diagnostics.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::ids::{BlockId, HirNodeId, HirValueId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    build_ast, default_external_package_registry, entry_and_start, function_node, lower_hir,
    make_test_variable, node, reference_expr, run_borrow_checker, symbol, test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

#[test]
fn statement_terminator_and_value_facts_are_populated() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

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
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(1), ValueMode::MutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y.clone(),
                    Expression::int(0, test_location(2), ValueMode::ImmutableOwned),
                )),
                test_location(2),
            ),
            node(
                NodeKind::If(
                    Expression::bool(true, test_location(3), ValueMode::ImmutableOwned),
                    vec![node(
                        NodeKind::Assignment {
                            target: Box::new(node(
                                NodeKind::Rvalue(reference_expr(
                                    x.clone(),
                                    DataType::Int,
                                    test_location(4),
                                )),
                                test_location(4),
                            )),
                            value: Expression::int(2, test_location(4), ValueMode::ImmutableOwned),
                        },
                        test_location(4),
                    )],
                    Some(vec![node(
                        NodeKind::Assignment {
                            target: Box::new(node(
                                NodeKind::Rvalue(reference_expr(
                                    x.clone(),
                                    DataType::Int,
                                    test_location(5),
                                )),
                                test_location(5),
                            )),
                            value: Expression::int(3, test_location(5), ValueMode::ImmutableOwned),
                        },
                        test_location(5),
                    )]),
                ),
                test_location(3),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("borrow checking should succeed");

    let start = &hir.functions[hir.start_function.0 as usize];
    let reachable = collect_reachable_blocks(&hir, start.entry);

    for block_id in &reachable {
        let block = &hir.blocks[block_id.0 as usize];
        assert!(
            report.analysis.terminator_fact(*block_id).is_some(),
            "missing terminator fact for block {block_id:?}"
        );

        for statement in &block.statements {
            assert!(
                report.analysis.statement_fact(statement.id).is_some(),
                "missing statement fact for statement {:?}",
                statement.id
            );
            assert!(
                hir.side_table
                    .hir_source_location_for_hir(HirLocation::Statement(statement.id))
                    .is_some(),
                "statement {:?} should have source mapping",
                statement.id
            );
        }
    }

    let mut value_ids = FxHashSet::default();
    for block_id in &reachable {
        let block = &hir.blocks[block_id.0 as usize];
        for statement in &block.statements {
            collect_statement_values(statement.kind.clone(), &mut value_ids);
        }
        collect_terminator_values(&block.terminator, &mut value_ids);
    }

    for value_id in value_ids {
        assert!(
            report.analysis.value_fact(value_id).is_some(),
            "missing value fact for value {value_id:?}"
        );
        assert!(
            hir.side_table.value_source_location(value_id).is_some(),
            "value {value_id:?} should have side-table source mapping"
        );
    }
}

#[test]
fn drop_statement_produces_statement_fact() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let value = symbol("value", &mut string_table);
    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::VariableDeclaration(make_test_variable(
                value,
                Expression::int(1, test_location(1), ValueMode::MutableOwned),
            )),
            test_location(1),
        )],
        test_location(1),
    );

    let mut hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let start = &hir.functions[hir.start_function.0 as usize];
    let entry_block = &mut hir.blocks[start.entry.0 as usize];
    let drop_local = entry_block
        .locals
        .first()
        .expect("entry block should contain at least one local")
        .id;

    let next_statement_id = entry_block
        .statements
        .iter()
        .map(|statement| statement.id.0)
        .max()
        .unwrap_or(0)
        + 1;

    entry_block.statements.push(HirStatement {
        id: HirNodeId(next_statement_id),
        kind: HirStatementKind::Drop(drop_local),
        location: test_location(2),
    });

    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("borrow checking should succeed");

    let fact = report
        .analysis
        .statement_fact(HirNodeId(next_statement_id))
        .expect("drop statement should have a statement fact");
    assert!(fact.shared_roots.is_empty());
    assert!(fact.mutable_roots.is_empty());
}

#[test]
fn statement_entry_state_reflects_last_use_reborrow_window() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let data = symbol("data", &mut string_table);
    let first_ref = symbol("first_ref", &mut string_table);
    let sink = symbol("sink", &mut string_table);
    let second_ref = symbol("second_ref", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    data.clone(),
                    Expression::int(7, test_location(1), ValueMode::MutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    first_ref.clone(),
                    Expression::reference(
                        data.clone(),
                        DataType::Int,
                        test_location(2),
                        ValueMode::MutableReference,
                    ),
                )),
                test_location(2),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    sink,
                    reference_expr(first_ref, DataType::Int, test_location(3)),
                )),
                test_location(3),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    second_ref,
                    Expression::reference(
                        data,
                        DataType::Int,
                        test_location(4),
                        ValueMode::MutableReference,
                    ),
                )),
                test_location(4),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("reborrow after last-use should pass");

    let second_statement_id = find_statement_id_for_line(&hir, 4)
        .expect("should locate the reborrow statement by source line");
    let data_local = find_assigned_local_for_line(&hir, 1)
        .expect("should locate the source local by declaration line");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&second_statement_id)
        .expect("reborrow statement should have an entry snapshot");
    let data_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == data_local)
        .expect("entry snapshot should include the data local");

    assert!(
        data_snapshot.alias_roots.is_empty(),
        "data local should not retain live alias roots at the reborrow point"
    );
}

fn find_statement_id_for_line(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    line: i32,
) -> Option<HirNodeId> {
    for block in &hir.blocks {
        for statement in &block.statements {
            let Some(source) = hir
                .side_table
                .hir_source_location_for_hir(HirLocation::Statement(statement.id))
            else {
                continue;
            };
            if source.start_pos.line_number == line {
                return Some(statement.id);
            }
        }
    }
    None
}

fn find_assigned_local_for_line(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    line: i32,
) -> Option<crate::compiler_frontend::hir::ids::LocalId> {
    for block in &hir.blocks {
        for statement in &block.statements {
            let Some(source) = hir
                .side_table
                .hir_source_location_for_hir(HirLocation::Statement(statement.id))
            else {
                continue;
            };
            if source.start_pos.line_number != line {
                continue;
            }
            if let HirStatementKind::Assign {
                target: HirPlace::Local(local),
                ..
            } = &statement.kind
            {
                return Some(*local);
            }
        }
    }
    None
}

fn collect_reachable_blocks(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    entry: BlockId,
) -> Vec<BlockId> {
    let mut visited = FxHashSet::default();
    let mut queue = VecDeque::new();
    let mut blocks = Vec::new();
    queue.push_back(entry);

    while let Some(block_id) = queue.pop_front() {
        if !visited.insert(block_id) {
            continue;
        }

        blocks.push(block_id);
        match &hir.blocks[block_id.0 as usize].terminator {
            HirTerminator::Jump { target, .. } => queue.push_back(*target),
            HirTerminator::If {
                then_block,
                else_block,
                ..
            } => {
                queue.push_back(*then_block);
                queue.push_back(*else_block);
            }
            HirTerminator::Match { arms, .. } => {
                for arm in arms {
                    queue.push_back(arm.body);
                }
            }
            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                queue.push_back(*target);
            }
            HirTerminator::Return(_) | HirTerminator::Panic { .. } => {}
        }
    }

    blocks
}

fn collect_statement_values(kind: HirStatementKind, out: &mut FxHashSet<HirValueId>) {
    match kind {
        HirStatementKind::Assign { value, .. } => collect_expression_values(&value, out),
        HirStatementKind::Call { args, .. } => {
            for arg in args {
                collect_expression_values(&arg, out);
            }
        }
        HirStatementKind::Expr(expr) => collect_expression_values(&expr, out),
        HirStatementKind::Drop(_) => {}
        HirStatementKind::PushRuntimeFragment { value, .. } => {
            collect_expression_values(&value, out)
        }
    }
}

fn collect_terminator_values(terminator: &HirTerminator, out: &mut FxHashSet<HirValueId>) {
    match terminator {
        HirTerminator::If { condition, .. } => collect_expression_values(condition, out),
        HirTerminator::Match { scrutinee, arms } => {
            collect_expression_values(scrutinee, out);
            for arm in arms {
                if let crate::compiler_frontend::hir::patterns::HirPattern::Literal(value) =
                    &arm.pattern
                {
                    collect_expression_values(value, out);
                }
                if let Some(guard) = &arm.guard {
                    collect_expression_values(guard, out);
                }
            }
        }
        HirTerminator::Return(value) => collect_expression_values(value, out),
        HirTerminator::Panic { message } => {
            if let Some(value) = message {
                collect_expression_values(value, out);
            }
        }
        HirTerminator::Jump { .. }
        | HirTerminator::Break { .. }
        | HirTerminator::Continue { .. } => {}
    }
}

fn collect_expression_values(expression: &HirExpression, out: &mut FxHashSet<HirValueId>) {
    out.insert(expression.id);

    match &expression.kind {
        HirExpressionKind::BinOp { left, right, .. } => {
            collect_expression_values(left, out);
            collect_expression_values(right, out);
        }
        HirExpressionKind::UnaryOp { operand, .. } => collect_expression_values(operand, out),
        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                collect_expression_values(value, out);
            }
        }
        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                collect_expression_values(element, out);
            }
        }
        HirExpressionKind::TupleGet { tuple, .. } => {
            collect_expression_values(tuple, out);
        }
        HirExpressionKind::Range { start, end } => {
            collect_expression_values(start, out);
            collect_expression_values(end, out);
        }
        HirExpressionKind::VariantConstruct { fields, .. } => {
            for field in fields {
                collect_expression_values(&field.value, out);
            }
        }
        HirExpressionKind::ResultPropagate { result } => collect_expression_values(result, out),
        HirExpressionKind::ResultIsOk { result }
        | HirExpressionKind::ResultUnwrapOk { result }
        | HirExpressionKind::ResultUnwrapErr { result }
        | HirExpressionKind::BuiltinCast { value: result, .. } => {
            collect_expression_values(result, out);
        }
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_)
        | HirExpressionKind::Copy(_)
        | HirExpressionKind::Load(_) => {}

        HirExpressionKind::VariantPayloadGet { source, .. } => {
            collect_expression_values(source, out);
        }
    }
}
