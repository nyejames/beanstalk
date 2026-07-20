//! Borrow-checker fact-generation regression tests.
//!
//! WHAT: checks the low-level facts emitted for borrows, moves, assignments, and returns.
//! WHY: these facts are the borrow checker's source of truth, so targeted tests catch drift
//! before it reaches higher-level diagnostics.

use crate::compiler_frontend::analysis::borrow_checker::LocalMode;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::{DataType, builtin_type_ids};
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, HirMapOp};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::ids::{BlockId, HirNodeId, HirValueId, LocalId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::ast_fixture_support::{
    assignment_target, function_node, make_test_variable, node, reference_expr, symbol,
    test_location,
};
use crate::compiler_frontend::tests::borrow_fixture_support::run_borrow_checker;
use crate::compiler_frontend::tests::external_package_support::default_external_package_registry;
use crate::compiler_frontend::tests::hir_fixture_support::{build_ast, entry_and_start, lower_hir};
use crate::compiler_frontend::tests::parse_support::parse_single_file_ast;
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
                            target: assignment_target(
                                x.clone(),
                                DataType::Int,
                                builtin_type_ids::INT,
                                test_location(4),
                            ),
                            value: Expression::int(2, test_location(4), ValueMode::ImmutableOwned),
                        },
                        test_location(4),
                    )],
                    Some(vec![node(
                        NodeKind::Assignment {
                            target: assignment_target(
                                x.clone(),
                                DataType::Int,
                                builtin_type_ids::INT,
                                test_location(5),
                            ),
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
                    reference_expr(
                        first_ref,
                        DataType::Int,
                        builtin_type_ids::INT,
                        test_location(3),
                    ),
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

#[test]
fn statement_entry_state_marks_source_uninitialized_after_inferred_assignment_move() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let source = symbol("source", &mut string_table);
    let target = symbol("target", &mut string_table);
    let sentinel = symbol("sentinel", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    source.clone(),
                    Expression::int(7, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    target,
                    Expression::reference(
                        source,
                        DataType::Int,
                        test_location(11),
                        ValueMode::MutableOwned,
                    ),
                )),
                test_location(11),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    sentinel,
                    Expression::int(0, test_location(12), ValueMode::ImmutableOwned),
                )),
                test_location(12),
            ),
        ],
        test_location(2),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("inferred assignment move should pass");

    let source_local = find_assigned_local_for_line(&hir, 10)
        .expect("should locate the source local by declaration line");
    let target_local = find_assigned_local_for_line(&hir, 11)
        .expect("should locate the target local by declaration line");
    let sentinel_statement_id =
        find_statement_id_for_line(&hir, 12).expect("should locate the sentinel statement");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&sentinel_statement_id)
        .expect("sentinel statement should have an entry snapshot");
    let source_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == source_local)
        .expect("entry snapshot should include the source local");
    let target_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == target_local)
        .expect("entry snapshot should include the target local");

    assert!(
        source_snapshot.mode.contains(LocalMode::UNINIT),
        "moved-from source should be uninitialized at the sentinel statement, got source mode {:?} with aliases {:?}; target mode {:?} with aliases {:?}",
        source_snapshot.mode,
        source_snapshot.alias_roots,
        target_snapshot.mode,
        target_snapshot.alias_roots
    );
    assert!(
        target_snapshot.mode.contains(LocalMode::SLOT),
        "move target should own an independent slot at the sentinel statement, got mode {:?} with aliases {:?}",
        target_snapshot.mode,
        target_snapshot.alias_roots
    );
    assert!(
        target_snapshot.alias_roots.is_empty(),
        "move target should not retain alias roots at the sentinel statement"
    );
}

// WHAT: hidden map-operation transfer facts that integration output cannot inspect.
// WHY: Phase 6 integration owns user-visible map borrow behavior; these narrow state
//      assertions protect the receiver-alias shape, MayConsume last-use classification,
//      and recursive aggregate-literal ownership transfer.

#[test]
fn map_get_operation_result_alias_retains_receiver_root() {
    // WHAT: the first-class HIR map-operation result aliases the receiver root before catch
    //      handling transfers the success value.
    // WHY: later conflict analysis reads this alias state; integration only sees the
    //      resulting conflict, not which root the get binding aliases.
    let source = r#"scores ~{String = Int} = {"Ada" = 10}
score = scores.get("Ada") catch:
    then 0
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a get with no later mutation should pass");

    let scores_local = find_local_by_name(&hir, &string_table, "scores")
        .expect("should locate the receiver local by name");
    let (result_local, following_statement) =
        find_map_op_result_and_following_statement(&hir, HirMapOp::Get)
            .expect("should locate the get operation result and its consumer");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&following_statement)
        .expect("the operation-result consumer should have an entry snapshot");
    let result_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == result_local)
        .expect("entry snapshot should include the map-operation result");

    assert!(
        result_snapshot.mode.contains(LocalMode::ALIAS),
        "get result should alias the receiver, got mode {:?}",
        result_snapshot.mode
    );
    assert!(
        result_snapshot.alias_roots.contains(&scores_local),
        "get result alias root should be the receiver, got {:?}",
        result_snapshot.alias_roots
    );
}

#[test]
fn map_remove_result_is_fresh_owned() {
    // WHAT: the binding produced by fallible map `remove` is a fresh owned slot with no
    //      receiver alias root, unlike `get`.
    // WHY: the Fresh result-alias decision is a hidden transfer fact; if remove aliased
    //      the receiver, a later mutation would falsely conflict with the removed value.
    let source = r#"scores ~{String = String} = {"Ada" = "ten"}
removed = ~scores.remove("Ada") catch:
    then ""
;
sentinel = 0"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a remove with no later mutation should pass");

    let removed_local = find_local_by_name(&hir, &string_table, "removed")
        .expect("should locate the remove binding by name");
    let sentinel_statement =
        find_assign_statement_id_for_local_name(&hir, &string_table, "sentinel")
            .expect("should locate the sentinel statement by its assigned local");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&sentinel_statement)
        .expect("sentinel statement should have an entry snapshot");
    let result_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == removed_local)
        .expect("entry snapshot should include the remove binding");

    assert!(
        result_snapshot.mode.contains(LocalMode::SLOT),
        "remove result should own a fresh slot, got mode {:?}",
        result_snapshot.mode
    );
    assert!(
        !result_snapshot.mode.contains(LocalMode::ALIAS),
        "remove result should not alias the receiver, got mode {:?} with aliases {:?}",
        result_snapshot.mode,
        result_snapshot.alias_roots
    );
    assert!(
        result_snapshot.alias_roots.is_empty(),
        "remove result should carry no alias roots, got {:?}",
        result_snapshot.alias_roots
    );
}

#[test]
fn map_set_final_use_moves_inserted_non_copy_roots() {
    // WHAT: `set` MayConsume on final-use non-copy key and value inputs moves their roots.
    // WHY: last-use classification must consume ownership; a regression to a borrow would
    //      leave the root live, which integration output cannot distinguish from a move.
    let source = r#"scores ~{String = String} = {}
key ~= "key"
value ~= "hello"
~scores.set(key, value) catch:
;
sentinel = 0"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a final-use set with no later value use should pass");

    let sentinel_statement =
        find_assign_statement_id_for_local_name(&hir, &string_table, "sentinel")
            .expect("should locate the sentinel statement by its assigned local");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&sentinel_statement)
        .expect("sentinel statement should have an entry snapshot");
    for name in ["key", "value"] {
        let local = find_local_by_name(&hir, &string_table, name)
            .unwrap_or_else(|| panic!("should locate the inserted {name} local by name"));
        let snapshot = entry_state
            .locals
            .iter()
            .find(|snapshot| snapshot.local == local)
            .unwrap_or_else(|| panic!("entry snapshot should include the inserted {name} local"));

        assert!(
            snapshot.mode.is_definitely_uninit(),
            "final-use set should move the inserted {name} root, got mode {:?} with aliases {:?}",
            snapshot.mode,
            snapshot.alias_roots
        );
    }
}

#[test]
fn map_set_later_use_keeps_mutable_inputs_borrowed() {
    // WHAT: `set` MayConsume on later-use mutable key and value inputs borrows rather than moving.
    // WHY: last-use classification must not unconditionally move; the root stays live so
    //      the binding remains usable, which a regression to always-move would break.
    let source = r#"scores ~{String = String} = {}
key ~= "key"
value ~= "hello"
~scores.set(key, value) catch:
;
key_label = key
label = value
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a later-use mutable set should borrow and keep the value usable");

    let first_use_statement =
        find_assign_statement_id_for_local_name(&hir, &string_table, "key_label")
            .expect("should locate the first later-use statement by its assigned local");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&first_use_statement)
        .expect("first later-use statement should have an entry snapshot");

    for name in ["key", "value"] {
        let local = find_local_by_name(&hir, &string_table, name)
            .unwrap_or_else(|| panic!("should locate the inserted {name} local by name"));
        let snapshot = entry_state
            .locals
            .iter()
            .find(|snapshot| snapshot.local == local)
            .unwrap_or_else(|| panic!("entry snapshot should include the inserted {name} local"));

        assert!(
            snapshot.mode.contains(LocalMode::SLOT),
            "later-use set should keep the {name} as a live slot, got mode {:?}",
            snapshot.mode
        );
        assert!(
            !snapshot.mode.is_definitely_uninit(),
            "later-use set should not move the {name} root, got mode {:?} with aliases {:?}",
            snapshot.mode,
            snapshot.alias_roots
        );
    }
}

#[test]
fn nested_map_literal_moves_inner_non_copy_value_root() {
    // WHAT: a nested map literal recursively moves an inner inserted non-copy value.
    // WHY: aggregate ownership transfer must recurse into inner literals; integration
    //      only proves the outer rejection, not the inner root invalidation.
    let source = r#"value ~= "hello"
scores ~{String = {String = String}} = {"outer" = {"inner" = value}}
sentinel = 0"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a final-use nested literal with no later value use should pass");

    let value_local = find_local_by_name(&hir, &string_table, "value")
        .expect("should locate the inner inserted value local by name");
    let sentinel_statement =
        find_assign_statement_id_for_local_name(&hir, &string_table, "sentinel")
            .expect("should locate the sentinel statement by its assigned local");
    let entry_state = report
        .analysis
        .statement_entry_states
        .get(&sentinel_statement)
        .expect("sentinel statement should have an entry snapshot");
    let value_snapshot = entry_state
        .locals
        .iter()
        .find(|snapshot| snapshot.local == value_local)
        .expect("entry snapshot should include the inner inserted value local");

    assert!(
        value_snapshot.mode.is_definitely_uninit(),
        "nested literal should move the inner inserted value root, got mode {:?} with aliases {:?}",
        value_snapshot.mode,
        value_snapshot.alias_roots
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

fn find_local_by_name(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    string_table: &StringTable,
    name: &str,
) -> Option<LocalId> {
    hir.blocks
        .iter()
        .flat_map(|block| block.locals.iter())
        .find(|local| hir.side_table.resolve_local_name(local.id, string_table) == Some(name))
        .map(|local| local.id)
}

fn find_assign_statement_id_for_local_name(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    string_table: &StringTable,
    name: &str,
) -> Option<HirNodeId> {
    for block in &hir.blocks {
        for statement in &block.statements {
            if let HirStatementKind::Assign {
                target: HirPlace::Local(local),
                ..
            } = &statement.kind
                && hir.side_table.resolve_local_name(*local, string_table) == Some(name)
            {
                return Some(statement.id);
            }
        }
    }
    None
}

/// Finds the semantic result state immediately after a first-class HIR map operation.
fn find_map_op_result_and_following_statement(
    hir: &crate::compiler_frontend::hir::module::HirModule,
    wanted_op: HirMapOp,
) -> Option<(LocalId, HirNodeId)> {
    for block in &hir.blocks {
        for (index, statement) in block.statements.iter().enumerate() {
            if let HirStatementKind::MapOp { op, result, .. } = &statement.kind
                && *op == wanted_op
                && let Some(result_local) = *result
                && let Some(following_statement) = block.statements.get(index + 1)
            {
                return Some((result_local, following_statement.id));
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
            HirTerminator::FallibleBranch {
                success_block,
                error_block,
                ..
            } => {
                queue.push_back(*success_block);
                queue.push_back(*error_block);
            }
            HirTerminator::Match { arms, .. } => {
                for arm in arms {
                    queue.push_back(arm.body);
                }
            }
            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                queue.push_back(*target);
            }
            HirTerminator::Return(_)
            | HirTerminator::ReturnSuccess(_)
            | HirTerminator::ReturnError(_)
            | HirTerminator::RuntimeFailure { .. }
            | HirTerminator::Uninitialized
            | HirTerminator::AssertFailure { .. } => {}
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
        HirStatementKind::MapOp { receiver, args, .. } => {
            collect_expression_values(&receiver, out);
            for arg in args {
                collect_expression_values(&arg, out);
            }
        }
        HirStatementKind::Expr(expr) => collect_expression_values(&expr, out),
        HirStatementKind::CastOp { source, .. } => collect_expression_values(&source, out),
        HirStatementKind::NumericOp { operands, .. } => match operands {
            crate::compiler_frontend::hir::numeric::HirNumericOperands::Unary { operand } => {
                collect_expression_values(&operand, out);
            }
            crate::compiler_frontend::hir::numeric::HirNumericOperands::Binary { left, right } => {
                collect_expression_values(&left, out);
                collect_expression_values(&right, out);
            }
        },
        HirStatementKind::FormatFloat { source, .. }
        | HirStatementKind::ValidateFloat { source, .. } => collect_expression_values(&source, out),
        HirStatementKind::Drop(_) => {}
        HirStatementKind::PushRuntimeFragment { value, .. } => {
            collect_expression_values(&value, out)
        }
    }
}

fn collect_terminator_values(terminator: &HirTerminator, out: &mut FxHashSet<HirValueId>) {
    match terminator {
        HirTerminator::If { condition, .. } => collect_expression_values(condition, out),
        HirTerminator::FallibleBranch { result, .. } => collect_expression_values(result, out),
        HirTerminator::Match { scrutinee, arms } => {
            collect_expression_values(scrutinee, out);
            for arm in arms {
                if let crate::compiler_frontend::hir::patterns::HirPattern::Literal(value)
                | crate::compiler_frontend::hir::patterns::HirPattern::OptionValue { value } =
                    &arm.pattern
                {
                    collect_expression_values(value, out);
                }
                if let Some(guard) = &arm.guard {
                    collect_expression_values(guard, out);
                }
            }
        }
        HirTerminator::Return(value)
        | HirTerminator::ReturnSuccess(value)
        | HirTerminator::ReturnError(value) => collect_expression_values(value, out),
        HirTerminator::AssertFailure { .. } => {
            // Assertion messages are compile-time text, not expressions.
        }

        HirTerminator::RuntimeFailure { .. } => {
            // Runtime-failure messages are backend-facing text, not expressions.
        }

        HirTerminator::Uninitialized => {
            // Internal placeholder — no expressions to visit.
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
        HirExpressionKind::MapLiteral(entries) => {
            for entry in entries {
                collect_expression_values(&entry.key, out);
                collect_expression_values(&entry.value, out);
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
        HirExpressionKind::FallibleUnwrapSuccess { result }
        | HirExpressionKind::FallibleUnwrapError { result }
        | HirExpressionKind::Cast { source: result, .. } => {
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
