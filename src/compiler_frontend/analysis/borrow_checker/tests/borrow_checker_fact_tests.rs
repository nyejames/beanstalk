#![cfg(test)]

use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    build_ast, default_host_registry, entry_and_start, function_node, location, lower_hir, node,
    reference_expr, run_borrow_checker, symbol, var,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_display::HirLocation;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirStatementKind, HirTerminator, HirValueId,
};
use crate::compiler_frontend::string_interning::StringTable;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

#[test]
fn statement_terminator_and_value_facts_are_populated() {
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
                NodeKind::VariableDeclaration(var(
                    y.clone(),
                    Expression::int(0, location(2), Ownership::ImmutableOwned),
                )),
                location(2),
            ),
            node(
                NodeKind::If(
                    Expression::bool(true, location(3), Ownership::ImmutableOwned),
                    vec![node(
                        NodeKind::Assignment {
                            target: Box::new(node(
                                NodeKind::Rvalue(reference_expr(
                                    x.clone(),
                                    DataType::Int,
                                    location(4),
                                )),
                                location(4),
                            )),
                            value: Expression::int(2, location(4), Ownership::ImmutableOwned),
                        },
                        location(4),
                    )],
                    Some(vec![node(
                        NodeKind::Assignment {
                            target: Box::new(node(
                                NodeKind::Rvalue(reference_expr(
                                    x.clone(),
                                    DataType::Int,
                                    location(5),
                                )),
                                location(5),
                            )),
                            value: Expression::int(3, location(5), Ownership::ImmutableOwned),
                        },
                        location(5),
                    )]),
                ),
                location(3),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &host_registry, &string_table)
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

fn collect_reachable_blocks(
    hir: &crate::compiler_frontend::hir::hir_nodes::HirModule,
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
            HirTerminator::Loop { body, break_target } => {
                queue.push_back(*body);
                queue.push_back(*break_target);
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
    }
}

fn collect_terminator_values(terminator: &HirTerminator, out: &mut FxHashSet<HirValueId>) {
    match terminator {
        HirTerminator::If { condition, .. } => collect_expression_values(condition, out),
        HirTerminator::Match { scrutinee, arms } => {
            collect_expression_values(scrutinee, out);
            for arm in arms {
                if let crate::compiler_frontend::hir::hir_nodes::HirPattern::Literal(value) =
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
        | HirTerminator::Loop { .. }
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
        HirExpressionKind::Range { start, end } => {
            collect_expression_values(start, out);
            collect_expression_values(end, out);
        }
        HirExpressionKind::OptionConstruct { value, .. } => {
            if let Some(value) = value {
                collect_expression_values(value, out);
            }
        }
        HirExpressionKind::ResultConstruct { value, .. } => collect_expression_values(value, out),
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_)
        | HirExpressionKind::Load(_) => {}
    }
}
