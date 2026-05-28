//! HIR branch (`if`/`else`) lowering regression tests.
//!
//! WHAT: checks how conditional branches lower into HIR blocks with `If` terminators and merge
//!       continuation blocks.
//! WHY: branch lowering constructs the core CFG diamond shape; errors here corrupt control flow
//!      and variable liveness across arms.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::value_production::{
    ProducedValues,
    types::{ValueBlock, ValueIfBlock},
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::HirExpressionKind;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    function_node, make_test_variable, node, runtime_function_call_node, runtime_operator_node,
    test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;

use crate::compiler_frontend::hir::hir_builder::{
    assert_no_placeholder_terminators, build_ast, lower_ast,
};
use crate::compiler_frontend::tests::type_id_fixture_support::{
    fresh_success_returns, reference_expr, runtime_expr,
};

fn blocks_with_user_function_call(module: &HirModule, function_id: FunctionId) -> Vec<usize> {
    module
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let has_call = block.statements.iter().any(|statement| {
                matches!(
                    statement.kind,
                    HirStatementKind::Call {
                        target: CallTarget::UserFunction(target_id),
                        ..
                    } if target_id == function_id
                )
            });
            if has_call { Some(index) } else { None }
        })
        .collect()
}

fn value_block_result_assignment(
    module: &HirModule,
    block_id: BlockId,
) -> (LocalId, HirExpressionKind, BlockId) {
    let block = &module.blocks[block_id.0 as usize];
    let (result_local, value_kind) = block
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            HirStatementKind::Assign {
                target: HirPlace::Local(local),
                value,
            } => Some((*local, value.kind.clone())),
            _ => None,
        })
        .expect("value-block branch should assign a hidden result local");

    let merge_block = match block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("value-block branch should jump to the merge block"),
    };

    (result_local, value_kind, merge_block)
}

#[test]
fn lowers_if_to_then_else_merge_blocks() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);
    let y = super::symbol("y", &mut string_table);

    let if_node = node(
        NodeKind::If(
            Expression::bool(true, test_location(2), ValueMode::ImmutableOwned),
            vec![node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x,
                    Expression::int(1, test_location(2), ValueMode::ImmutableOwned),
                )),
                test_location(2),
            )],
            Some(vec![node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    Expression::int(2, test_location(3), ValueMode::ImmutableOwned),
                )),
                test_location(3),
            )]),
        ),
        test_location(2),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![if_node],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let (module, _type_environment) =
        lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let (then_block, else_block) = match entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected if terminator in entry block"),
    };

    assert!(matches!(
        module.blocks[then_block.0 as usize].terminator,
        HirTerminator::Jump { .. }
    ));
    assert!(matches!(
        module.blocks[else_block.0 as usize].terminator,
        HirTerminator::Jump { .. }
    ));
}

#[test]
fn short_circuit_and_keeps_rhs_call_off_always_run_path() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let rhs_name = super::symbol("rhs_and", &mut string_table);
    let location = test_location(30);

    let rhs_fn = function_node(
        rhs_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_success_returns(vec![builtin_type_ids::BOOL]),
        },
        vec![node(
            NodeKind::Return(vec![Expression::bool(
                true,
                location.clone(),
                ValueMode::ImmutableOwned,
            )]),
            location.clone(),
        )],
        location.clone(),
    );

    let condition = runtime_expr(
        vec![
            node(
                NodeKind::Rvalue(Expression::bool(
                    false,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location.clone(),
            ),
            runtime_function_call_node(
                rhs_name.clone(),
                vec![builtin_type_ids::BOOL],
                location.clone(),
            ),
            runtime_operator_node(Operator::And, location.clone()),
        ],
        builtin_type_ids::BOOL,
        location.clone(),
        ValueMode::MutableOwned,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::If(
                condition,
                vec![node(
                    NodeKind::Rvalue(Expression::int(
                        1,
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )),
                    location.clone(),
                )],
                None,
            ),
            location.clone(),
        )],
        location.clone(),
    );

    let (module, _type_environment) = lower_ast(
        build_ast(vec![rhs_fn, start_fn], entry_path),
        &mut string_table,
    )
    .expect("short-circuit and lowering should succeed");
    let rhs_function_id = module
        .functions
        .iter()
        .find(|function| function.id != module.start_function)
        .expect("rhs function should be present")
        .id;
    let start_function = module
        .functions
        .iter()
        .find(|function| function.id == module.start_function)
        .expect("start function should exist");
    let start_entry_index = start_function.entry.0 as usize;
    let start_entry_block = &module.blocks[start_entry_index];

    let (then_block, else_block) = match start_entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected short-circuit dispatcher if terminator in entry block"),
    };

    assert!(
        start_entry_block
            .statements
            .iter()
            .all(|statement| !matches!(statement.kind, HirStatementKind::Call { .. })),
        "entry block should not eagerly execute rhs calls before short-circuit dispatch"
    );

    let call_blocks = blocks_with_user_function_call(&module, rhs_function_id);
    assert_eq!(
        call_blocks.len(),
        1,
        "rhs function call should appear in exactly one guarded branch"
    );
    assert_eq!(call_blocks[0], then_block.0 as usize);
    assert_ne!(call_blocks[0], else_block.0 as usize);

    let rhs_branch_block = &module.blocks[then_block.0 as usize];
    let short_branch_block = &module.blocks[else_block.0 as usize];
    let (rhs_merge_target, rhs_jump_args) = match &rhs_branch_block.terminator {
        HirTerminator::Jump { target, args } => (*target, args.as_slice()),
        _ => panic!("rhs short-circuit branch should jump to merge block"),
    };
    let (short_merge_target, short_jump_args) = match &short_branch_block.terminator {
        HirTerminator::Jump { target, args } => (*target, args.as_slice()),
        _ => panic!("short short-circuit branch should jump to merge block"),
    };
    assert_eq!(
        rhs_merge_target, short_merge_target,
        "short-circuit branches should rejoin at one merge block"
    );
    assert_eq!(
        rhs_jump_args.len(),
        1,
        "rhs short-circuit branch should pass one merge argument"
    );
    assert_eq!(
        short_jump_args.len(),
        1,
        "short short-circuit branch should pass one merge argument"
    );
    assert!(
        rhs_branch_block.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local),
                    ..
                } if *local == rhs_jump_args[0]
            )
        }),
        "rhs short-circuit branch should materialize its jump argument local"
    );
    assert!(
        short_branch_block.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local),
                    ..
                } if *local == short_jump_args[0]
            )
        }),
        "short short-circuit branch should materialize its jump argument local"
    );
    let merge_block = &module.blocks[rhs_merge_target.0 as usize];
    assert!(
        !merge_block.locals.is_empty(),
        "merge block should declare a destination local for branch arguments"
    );
}

#[test]
fn short_circuit_or_keeps_rhs_call_off_true_short_path() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let rhs_name = super::symbol("rhs_or", &mut string_table);
    let location = test_location(40);

    let rhs_fn = function_node(
        rhs_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_success_returns(vec![builtin_type_ids::BOOL]),
        },
        vec![node(
            NodeKind::Return(vec![Expression::bool(
                false,
                location.clone(),
                ValueMode::ImmutableOwned,
            )]),
            location.clone(),
        )],
        location.clone(),
    );

    let condition = runtime_expr(
        vec![
            node(
                NodeKind::Rvalue(Expression::bool(
                    true,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location.clone(),
            ),
            runtime_function_call_node(
                rhs_name.clone(),
                vec![builtin_type_ids::BOOL],
                location.clone(),
            ),
            runtime_operator_node(Operator::Or, location.clone()),
        ],
        builtin_type_ids::BOOL,
        location.clone(),
        ValueMode::MutableOwned,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::If(
                condition,
                vec![node(
                    NodeKind::Rvalue(Expression::int(
                        1,
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )),
                    location.clone(),
                )],
                None,
            ),
            location.clone(),
        )],
        location.clone(),
    );

    let (module, _type_environment) = lower_ast(
        build_ast(vec![rhs_fn, start_fn], entry_path),
        &mut string_table,
    )
    .expect("short-circuit or lowering should succeed");
    let rhs_function_id = module
        .functions
        .iter()
        .find(|function| function.id != module.start_function)
        .expect("rhs function should be present")
        .id;
    let start_function = module
        .functions
        .iter()
        .find(|function| function.id == module.start_function)
        .expect("start function should exist");
    let start_entry_index = start_function.entry.0 as usize;
    let start_entry_block = &module.blocks[start_entry_index];

    let (then_block, else_block) = match start_entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected short-circuit dispatcher if terminator in entry block"),
    };

    let call_blocks = blocks_with_user_function_call(&module, rhs_function_id);
    assert_eq!(
        call_blocks.len(),
        1,
        "rhs function call should appear in exactly one guarded branch"
    );
    assert_eq!(call_blocks[0], else_block.0 as usize);
    assert_ne!(call_blocks[0], then_block.0 as usize);

    let rhs_branch_block = &module.blocks[else_block.0 as usize];
    let short_branch_block = &module.blocks[then_block.0 as usize];
    let (rhs_merge_target, rhs_jump_args) = match &rhs_branch_block.terminator {
        HirTerminator::Jump { target, args } => (*target, args.as_slice()),
        _ => panic!("rhs short-circuit branch should jump to merge block"),
    };
    let (short_merge_target, short_jump_args) = match &short_branch_block.terminator {
        HirTerminator::Jump { target, args } => (*target, args.as_slice()),
        _ => panic!("short short-circuit branch should jump to merge block"),
    };
    assert_eq!(
        rhs_merge_target, short_merge_target,
        "short-circuit branches should rejoin at one merge block"
    );
    assert_eq!(
        rhs_jump_args.len(),
        1,
        "rhs short-circuit branch should pass one merge argument"
    );
    assert_eq!(
        short_jump_args.len(),
        1,
        "short short-circuit branch should pass one merge argument"
    );
    assert!(
        rhs_branch_block.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local),
                    ..
                } if *local == rhs_jump_args[0]
            )
        }),
        "rhs short-circuit branch should materialize its jump argument local"
    );
    assert!(
        short_branch_block.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local),
                    ..
                } if *local == short_jump_args[0]
            )
        }),
        "short short-circuit branch should materialize its jump argument local"
    );
    let merge_block = &module.blocks[rhs_merge_target.0 as usize];
    assert!(
        !merge_block.locals.is_empty(),
        "merge block should declare a destination local for branch arguments"
    );
}

#[test]
fn if_condition_with_runtime_logical_expression_lowers_to_two_stage_cfg() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let rhs_name = super::symbol("rhs_if_condition", &mut string_table);
    let location = test_location(50);

    let rhs_fn = function_node(
        rhs_name.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_success_returns(vec![builtin_type_ids::BOOL]),
        },
        vec![node(
            NodeKind::Return(vec![Expression::bool(
                true,
                location.clone(),
                ValueMode::ImmutableOwned,
            )]),
            location.clone(),
        )],
        location.clone(),
    );

    let condition = runtime_expr(
        vec![
            node(
                NodeKind::Rvalue(Expression::int(
                    1,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location.clone(),
            ),
            node(
                NodeKind::Rvalue(Expression::int(
                    2,
                    location.clone(),
                    ValueMode::ImmutableOwned,
                )),
                location.clone(),
            ),
            runtime_operator_node(Operator::LessThan, location.clone()),
            runtime_function_call_node(rhs_name, vec![builtin_type_ids::BOOL], location.clone()),
            runtime_operator_node(Operator::And, location.clone()),
        ],
        builtin_type_ids::BOOL,
        location.clone(),
        ValueMode::MutableOwned,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::If(
                condition,
                vec![node(
                    NodeKind::Rvalue(Expression::int(
                        1,
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )),
                    location.clone(),
                )],
                Some(vec![node(
                    NodeKind::Rvalue(Expression::int(
                        2,
                        location.clone(),
                        ValueMode::ImmutableOwned,
                    )),
                    location.clone(),
                )]),
            ),
            location.clone(),
        )],
        location.clone(),
    );

    let (module, _type_environment) = lower_ast(
        build_ast(vec![rhs_fn, start_fn], entry_path),
        &mut string_table,
    )
    .expect("if condition lowering should succeed");

    let if_terminator_count = module
        .blocks
        .iter()
        .filter(|block| matches!(block.terminator, HirTerminator::If { .. }))
        .count();
    assert!(
        if_terminator_count >= 2,
        "expected separate if terminators for short-circuit dispatch and statement branching"
    );
}

#[test]
fn short_circuit_place_rhs_materializes_copy_before_merge_assignment() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let lhs_name = super::symbol("lhs", &mut string_table);
    let rhs_name = super::symbol("rhs", &mut string_table);
    let location = test_location(60);

    let condition = runtime_expr(
        vec![
            node(
                NodeKind::Rvalue(reference_expr(
                    lhs_name.clone(),
                    builtin_type_ids::BOOL,
                    location.clone(),
                    ValueMode::ImmutableReference,
                )),
                location.clone(),
            ),
            node(
                NodeKind::Rvalue(reference_expr(
                    rhs_name.clone(),
                    builtin_type_ids::BOOL,
                    location.clone(),
                    ValueMode::ImmutableReference,
                )),
                location.clone(),
            ),
            runtime_operator_node(Operator::And, location.clone()),
        ],
        builtin_type_ids::BOOL,
        location.clone(),
        ValueMode::MutableOwned,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    lhs_name,
                    Expression::bool(false, location.clone(), ValueMode::ImmutableOwned),
                )),
                location.clone(),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    rhs_name,
                    Expression::bool(true, location.clone(), ValueMode::MutableOwned),
                )),
                location.clone(),
            ),
            node(
                NodeKind::If(
                    condition,
                    vec![node(
                        NodeKind::Rvalue(Expression::int(
                            1,
                            location.clone(),
                            ValueMode::ImmutableOwned,
                        )),
                        location.clone(),
                    )],
                    None,
                ),
                location.clone(),
            ),
        ],
        location.clone(),
    );

    let (module, _type_environment) =
        lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
            .expect("short-circuit place rhs lowering should succeed");
    let start_function = module
        .functions
        .iter()
        .find(|function| function.id == module.start_function)
        .expect("start function should exist");
    let start_entry_block = &module.blocks[start_function.entry.0 as usize];
    let (rhs_block, _short_block) = match start_entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected short-circuit dispatcher if terminator in entry block"),
    };

    let rhs_branch_block = &module.blocks[rhs_block.0 as usize];
    let rhs_jump_arg_local = match &rhs_branch_block.terminator {
        HirTerminator::Jump { args, .. } if args.len() == 1 => args[0],
        _ => panic!("rhs short-circuit branch should jump with one merge argument"),
    };
    let rhs_jump_arg_assignment = rhs_branch_block
        .statements
        .iter()
        .find_map(|statement| match &statement.kind {
            HirStatementKind::Assign {
                target: HirPlace::Local(local),
                value,
            } if *local == rhs_jump_arg_local => Some(value),
            _ => None,
        })
        .expect("rhs short-circuit branch should assign the jump argument local");

    assert!(
        matches!(
            rhs_jump_arg_assignment.kind,
            HirExpressionKind::Copy(HirPlace::Local(_))
        ),
        "rhs place loads should be materialized as Copy before jump-argument assignment"
    );
}

#[test]
fn value_if_then_place_materializes_copy_before_hidden_result_assignment() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let left_name = super::symbol("left", &mut string_table);
    let right_name = super::symbol("right", &mut string_table);
    let result_name = super::symbol("result", &mut string_table);
    let location = test_location(70);

    let then_body = vec![node(
        NodeKind::ThenValue(ProducedValues {
            expressions: vec![reference_expr(
                left_name.clone(),
                builtin_type_ids::INT,
                location.clone(),
                ValueMode::ImmutableReference,
            )],
            location: location.clone(),
        }),
        location.clone(),
    )];

    let else_body = vec![node(
        NodeKind::ThenValue(ProducedValues {
            expressions: vec![reference_expr(
                right_name.clone(),
                builtin_type_ids::INT,
                location.clone(),
                ValueMode::ImmutableReference,
            )],
            location: location.clone(),
        }),
        location.clone(),
    )];

    let value_if_expression = Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::If(ValueIfBlock {
                condition: Expression::bool(true, location.clone(), ValueMode::ImmutableOwned),
                then_body,
                else_body,
                location: location.clone(),
                result_type_ids: vec![builtin_type_ids::INT],
            })),
        },
        location.clone(),
        builtin_type_ids::INT,
        DataType::Inferred,
        ValueMode::ImmutableOwned,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    left_name,
                    Expression::int(1, location.clone(), ValueMode::ImmutableOwned),
                )),
                location.clone(),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    right_name,
                    Expression::int(2, location.clone(), ValueMode::ImmutableOwned),
                )),
                location.clone(),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(result_name, value_if_expression)),
                location.clone(),
            ),
        ],
        location.clone(),
    );

    let (module, _type_environment) =
        lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
            .expect("value-if place production should lower successfully");
    let start_function = module
        .functions
        .iter()
        .find(|function| function.id == module.start_function)
        .expect("start function should exist");
    let entry_block = &module.blocks[start_function.entry.0 as usize];
    let (then_block, else_block) = match entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected value-if dispatcher terminator"),
    };

    let (then_result_local, then_value_kind, then_merge) =
        value_block_result_assignment(&module, then_block);
    let (else_result_local, else_value_kind, else_merge) =
        value_block_result_assignment(&module, else_block);

    assert_eq!(
        then_result_local, else_result_local,
        "both branches should write the shared hidden result local"
    );
    assert_eq!(
        then_merge, else_merge,
        "both branches should rejoin at the shared value-if merge block"
    );
    assert!(
        matches!(then_value_kind, HirExpressionKind::Copy(HirPlace::Local(_))),
        "then name should be copied before assigning the hidden result local"
    );
    assert!(
        matches!(else_value_kind, HirExpressionKind::Copy(HirPlace::Local(_))),
        "else name should be copied before assigning the hidden result local"
    );
}

#[test]
fn non_unit_function_with_terminal_if_does_not_report_fallthrough() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let chooser = super::symbol("chooser", &mut string_table);

    let chooser_fn = function_node(
        chooser,
        FunctionSignature {
            parameters: vec![],
            returns: fresh_success_returns(vec![builtin_type_ids::INT]),
        },
        vec![node(
            NodeKind::If(
                Expression::bool(true, test_location(8), ValueMode::ImmutableOwned),
                vec![node(
                    NodeKind::Return(vec![Expression::int(
                        1,
                        test_location(8),
                        ValueMode::ImmutableOwned,
                    )]),
                    test_location(8),
                )],
                Some(vec![node(
                    NodeKind::Return(vec![Expression::int(
                        2,
                        test_location(9),
                        ValueMode::ImmutableOwned,
                    )]),
                    test_location(9),
                )]),
            ),
            test_location(8),
        )],
        test_location(7),
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
    let (module, _type_environment) =
        lower_ast(ast, &mut string_table).expect("all-terminal if should not trigger fallthrough");

    let chooser_block = &module.blocks[module.functions[1].entry.0 as usize];
    assert!(matches!(chooser_block.terminator, HirTerminator::If { .. }));
    assert_no_placeholder_terminators(&module);
}
