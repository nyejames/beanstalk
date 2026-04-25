//! Runtime index/lookup tests for the Rust interpreter backend.
//!
//! WHAT: verifies that the RuntimeEngine builds correct lookup tables for functions, blocks,
//!       and constants, and that scoped lookups do not confuse same ids across functions.
//! WHY: the lookup tables are the critical indirection layer between Exec IR ids and vec
//!      indices; any mapping bug would cause silent data corruption rather than a clear error.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecConst, ExecConstId, ExecConstValue, ExecFunction,
    ExecFunctionFlags, ExecFunctionId, ExecInstruction, ExecLocal, ExecLocalId, ExecLocalRole,
    ExecModule, ExecModuleId, ExecProgram, ExecStorageType, ExecTerminator,
};
use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy;
use crate::backends::rust_interpreter::runtime::RuntimeEngine;

// ============================================================
// Test 1: resolve function by id
// ============================================================

#[test]
fn runtime_resolves_function_by_id() {
    let function_a = make_function(
        ExecFunctionId(0),
        "fn_zero",
        vec![],
        vec![make_return_block(ExecBlockId(0))],
    );
    let function_b = make_function(
        ExecFunctionId(1),
        "fn_one",
        vec![],
        vec![make_return_block(ExecBlockId(0))],
    );

    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![function_a, function_b],
            constants: vec![],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    let fn_a = runtime
        .function_by_id(ExecFunctionId(0))
        .expect("function 0 should resolve");
    assert_eq!(fn_a.debug_name, "fn_zero");

    let fn_b = runtime
        .function_by_id(ExecFunctionId(1))
        .expect("function 1 should resolve");
    assert_eq!(fn_b.debug_name, "fn_one");
}

// ============================================================
// Test 2: resolve block by function and block id
// ============================================================

#[test]
fn runtime_resolves_block_by_function_and_block_id() {
    let block_a = ExecBlock {
        id: ExecBlockId(0),
        instructions: vec![],
        terminator: ExecTerminator::Return { value: None },
    };
    let block_b = ExecBlock {
        id: ExecBlockId(1),
        instructions: vec![ExecInstruction::ReadLocal {
            target: ExecLocalId(0),
            source: ExecLocalId(0),
        }],
        terminator: ExecTerminator::Jump {
            target: ExecBlockId(0),
        },
    };

    let function = make_function(
        ExecFunctionId(0),
        "test_fn",
        vec![make_temp_local(ExecLocalId(0))],
        vec![block_a, block_b],
    );

    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![function],
            constants: vec![],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    let block_0 = runtime
        .block_by_ids(ExecFunctionId(0), ExecBlockId(0))
        .expect("block 0 should resolve");
    assert_eq!(
        block_0.instructions.len(),
        0,
        "block 0 should have no instructions"
    );

    let block_1 = runtime
        .block_by_ids(ExecFunctionId(0), ExecBlockId(1))
        .expect("block 1 should resolve");
    assert_eq!(
        block_1.instructions.len(),
        1,
        "block 1 should have one instruction"
    );
}

// ============================================================
// Test 3: resolve const by id
// ============================================================

#[test]
fn runtime_resolves_const_by_id() {
    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![make_function(
                ExecFunctionId(0),
                "test_fn",
                vec![],
                vec![make_return_block(ExecBlockId(0))],
            )],
            constants: vec![
                ExecConst {
                    id: ExecConstId(0),
                    value: ExecConstValue::Int(10),
                },
                ExecConst {
                    id: ExecConstId(1),
                    value: ExecConstValue::Bool(true),
                },
                ExecConst {
                    id: ExecConstId(2),
                    value: ExecConstValue::String("lookup_test".to_owned()),
                },
            ],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    let const_0 = runtime
        .const_value_by_id(ExecConstId(0))
        .expect("const 0 should resolve");
    assert!(matches!(const_0, ExecConstValue::Int(10)));

    let const_1 = runtime
        .const_value_by_id(ExecConstId(1))
        .expect("const 1 should resolve");
    assert!(matches!(const_1, ExecConstValue::Bool(true)));

    let const_2 = runtime
        .const_value_by_id(ExecConstId(2))
        .expect("const 2 should resolve");
    assert!(matches!(const_2, ExecConstValue::String(s) if s == "lookup_test"));
}

// ============================================================
// Test 4: lookup tables are consistent across functions/blocks/consts
// ============================================================

#[test]
fn runtime_builds_lookup_indexes_consistently() {
    let function_0 = make_function(
        ExecFunctionId(0),
        "fn_0",
        vec![],
        vec![
            make_return_block(ExecBlockId(0)),
            make_jump_block(ExecBlockId(1), ExecBlockId(0)),
        ],
    );
    let function_1 = make_function(
        ExecFunctionId(1),
        "fn_1",
        vec![],
        vec![make_return_block(ExecBlockId(0))],
    );

    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![function_0, function_1],
            constants: vec![
                ExecConst {
                    id: ExecConstId(0),
                    value: ExecConstValue::Int(1),
                },
                ExecConst {
                    id: ExecConstId(1),
                    value: ExecConstValue::Int(2),
                },
            ],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    // All functions resolve
    assert!(runtime.function_by_id(ExecFunctionId(0)).is_ok());
    assert!(runtime.function_by_id(ExecFunctionId(1)).is_ok());

    // fn_0 has two blocks; fn_1 has one block
    assert!(
        runtime
            .block_by_ids(ExecFunctionId(0), ExecBlockId(0))
            .is_ok()
    );
    assert!(
        runtime
            .block_by_ids(ExecFunctionId(0), ExecBlockId(1))
            .is_ok()
    );
    assert!(
        runtime
            .block_by_ids(ExecFunctionId(1), ExecBlockId(0))
            .is_ok()
    );

    // All constants resolve
    assert!(runtime.const_value_by_id(ExecConstId(0)).is_ok());
    assert!(runtime.const_value_by_id(ExecConstId(1)).is_ok());

    // Cross-function block lookups fail correctly
    assert!(
        runtime
            .block_by_ids(ExecFunctionId(1), ExecBlockId(1))
            .is_err(),
        "fn_1 does not have block 1 — lookup should fail"
    );

    // Unknown function id fails
    assert!(runtime.function_by_id(ExecFunctionId(99)).is_err());
}

// ============================================================
// Test 5: block lookup is scoped by function — same block id in two functions
// ============================================================

#[test]
fn block_lookup_is_scoped_by_function() {
    // Both functions have ExecBlockId(0), but with different instruction counts.
    let block_in_fn_a = ExecBlock {
        id: ExecBlockId(0),
        instructions: vec![],
        terminator: ExecTerminator::Return { value: None },
    };
    let block_in_fn_b = ExecBlock {
        id: ExecBlockId(0),
        instructions: vec![
            ExecInstruction::ReadLocal {
                target: ExecLocalId(0),
                source: ExecLocalId(0),
            },
            ExecInstruction::ReadLocal {
                target: ExecLocalId(0),
                source: ExecLocalId(0),
            },
        ],
        terminator: ExecTerminator::Return { value: None },
    };

    let function_a = make_function(
        ExecFunctionId(0),
        "fn_a",
        vec![make_temp_local(ExecLocalId(0))],
        vec![block_in_fn_a],
    );
    let function_b = make_function(
        ExecFunctionId(1),
        "fn_b",
        vec![make_temp_local(ExecLocalId(0))],
        vec![block_in_fn_b],
    );

    let program = ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![function_a, function_b],
            constants: vec![],
            entry_function: Some(ExecFunctionId(0)),
        },
    };

    let runtime = RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless);

    let block_from_a = runtime
        .block_by_ids(ExecFunctionId(0), ExecBlockId(0))
        .expect("fn_a block 0 should resolve");
    assert_eq!(
        block_from_a.instructions.len(),
        0,
        "fn_a block 0 should have 0 instructions"
    );

    let block_from_b = runtime
        .block_by_ids(ExecFunctionId(1), ExecBlockId(0))
        .expect("fn_b block 0 should resolve");
    assert_eq!(
        block_from_b.instructions.len(),
        2,
        "fn_b block 0 should have 2 instructions"
    );
}

// ============================================================
// Helpers
// ============================================================

fn make_function(
    id: ExecFunctionId,
    debug_name: &str,
    locals: Vec<ExecLocal>,
    blocks: Vec<ExecBlock>,
) -> ExecFunction {
    let entry_block = blocks.first().map(|b| b.id).unwrap_or(ExecBlockId(0));
    ExecFunction {
        id,
        debug_name: debug_name.to_owned(),
        entry_block,
        parameter_slots: vec![],
        locals,
        blocks,
        result_type: ExecStorageType::Unknown,
        flags: ExecFunctionFlags::default(),
    }
}

fn make_return_block(id: ExecBlockId) -> ExecBlock {
    ExecBlock {
        id,
        instructions: vec![],
        terminator: ExecTerminator::Return { value: None },
    }
}

fn make_jump_block(id: ExecBlockId, target: ExecBlockId) -> ExecBlock {
    ExecBlock {
        id,
        instructions: vec![],
        terminator: ExecTerminator::Jump { target },
    }
}

fn make_temp_local(id: ExecLocalId) -> ExecLocal {
    ExecLocal {
        id,
        debug_name: Some(format!("temp_{}", id.0)),
        storage_type: ExecStorageType::Unknown,
        role: ExecLocalRole::Temp,
    }
}
