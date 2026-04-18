//! End-to-end runtime execution tests for the Rust interpreter backend.
//!
//! WHAT: verifies that supported HIR constructs execute correctly through the full
//!       backend pipeline (HIR -> Exec IR -> RuntimeEngine).
//! WHY: successful execution behavior should be pinned independently from lowering structure
//!      so that regressions in either pass are immediately visible.

use super::test_support::{
    assert_value_is_bool, assert_value_is_char, assert_value_is_float, assert_value_is_int,
    assert_value_is_unit, bool_expression, build_manual_exec_program_returning_string,
    build_module, build_type_context, copy_local_expression, expression, int_expression,
    load_local_expression, local, lower_and_execute_start, lower_and_execute_start_with_runtime,
    statement, unit_expression,
};
use crate::backends::rust_interpreter::heap::HeapObject;
use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy;
use crate::backends::rust_interpreter::runtime::RuntimeEngine;
use crate::backends::rust_interpreter::value::Value;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBinOp, HirBlock, HirExpressionKind, HirFunction, HirFunctionOrigin,
    HirPlace, HirStatementKind, HirTerminator, LocalId, RegionId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

// ============================================================
// Test 1: return 42
// ============================================================

#[test]
fn executes_return_int_from_start() {
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

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 42);
}

// ============================================================
// Test 2: return true
// ============================================================

#[test]
fn executes_return_bool_from_start() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(bool_expression(1, true, types.boolean, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.boolean,
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

    let value = lower_and_execute_start(&module);
    assert_value_is_bool(&value, true);
}

// ============================================================
// Test 3: return 'x'
// ============================================================

#[test]
fn executes_return_char_from_start() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(super::test_support::char_expression(
            1,
            'x',
            types.char_type,
            region,
        )),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.char_type,
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

    let value = lower_and_execute_start(&module);
    assert_value_is_char(&value, 'x');
}

// ============================================================
// Test 4: return 1.5
// ============================================================

#[test]
fn executes_return_float_from_start() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(super::test_support::float_expression(
            1,
            1.5,
            types.float,
            region,
        )),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.float,
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

    let value = lower_and_execute_start(&module);
    assert_value_is_float(&value, 1.5);
}

// ============================================================
// Test 5: return ()
// ============================================================

#[test]
fn executes_return_unit_from_start() {
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

    let value = lower_and_execute_start(&module);
    assert_value_is_unit(&value);
}

#[test]
fn executes_int_division_as_real_division_returning_float() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let divide_expr = expression(
        1,
        HirExpressionKind::BinOp {
            left: Box::new(int_expression(2, 5, types.int, region)),
            op: HirBinOp::Div,
            right: Box::new(int_expression(3, 2, types.int, region)),
        },
        types.float,
        region,
        ValueKind::RValue,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(divide_expr),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.float,
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

    let value = lower_and_execute_start(&module);
    assert_value_is_float(&value, 2.5);
}

#[test]
fn executes_real_division_by_zero_without_trapping() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let divide_expr = expression(
        1,
        HirExpressionKind::BinOp {
            left: Box::new(int_expression(2, 5, types.int, region)),
            op: HirBinOp::Div,
            right: Box::new(int_expression(3, 0, types.int, region)),
        },
        types.float,
        region,
        ValueKind::RValue,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(divide_expr),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.float,
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

    let value = lower_and_execute_start(&module);
    match value {
        Value::Float(v) => assert!(v.is_infinite() && v.is_sign_positive()),
        other => panic!("expected positive infinity float result, got {other:?}"),
    }
}

#[test]
fn executes_integer_division_operator_with_truncation_toward_zero() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let divide_expr = expression(
        1,
        HirExpressionKind::BinOp {
            left: Box::new(int_expression(2, -5, types.int, region)),
            op: HirBinOp::IntDiv,
            right: Box::new(int_expression(3, 2, types.int, region)),
        },
        types.int,
        region,
        ValueKind::RValue,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(divide_expr),
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

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, -2);
}

// ============================================================
// Test 6: x = 42; return x
// ============================================================

#[test]
fn executes_assign_then_return_local() {
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

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 42);
}

// ============================================================
// Test 7: if true then return 1 else return 0
// ============================================================

#[test]
fn executes_if_true_branch() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let entry = HirBlock {
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
        vec![entry, then_block, else_block],
        type_context,
        FunctionId(0),
    );

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 1);
}

// ============================================================
// Test 8: if false then return 1 else return 0
// ============================================================

#[test]
fn executes_if_false_branch() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let entry = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::If {
            condition: bool_expression(1, false, types.boolean, region),
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
        vec![entry, then_block, else_block],
        type_context,
        FunctionId(0),
    );

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 0);
}

// ============================================================
// Test 9: block0 -> block1 -> block2 -> return 99
// ============================================================

#[test]
fn executes_jump_chain() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block0 = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Jump {
            target: BlockId(1),
            args: vec![],
        },
    };
    let block1 = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Jump {
            target: BlockId(2),
            args: vec![],
        },
    };
    let block2 = HirBlock {
        id: BlockId(2),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(1, 99, types.int, region)),
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
        vec![block0, block1, block2],
        type_context,
        FunctionId(0),
    );

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 99);
}

// ============================================================
// Test 10: return "hello" materializes a heap string object
// ============================================================

#[test]
fn executes_string_return_and_materializes_heap_object() {
    let program = build_manual_exec_program_returning_string("hello");
    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    let returned_value = runtime.execute_start().expect("execution should succeed");

    let handle = match returned_value {
        Value::Handle(handle) => handle,
        other => panic!("expected heap handle, got {:?}", other),
    };

    let object = runtime
        .heap
        .get(handle)
        .expect("string handle should resolve");
    match object {
        HeapObject::String(string_object) => {
            assert_eq!(string_object.text, "hello", "heap string text should match")
        }
        other => panic!("expected HeapObject::String, got {:?}", other),
    }
}

// ============================================================
// Test 11: copy local for scalar values
// ============================================================

#[test]
fn executes_copy_local_for_scalar_values() {
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
                    value: int_expression(100, 55, types.int, region),
                },
                1,
            ),
            statement(
                2,
                HirStatementKind::Assign {
                    target: HirPlace::Local(local_y),
                    value: copy_local_expression(101, local_x, types.int, region),
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

    let value = lower_and_execute_start(&module);
    assert_value_is_int(&value, 55);
}

// ============================================================
// Test 12: string return via HIR lowering also materializes heap object
// ============================================================

#[test]
fn executes_string_return_via_hir_lowering_materializes_heap_object() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(super::test_support::string_expression(
            1,
            "world",
            types.string,
            region,
        )),
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

    let (value, runtime) = lower_and_execute_start_with_runtime(&module);

    let handle = match value {
        Value::Handle(h) => h,
        other => panic!("expected Value::Handle, got {:?}", other),
    };

    match runtime.heap.get(handle).expect("handle should resolve") {
        HeapObject::String(s) => assert_eq!(s.text, "world"),
        other => panic!("expected HeapObject::String, got {:?}", other),
    }
}
