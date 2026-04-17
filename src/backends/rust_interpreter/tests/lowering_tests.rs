//! HIR -> Exec IR structure tests for the Rust interpreter backend.
//!
//! WHAT: verifies that the lowering pass produces correctly shaped Exec IR for the supported
//!       HIR subset, without executing anything.
//! WHY: keeping lowering assertions separate from execution behavior makes it easier to
//!      diagnose regressions in the lowering pass itself.

use super::test_support::{
    bool_expression, build_module, build_type_context, int_expression, load_local_expression,
    local, lower_only, statement, string_expression, unit_expression,
};
use crate::backends::rust_interpreter::exec_ir::{ExecConstValue, ExecInstruction, ExecTerminator};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBlock, HirFunction, HirFunctionOrigin, HirPlace, HirStatementKind,
    HirTerminator, LocalId, RegionId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

// ============================================================
// Test 1: minimal unit-returning start function
// ============================================================

#[test]
fn lowers_minimal_unit_return_start_function() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(1, types.unit, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    assert_eq!(
        exec_program.module.entry_function,
        Some(crate::backends::rust_interpreter::exec_ir::ExecFunctionId(
            0
        ))
    );
    assert_eq!(exec_program.module.functions.len(), 1);

    let function = &exec_program.module.functions[0];
    assert_eq!(function.blocks.len(), 1);

    let entry_block = &function.blocks[0];
    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: None }
        ),
        "expected Return {{ value: None }}, got {:?}",
        entry_block.terminator
    );
}

// ============================================================
// Test 2: integer literal lowers to LoadConst then Return
// ============================================================

#[test]
fn lowers_integer_return_into_load_const_then_return() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(1, 42, types.int, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let any_int_42 = exec_program
        .module
        .constants
        .iter()
        .any(|c| matches!(&c.value, ExecConstValue::Int(42)));
    assert!(any_int_42, "expected ExecConstValue::Int(42) in constants");

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    let has_load_const = entry_block
        .instructions
        .iter()
        .any(|inst| matches!(inst, ExecInstruction::LoadConst { .. }));
    assert!(
        has_load_const,
        "expected LoadConst instruction in entry block"
    );

    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: Some(_) }
        ),
        "expected Return with a value, got {:?}",
        entry_block.terminator
    );
}

// ============================================================
// Test 3: assignment + load local path
// ============================================================

#[test]
fn lowers_assignment_to_local_then_return_load_local() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);
    let local_x = LocalId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![local(0, types.int, region)],
        statements: vec![statement(
            1,
            HirStatementKind::Assign {
                target: HirPlace::Local(local_x),
                value: int_expression(100, 42, types.int, region),
            },
            1,
        )],
        terminator: HirTerminator::Return(load_local_expression(101, local_x, types.int, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    let has_load_const = entry_block
        .instructions
        .iter()
        .any(|inst| matches!(inst, ExecInstruction::LoadConst { .. }));
    assert!(has_load_const, "expected LoadConst from assignment");

    // WHAT: with ExecValue optimization, Load expressions return ExecValue::Local directly
    // WHY: no ReadLocal instruction is emitted during expression lowering; the local reference
    //      is passed directly to the terminator
    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: Some(_) }
        ),
        "expected Return with a value, got {:?}",
        entry_block.terminator
    );
}

// ============================================================
// Test 4: If terminator lowers to BranchBool
// ============================================================

#[test]
fn lowers_if_terminator_to_branch_bool() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let entry_block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::If {
            condition: bool_expression(1, true, types.boolean, region),
            then_block: BlockId(1),
            else_block: BlockId(2),
        },
    };
    let then_block = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(2, 1, types.int, region)),
    };
    let else_block = HirBlock {
        id: BlockId(2),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(3, 0, types.int, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![entry_block, then_block, else_block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let function = &exec_program.module.functions[0];
    let entry = &function.blocks[0];

    let (then_id, else_id) = match &entry.terminator {
        ExecTerminator::BranchBool {
            then_block,
            else_block,
            ..
        } => (*then_block, *else_block),
        other => panic!("expected BranchBool terminator, got {:?}", other),
    };

    assert_ne!(
        then_id, else_id,
        "then_block and else_block should be distinct"
    );
    assert!(
        function.blocks.iter().any(|b| b.id == then_id),
        "then_block id should resolve to a real block"
    );
    assert!(
        function.blocks.iter().any(|b| b.id == else_id),
        "else_block id should resolve to a real block"
    );
}

// ============================================================
// Test 5: Jump / Break / Continue all lower to ExecTerminator::Jump
// ============================================================

#[test]
fn lowers_jump_break_continue_to_jump_terminators() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    // --- Jump ---
    let jump_entry = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Jump {
            target: BlockId(1),
            args: vec![],
        },
    };
    let jump_exit = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(1, types.unit, region)),
    };

    let function_jump = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module_jump = build_module(
        &mut string_table,
        vec![(function_jump, path, HirFunctionOrigin::EntryStart)],
        vec![jump_entry, jump_exit],
        type_context.clone(),
        FunctionId(0),
    );

    let exec_jump = lower_only(&module_jump);
    let jump_block = &exec_jump.module.functions[0].blocks[0];
    assert!(
        matches!(&jump_block.terminator, ExecTerminator::Jump { .. }),
        "Jump should lower to ExecTerminator::Jump"
    );

    // --- Break ---
    let break_entry = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Break { target: BlockId(1) },
    };
    let break_exit = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, region)),
    };

    let function_break = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let mut string_table2 = StringTable::new();
    let path2 = InternedPath::from_single_str("start", &mut string_table2);
    let module_break = build_module(
        &mut string_table2,
        vec![(function_break, path2, HirFunctionOrigin::EntryStart)],
        vec![break_entry, break_exit],
        type_context.clone(),
        FunctionId(0),
    );

    let exec_break = lower_only(&module_break);
    let break_block = &exec_break.module.functions[0].blocks[0];
    assert!(
        matches!(&break_block.terminator, ExecTerminator::Jump { .. }),
        "Break should lower to ExecTerminator::Jump"
    );

    // --- Continue ---
    let continue_entry = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Continue { target: BlockId(1) },
    };
    let continue_exit = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, region)),
    };

    let function_continue = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let mut string_table3 = StringTable::new();
    let path3 = InternedPath::from_single_str("start", &mut string_table3);
    let module_continue = build_module(
        &mut string_table3,
        vec![(function_continue, path3, HirFunctionOrigin::EntryStart)],
        vec![continue_entry, continue_exit],
        type_context,
        FunctionId(0),
    );

    let exec_continue = lower_only(&module_continue);
    let continue_block = &exec_continue.module.functions[0].blocks[0];
    assert!(
        matches!(&continue_block.terminator, ExecTerminator::Jump { .. }),
        "Continue should lower to ExecTerminator::Jump"
    );
}

// ============================================================
// Test 6: string literal lowers to ExecConstValue::String
// ============================================================

#[test]
fn lowers_string_literal_to_exec_const_string() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(string_expression(1, "hello", types.string, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.string,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let any_string_hello = exec_program
        .module
        .constants
        .iter()
        .any(|c| matches!(&c.value, ExecConstValue::String(s) if s == "hello"));
    assert!(
        any_string_hello,
        "expected ExecConstValue::String(\"hello\") in constants"
    );

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    assert!(
        entry_block
            .instructions
            .iter()
            .any(|inst| matches!(inst, ExecInstruction::LoadConst { .. })),
        "expected LoadConst instruction for string literal"
    );

    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: Some(_) }
        ),
        "expected Return with a value for string return"
    );

    let no_handle_in_constants = exec_program
        .module
        .constants
        .iter()
        .all(|c| !matches!(&c.value, ExecConstValue::Unit));
    assert!(no_handle_in_constants, "no fake Unit constants expected");
}

// ============================================================
// Test 7: empty tuple construct lowers as ExecConstValue::Unit via Expr statement
// ============================================================

#[test]
fn lowers_empty_tuple_construct_as_unit_constant() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![statement(
            1,
            HirStatementKind::Expr(unit_expression(100, types.unit, region)),
            1,
        )],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let any_unit = exec_program
        .module
        .constants
        .iter()
        .any(|c| matches!(&c.value, ExecConstValue::Unit));
    assert!(
        any_unit,
        "expected ExecConstValue::Unit in constants from TupleConstruct expression"
    );

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    assert!(
        entry_block
            .instructions
            .iter()
            .any(|inst| matches!(inst, ExecInstruction::LoadConst { .. })),
        "expected LoadConst for unit expression"
    );

    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: None }
        ),
        "unit return should produce Return {{ value: None }}, got {:?}",
        entry_block.terminator
    );
}

// ============================================================
// Test 8: copy local lowers to CopyLocal instruction
// ============================================================

#[test]
fn lowers_copy_local_to_copy_local_instruction() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);
    let local_x = LocalId(0);
    let local_y = LocalId(1);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![local(0, types.int, region), local(1, types.int, region)],
        statements: vec![
            statement(
                1,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local_x),
                    value: int_expression(100, 7, types.int, region),
                },
                1,
            ),
            statement(
                2,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local_y),
                    value: super::test_support::copy_local_expression(
                        101, local_x, types.int, region,
                    ),
                },
                2,
            ),
        ],
        terminator: HirTerminator::Return(load_local_expression(102, local_y, types.int, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    assert!(
        entry_block
            .instructions
            .iter()
            .any(|inst| matches!(inst, ExecInstruction::CopyLocal { .. })),
        "expected CopyLocal instruction from Copy(Local) expression"
    );
}

// ============================================================
// Test 9: entry function id is set correctly
// ============================================================

#[test]
fn lowers_entry_function_id_matches_start_function() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(1, types.unit, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let entry_id = exec_program
        .module
        .entry_function
        .expect("entry_function must be set");
    assert!(
        exec_program
            .module
            .functions
            .iter()
            .any(|f| f.id == entry_id),
        "entry_function id must point to a real function"
    );

    let entry_fn = exec_program
        .module
        .functions
        .iter()
        .find(|f| f.id == entry_id)
        .unwrap();
    assert!(
        entry_fn.flags.is_start,
        "entry function should have is_start flag set"
    );
}

// ============================================================
// Test: Binary operation lowering
// ============================================================

#[test]
fn lowers_simple_binary_operation() {
    use crate::compiler_frontend::hir::hir_nodes::{HirBinOp, HirExpression, HirExpressionKind};

    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    // Create a simple binary operation: 1 + 2
    let left = Box::new(int_expression(1, 1, types.int, region));
    let right = Box::new(int_expression(2, 2, types.int, region));
    let add_expr = HirExpression {
        id: crate::compiler_frontend::hir::hir_nodes::HirValueId(3),
        kind: HirExpressionKind::BinOp {
            left,
            op: HirBinOp::Add,
            right,
        },
        ty: types.int,
        value_kind: crate::compiler_frontend::hir::hir_nodes::ValueKind::RValue,
        region,
    };

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(add_expr),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    // Verify that BinaryOp instruction was emitted
    let has_binary_op = entry_block
        .instructions
        .iter()
        .any(|inst| matches!(inst, ExecInstruction::BinaryOp { .. }));
    assert!(
        has_binary_op,
        "expected BinaryOp instruction for binary operation"
    );

    // Verify that temporary locals were allocated for the literals
    let load_const_count = entry_block
        .instructions
        .iter()
        .filter(|inst| matches!(inst, ExecInstruction::LoadConst { .. }))
        .count();
    assert_eq!(
        load_const_count, 2,
        "expected 2 LoadConst instructions for the two literal operands"
    );

    // Verify return terminator exists
    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: Some(_) }
        ),
        "expected Return with a value, got {:?}",
        entry_block.terminator
    );
}

#[test]
fn lowers_simple_unary_operation() {
    use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirUnaryOp};

    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    // Create a simple unary operation: -42
    let operand = Box::new(int_expression(42, 1, types.int, region));
    let negate_expr = HirExpression {
        id: crate::compiler_frontend::hir::hir_nodes::HirValueId(2),
        kind: HirExpressionKind::UnaryOp {
            op: HirUnaryOp::Neg,
            operand,
        },
        ty: types.int,
        value_kind: crate::compiler_frontend::hir::hir_nodes::ValueKind::RValue,
        region,
    };

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(negate_expr),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);

    let function = &exec_program.module.functions[0];
    let entry_block = &function.blocks[0];

    // Verify that UnaryOp instruction was emitted
    let has_unary_op = entry_block
        .instructions
        .iter()
        .any(|inst| matches!(inst, ExecInstruction::UnaryOp { .. }));
    assert!(
        has_unary_op,
        "expected UnaryOp instruction for unary operation"
    );

    // Verify that temporary local was allocated for the literal operand
    let load_const_count = entry_block
        .instructions
        .iter()
        .filter(|inst| matches!(inst, ExecInstruction::LoadConst { .. }))
        .count();
    assert_eq!(
        load_const_count, 1,
        "expected 1 LoadConst instruction for the literal operand"
    );

    // Verify return terminator exists
    assert!(
        matches!(
            &entry_block.terminator,
            ExecTerminator::Return { value: Some(_) }
        ),
        "expected Return with a value, got {:?}",
        entry_block.terminator
    );
}
