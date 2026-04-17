//! Debug output contract tests for the Rust interpreter backend.
//!
//! WHAT: locks down the textual debug contract for lowering plan, function layouts,
//!       and final value output.
//! WHY: accidental always-on debug output would pollute compiler output; accidental
//!      silencing would break diagnostics.  These tests prevent both.

use super::test_support::{build_module, build_type_context, int_expression, unit_expression};
use crate::backends::rust_interpreter::debug::build_debug_outputs;
use crate::backends::rust_interpreter::request::{
    InterpreterBackendRequest, InterpreterDebugFlags,
};
use crate::backends::rust_interpreter::result::InterpreterExecutionResult;
use crate::backends::rust_interpreter::value::Value;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBlock, HirFunction, HirFunctionOrigin, HirTerminator, RegionId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

// ============================================================
// Test 1: lowering plan text reports expected counts
// ============================================================

#[test]
fn lowering_plan_text_reports_counts() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    // One function, one block, one instruction (from the int constant), one constant
    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(1, 7, types.int, region)),
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

    let exec_program = super::test_support::lower_only(&module);

    let request = InterpreterBackendRequest {
        debug_flags: InterpreterDebugFlags {
            show_lowering_plan: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let outputs = build_debug_outputs(&request, &exec_program, None);

    let plan_text = outputs
        .plan_text
        .expect("plan_text should be present when flag is set");
    assert!(
        plan_text.contains("1 function"),
        "plan text should report 1 function: {plan_text}"
    );
    assert!(
        plan_text.contains("1 block"),
        "plan text should report 1 block: {plan_text}"
    );
    assert!(
        plan_text.contains("1 constant"),
        "plan text should report 1 constant: {plan_text}"
    );
}

// ============================================================
// Test 2: function layouts text lists each function
// ============================================================

#[test]
fn function_layouts_text_lists_each_function() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block_a = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(1, types.unit, region)),
    };
    let block_b = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, region)),
    };

    let function_a = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };
    let function_b = HirFunction {
        id: FunctionId(1),
        entry: BlockId(1),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path_a = InternedPath::from_single_str("fn_a", &mut string_table);
    let path_b = InternedPath::from_single_str("fn_b", &mut string_table);

    let module = build_module(
        &mut string_table,
        vec![
            (function_a, path_a, HirFunctionOrigin::EntryStart),
            (function_b, path_b, HirFunctionOrigin::Normal),
        ],
        vec![block_a, block_b],
        type_context,
        FunctionId(0),
    );

    let exec_program = super::test_support::lower_only(&module);

    let request = InterpreterBackendRequest {
        debug_flags: InterpreterDebugFlags {
            show_function_layouts: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let outputs = build_debug_outputs(&request, &exec_program, None);

    let layouts_text = outputs
        .function_layouts_text
        .expect("function_layouts_text should be present when flag is set");

    assert_eq!(
        exec_program.module.functions.len(),
        2,
        "should have lowered 2 functions"
    );
    assert!(
        layouts_text.contains("fn_0"),
        "layouts text should contain fn_0 debug name: {layouts_text}"
    );
    assert!(
        layouts_text.contains("fn_1"),
        "layouts text should contain fn_1 debug name: {layouts_text}"
    );
}

// ============================================================
// Test 3: final value text reports scalar return value
// ============================================================

#[test]
fn final_value_text_reports_returned_scalar_value() {
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

    let exec_program = super::test_support::lower_only(&module);

    let execution_result = InterpreterExecutionResult {
        returned_value: Value::Int(42),
    };

    let request = InterpreterBackendRequest {
        debug_flags: InterpreterDebugFlags {
            show_final_value: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let outputs = build_debug_outputs(&request, &exec_program, Some(&execution_result));

    let final_text = outputs
        .final_value_text
        .expect("final_value_text should be present when flag is set");

    assert!(
        final_text.contains("Int"),
        "final value text should contain \"Int\": {final_text}"
    );
    assert!(
        final_text.contains("42"),
        "final value text should contain \"42\": {final_text}"
    );
}

// ============================================================
// Test 4: all debug flags off produces no output
// ============================================================

#[test]
fn debug_outputs_respect_flags() {
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

    let exec_program = super::test_support::lower_only(&module);

    // All flags default to false
    let request = InterpreterBackendRequest::default();
    let outputs = build_debug_outputs(&request, &exec_program, None);

    assert!(
        outputs.plan_text.is_none(),
        "plan_text should be None when flag is off"
    );
    assert!(
        outputs.exec_ir_text.is_none(),
        "exec_ir_text should be None when flag is off"
    );
    assert!(
        outputs.function_layouts_text.is_none(),
        "function_layouts_text should be None when flag is off"
    );
    assert!(
        outputs.execution_trace_text.is_none(),
        "execution_trace_text should be None when flag is off"
    );
    assert!(
        outputs.final_value_text.is_none(),
        "final_value_text should be None when flag is off"
    );
}
