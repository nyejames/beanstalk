//! Failure-path tests for the Rust interpreter backend.
//!
//! WHAT: verifies that unsupported or invalid constructs produce clear errors rather than
//!       panics, silent corruption, or misleading success.
//! WHY: the backend must reject unsupported cases with diagnostic messages so the compiler
//!      can surface them clearly and avoid fragile undefined behaviour.

use super::test_support::{
    build_module, build_simple_exec_program, build_type_context, exec_const_int, exec_const_string,
    expression, int_expression, lower_only_expect_error, statement, unit_expression, user_local,
};
use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecConstId, ExecFunction, ExecFunctionFlags, ExecFunctionId,
    ExecInstruction, ExecLocal, ExecLocalId, ExecLocalRole, ExecModule, ExecModuleId, ExecProgram,
    ExecStorageType, ExecTerminator,
};
use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy;
use crate::backends::rust_interpreter::runtime::RuntimeEngine;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::functions::{HirFunction, HirFunctionOrigin};
use crate::compiler_frontend::hir::ids::{BlockId, FieldId, FunctionId, LocalId, RegionId};
use crate::compiler_frontend::hir::operators::HirBinOp;
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

// ============================================================
// Lowering failure: Call statements
// ============================================================

#[test]
fn lowering_rejects_call_statements() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![statement(
            1,
            HirStatementKind::Call {
                target: CallTarget::UserFunction(FunctionId(0)),
                args: vec![],
                result: None,
            },
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

    let error = lower_only_expect_error(&module);
    assert!(
        error.msg.contains("does not support call statements yet"),
        "unexpected error message: {}",
        error.msg
    );
}

// ============================================================
// Lowering failure: non-local assignment target
// ============================================================

#[test]
fn lowering_rejects_non_local_assignment_targets() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![statement(
            1,
            HirStatementKind::Assign {
                target: HirPlace::Field {
                    base: Box::new(HirPlace::Local(LocalId(0))),
                    field: FieldId(0),
                },
                value: int_expression(100, 1, types.int, region),
            },
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

    let error = lower_only_expect_error(&module);
    assert!(
        error.msg.contains("non-local assignment target"),
        "unexpected error message: {}",
        error.msg
    );
}

// ============================================================
// Lowering failure: non-local load place
// ============================================================

#[test]
fn lowering_rejects_non_local_load_places() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let non_local_load = super::test_support::expression(
        1,
        HirExpressionKind::Load(HirPlace::Field {
            base: Box::new(HirPlace::Local(LocalId(0))),
            field: FieldId(0),
        }),
        types.int,
        region,
        crate::compiler_frontend::hir::expressions::ValueKind::Place,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(non_local_load),
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

    let error = lower_only_expect_error(&module);
    assert!(
        error.msg.contains("non-local load places"),
        "unexpected error message: {}",
        error.msg
    );
}

// ============================================================
// Lowering failure: non-local copy place
// ============================================================

#[test]
fn lowering_rejects_non_local_copy_places() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let non_local_copy = super::test_support::expression(
        1,
        HirExpressionKind::Copy(HirPlace::Field {
            base: Box::new(HirPlace::Local(LocalId(0))),
            field: FieldId(0),
        }),
        types.int,
        region,
        crate::compiler_frontend::hir::expressions::ValueKind::RValue,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(non_local_copy),
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

    let error = lower_only_expect_error(&module);
    assert!(
        error.msg.contains("non-local copy places"),
        "unexpected error message: {}",
        error.msg
    );
}

// ============================================================
// Lowering: Match terminator lowers to PendingLowering,
//           execution then fails cleanly (no panic)
// ============================================================

#[test]
fn lowering_rejects_match_terminator_execution_path() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let catch_all_block = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(10, types.unit, region)),
    };

    let entry_block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Match {
            scrutinee: unit_expression(1, types.unit, region),
            arms: vec![HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: BlockId(1),
            }],
        },
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
        vec![entry_block, catch_all_block],
        type_context,
        FunctionId(0),
    );

    // Lowering should succeed (produces PendingLowering terminator)
    let exec_program = super::test_support::lower_only(&module);
    let function = &exec_program.module.functions[0];
    let entry = &function.blocks[0];
    assert!(
        matches!(
            &entry.terminator,
            crate::backends::rust_interpreter::exec_ir::ExecTerminator::PendingLowering { .. }
        ),
        "Match should lower to PendingLowering"
    );

    // Execution should fail cleanly (not panic)
    let mut runtime = RuntimeEngine::new(exec_program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "executing PendingLowering should return an error"
    );
    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("pending-lowering"),
                "error should mention pending-lowering, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Lowering: Panic terminator lowers to PendingLowering,
//           execution then fails cleanly (no panic)
// ============================================================

#[test]
fn lowering_rejects_panic_terminator_execution_path() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Panic { message: None },
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
    let entry = &exec_program.module.functions[0].blocks[0];
    assert!(
        matches!(
            &entry.terminator,
            crate::backends::rust_interpreter::exec_ir::ExecTerminator::PendingLowering { .. }
        ),
        "Panic should lower to PendingLowering"
    );

    let mut runtime = RuntimeEngine::new(exec_program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "executing PendingLowering should return an error"
    );
    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("pending-lowering"),
                "error should mention pending-lowering, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Runtime failure: no entry function
// ============================================================

#[test]
fn runtime_rejects_missing_entry_function() {
    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![ExecFunction {
                id: ExecFunctionId(0),
                debug_name: "orphan_fn".to_owned(),
                entry_block: ExecBlockId(0),
                parameter_slots: vec![],
                locals: vec![],
                blocks: vec![ExecBlock {
                    id: ExecBlockId(0),
                    instructions: vec![],
                    terminator: ExecTerminator::Return { value: None },
                }],
                result_type: ExecStorageType::Unit,
                flags: ExecFunctionFlags::default(),
            }],
            constants: vec![],
            entry_function: None, // deliberately missing
        },
    };

    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "missing entry function should produce an error"
    );

    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("no entry function"),
                "error should mention missing entry function, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Runtime failure: non-bool BranchBool condition
// ============================================================

#[test]
fn runtime_rejects_non_bool_branch_condition() {
    // const_id=0: Int(42), local_id=0 holds that int, then BranchBool on it
    let const_id = ExecConstId(0);
    let cond_local = ExecLocalId(0);

    let program = build_simple_exec_program(
        vec![
            ExecBlock {
                id: ExecBlockId(0),
                instructions: vec![ExecInstruction::LoadConst {
                    target: cond_local,
                    const_id,
                }],
                terminator: ExecTerminator::BranchBool {
                    condition: cond_local,
                    then_block: ExecBlockId(1),
                    else_block: ExecBlockId(2),
                },
            },
            ExecBlock {
                id: ExecBlockId(1),
                instructions: vec![],
                terminator: ExecTerminator::Return { value: None },
            },
            ExecBlock {
                id: ExecBlockId(2),
                instructions: vec![],
                terminator: ExecTerminator::Return { value: None },
            },
        ],
        vec![user_local(0, ExecStorageType::Int)],
        vec![exec_const_int(0, 42)],
        vec![],
    );

    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "non-bool branch condition should produce an error"
    );

    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("expected bool branch condition"),
                "error should mention expected bool condition, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Runtime failure: PendingLowering terminator
// ============================================================

#[test]
fn runtime_rejects_pending_lowering_terminator() {
    let program = build_simple_exec_program(
        vec![ExecBlock {
            id: ExecBlockId(0),
            instructions: vec![],
            terminator: ExecTerminator::PendingLowering {
                description: "test pending lowering path".to_owned(),
            },
        }],
        vec![],
        vec![],
        vec![],
    );

    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "PendingLowering terminator should produce an error"
    );

    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("pending-lowering"),
                "error should mention pending-lowering, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Runtime failure: copy of heap-backed value
// ============================================================

#[test]
fn runtime_rejects_copy_of_heap_backed_value() {
    // block0: LoadConst string -> local0 (becomes Handle), then CopyLocal local0 -> local1
    let string_const = ExecConstId(0);
    let handle_local = ExecLocalId(0);
    let copy_target = ExecLocalId(1);

    let program = build_simple_exec_program(
        vec![ExecBlock {
            id: ExecBlockId(0),
            instructions: vec![
                ExecInstruction::LoadConst {
                    target: handle_local,
                    const_id: string_const,
                },
                ExecInstruction::CopyLocal {
                    target: copy_target,
                    source: handle_local,
                },
            ],
            terminator: ExecTerminator::Return { value: None },
        }],
        vec![
            user_local(0, ExecStorageType::HeapHandle),
            user_local(1, ExecStorageType::HeapHandle),
        ],
        vec![exec_const_string(0, "heap_copy_test")],
        vec![],
    );

    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "copying a heap-backed value should produce an error"
    );

    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("heap-backed"),
                "error should mention heap-backed copy, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

#[test]
fn runtime_rejects_integer_division_by_zero() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let divide_expr = expression(
        1,
        HirExpressionKind::BinOp {
            left: Box::new(int_expression(2, 5, types.int, region)),
            op: HirBinOp::IntDiv,
            right: Box::new(int_expression(3, 0, types.int, region)),
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

    let exec_program = super::test_support::lower_only(&module);
    let mut runtime = RuntimeEngine::new(exec_program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "integer division by zero should produce an execution error"
    );
    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("Division by zero"),
                "error should mention division by zero, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}

// ============================================================
// Runtime failure: parameterized function execution
// ============================================================

#[test]
fn runtime_rejects_parameterized_function_execution() {
    let param_local = ExecLocalId(0);
    let temp_local = ExecLocalId(1);

    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![ExecFunction {
                id: ExecFunctionId(0),
                debug_name: "parameterized_fn".to_owned(),
                entry_block: ExecBlockId(0),
                parameter_slots: vec![param_local], // non-empty = has parameters
                locals: vec![
                    ExecLocal {
                        id: param_local,
                        debug_name: Some("param_0".to_owned()),
                        storage_type: ExecStorageType::Int,
                        role: ExecLocalRole::Param,
                    },
                    ExecLocal {
                        id: temp_local,
                        debug_name: Some("temp_1".to_owned()),
                        storage_type: ExecStorageType::Unknown,
                        role: ExecLocalRole::Temp,
                    },
                ],
                blocks: vec![ExecBlock {
                    id: ExecBlockId(0),
                    instructions: vec![],
                    terminator: ExecTerminator::Return { value: None },
                }],
                result_type: ExecStorageType::Unit,
                flags: ExecFunctionFlags {
                    is_start: true,
                    is_ctfe_allowed: false,
                },
            }],
            constants: vec![],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let mut runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);
    let result = runtime.execute_start();
    assert!(
        result.is_err(),
        "parameterized function execution should produce an error"
    );

    match result.unwrap_err() {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("parameters"),
                "error should mention parameters, got: {message}"
            );
        }
        other => panic!("unexpected error type: {:?}", other),
    }
}
