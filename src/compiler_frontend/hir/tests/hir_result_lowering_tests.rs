//! HIR result-handling lowering regression tests.
//!
//! WHAT: checks how result propagation, multi-bind fallbacks, and error handlers lower into HIR
//!       control-flow and local bindings.
//! WHY: result handling spans expression and statement boundaries; targeted tests prevent
//!      regressions in error-path routing and fallback binding.

use crate::compiler_frontend::ast::ast_nodes::{MultiBindTarget, MultiBindTargetKind, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ResultCallHandling};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::expressions::HirExpressionKind;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    fresh_returns, function_node, node, test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;

use crate::compiler_frontend::hir::hir_builder::{build_ast, lower_ast};

#[test]
fn statement_result_propagation_with_unit_success_emits_runtime_propagate_expression() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let can_fail_name = super::symbol("can_fail", &mut string_table);
    let location = test_location(1);

    let can_fail_function = function_node(
        can_fail_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: vec![ReturnSlot::error(FunctionReturn::Value(
                DataType::StringSlice,
            ))],
        },
        vec![node(
            NodeKind::ReturnError(Expression::string_slice(
                string_table.intern("boom"),
                location.clone(),
                ValueMode::ImmutableOwned,
            )),
            location.clone(),
        )],
        location.clone(),
    );

    let start_function = function_node(
        start_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: vec![ReturnSlot::error(FunctionReturn::Value(
                DataType::StringSlice,
            ))],
        },
        vec![node(
            NodeKind::ResultHandledFunctionCall {
                name: can_fail_name,
                args: vec![],
                result_types: vec![],
                handling: ResultCallHandling::Propagate,
                location: location.clone(),
            },
            location.clone(),
        )],
        location.clone(),
    );

    let module = lower_ast(
        build_ast(vec![can_fail_function, start_function], entry_path),
        &mut string_table,
    )
    .expect("statement propagation lowering should succeed");

    let start_function = module
        .functions
        .iter()
        .find(|function| function.id == module.start_function)
        .expect("start function should exist");
    let start_entry = module
        .blocks
        .iter()
        .find(|block| block.id == start_function.entry)
        .expect("start entry block should exist");

    assert!(
        start_entry.statements.iter().any(|statement| matches!(
            statement.kind,
            HirStatementKind::Expr(crate::compiler_frontend::hir::expressions::HirExpression {
                kind: HirExpressionKind::ResultPropagate { .. },
                ..
            })
        )),
        "unit-success statement propagation should still emit a ResultPropagate expression statement"
    );
}

#[test]
fn statement_named_handler_lowering_builds_explicit_result_branching() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let can_fail_name = super::symbol("can_fail", &mut string_table);
    let location = test_location(10);
    let error_name = string_table.intern("err");
    let error_binding = start_name.join_str("err", &mut string_table);

    let can_fail_function = function_node(
        can_fail_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: vec![
                ReturnSlot::success(FunctionReturn::Value(DataType::StringSlice)),
                ReturnSlot::error(FunctionReturn::Value(DataType::StringSlice)),
            ],
        },
        vec![node(
            NodeKind::ReturnError(Expression::string_slice(
                string_table.intern("boom"),
                location.clone(),
                ValueMode::ImmutableOwned,
            )),
            location.clone(),
        )],
        location.clone(),
    );

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::ResultHandledFunctionCall {
                name: can_fail_name,
                args: vec![],
                result_types: vec![DataType::StringSlice],
                handling: ResultCallHandling::Handler {
                    error_name,
                    error_binding,
                    fallback: Some(vec![Expression::string_slice(
                        string_table.intern("fallback"),
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )]),
                    body: vec![],
                },
                location: location.clone(),
            },
            location.clone(),
        )],
        location.clone(),
    );

    let module = lower_ast(
        build_ast(vec![can_fail_function, start_function], entry_path),
        &mut string_table,
    )
    .expect("named-handler statement lowering should succeed");

    let saw_result_if = module.blocks.iter().any(|block| {
        matches!(
            block.terminator,
            HirTerminator::If {
                condition: crate::compiler_frontend::hir::expressions::HirExpression {
                    kind: HirExpressionKind::ResultIsOk { .. },
                    ..
                },
                ..
            }
        )
    });
    assert!(
        saw_result_if,
        "expected named-handler lowering to branch on ResultIsOk"
    );

    let saw_err_unwrap_assign = module.blocks.iter().any(|block| {
        block.statements.iter().any(|statement| {
            matches!(
                statement.kind,
                HirStatementKind::Assign {
                    value: crate::compiler_frontend::hir::expressions::HirExpression {
                        kind: HirExpressionKind::ResultUnwrapErr { .. },
                        ..
                    },
                    ..
                }
            )
        })
    });
    assert!(
        saw_err_unwrap_assign,
        "expected named-handler lowering to unwrap and bind the err branch payload"
    );
}

#[test]
fn multi_bind_lowering_projects_tuple_slots_from_single_rhs_call() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let pair_name = super::symbol("pair", &mut string_table);
    let location = test_location(20);

    let pair_function = function_node(
        pair_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_returns(vec![DataType::Int, DataType::StringSlice]),
        },
        vec![node(
            NodeKind::Return(vec![
                Expression::int(1, location.clone(), ValueMode::ImmutableOwned),
                Expression::string_slice(
                    string_table.intern("value"),
                    location.clone(),
                    ValueMode::ImmutableOwned,
                ),
            ]),
            location.clone(),
        )],
        location.clone(),
    );

    let left_id = start_name.join_str("left", &mut string_table);
    let right_id = start_name.join_str("right", &mut string_table);

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::MultiBind {
                targets: vec![
                    MultiBindTarget {
                        id: left_id,
                        data_type: DataType::Int,
                        value_mode: ValueMode::ImmutableOwned,
                        kind: MultiBindTargetKind::Declaration,
                        location: location.clone(),
                    },
                    MultiBindTarget {
                        id: right_id,
                        data_type: DataType::StringSlice,
                        value_mode: ValueMode::ImmutableOwned,
                        kind: MultiBindTargetKind::Declaration,
                        location: location.clone(),
                    },
                ],
                value: Expression::function_call(
                    pair_name,
                    vec![],
                    vec![DataType::Int, DataType::StringSlice],
                    location.clone(),
                ),
            },
            location.clone(),
        )],
        location.clone(),
    );

    let module = lower_ast(
        build_ast(vec![pair_function, start_function], entry_path),
        &mut string_table,
    )
    .expect("multi-bind lowering should succeed");

    let call_count = module
        .blocks
        .iter()
        .flat_map(|block| block.statements.iter())
        .filter(|statement| matches!(statement.kind, HirStatementKind::Call { .. }))
        .count();
    assert_eq!(
        call_count, 1,
        "multi-bind RHS call should lower exactly once"
    );

    let tuple_get_assignments = module
        .blocks
        .iter()
        .flat_map(|block| block.statements.iter())
        .filter(|statement| {
            matches!(
                statement.kind,
                HirStatementKind::Assign {
                    value: crate::compiler_frontend::hir::expressions::HirExpression {
                        kind: HirExpressionKind::TupleGet { .. },
                        ..
                    },
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        tuple_get_assignments, 2,
        "multi-bind lowering should assign both tuple slots in order"
    );
}
