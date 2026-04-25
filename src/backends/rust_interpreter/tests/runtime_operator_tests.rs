//! Runtime operator tests for the Rust interpreter.
//!
//! WHAT: verifies panic-safe integer operator behavior.
//! WHY: arithmetic over user-controlled values must return structured runtime errors.

use super::test_support::{build_simple_exec_program, exec_block, exec_const_int, user_local};
use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{
    ExecBinaryOperator, ExecInstruction, ExecLocalId, ExecStorageType, ExecTerminator,
    ExecUnaryOperator,
};
use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy;
use crate::backends::rust_interpreter::runtime::RuntimeEngine;

#[test]
fn integer_add_overflow_returns_runtime_error() {
    let result = execute_binary_int(i64::MAX, ExecBinaryOperator::Add, 1);
    assert_integer_overflow(result);
}

#[test]
fn integer_subtract_overflow_returns_runtime_error() {
    let result = execute_binary_int(i64::MIN, ExecBinaryOperator::Subtract, 1);
    assert_integer_overflow(result);
}

#[test]
fn integer_multiply_overflow_returns_runtime_error() {
    let result = execute_binary_int(i64::MAX, ExecBinaryOperator::Multiply, 2);
    assert_integer_overflow(result);
}

#[test]
fn integer_negate_overflow_returns_runtime_error() {
    let result = execute_unary_int(ExecUnaryOperator::Negate, i64::MIN);
    assert_integer_overflow(result);
}

#[test]
fn integer_divide_min_by_negative_one_returns_runtime_error() {
    let result = execute_binary_int(i64::MIN, ExecBinaryOperator::IntDivide, -1);
    assert_integer_overflow(result);
}

#[test]
fn integer_modulo_min_by_negative_one_returns_runtime_error() {
    let result = execute_binary_int(i64::MIN, ExecBinaryOperator::Modulo, -1);
    assert_integer_overflow(result);
}

fn execute_binary_int(
    left: i64,
    operator: ExecBinaryOperator,
    right: i64,
) -> Result<crate::backends::rust_interpreter::value::Value, InterpreterBackendError> {
    let left_local = ExecLocalId(0);
    let right_local = ExecLocalId(1);
    let result_local = ExecLocalId(2);

    let program = build_simple_exec_program(
        vec![exec_block(
            0,
            vec![
                ExecInstruction::LoadConst {
                    target: left_local,
                    const_id: crate::backends::rust_interpreter::exec_ir::ExecConstId(0),
                },
                ExecInstruction::LoadConst {
                    target: right_local,
                    const_id: crate::backends::rust_interpreter::exec_ir::ExecConstId(1),
                },
                ExecInstruction::BinaryOp {
                    left: left_local,
                    operator,
                    right: right_local,
                    destination: result_local,
                },
            ],
            ExecTerminator::Return {
                value: Some(result_local),
            },
        )],
        vec![
            user_local(0, ExecStorageType::Int),
            user_local(1, ExecStorageType::Int),
            user_local(2, ExecStorageType::Int),
        ],
        vec![exec_const_int(0, left), exec_const_int(1, right)],
        vec![],
    );

    RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless).execute_start()
}

fn execute_unary_int(
    operator: ExecUnaryOperator,
    value: i64,
) -> Result<crate::backends::rust_interpreter::value::Value, InterpreterBackendError> {
    let operand_local = ExecLocalId(0);
    let result_local = ExecLocalId(1);

    let program = build_simple_exec_program(
        vec![exec_block(
            0,
            vec![
                ExecInstruction::LoadConst {
                    target: operand_local,
                    const_id: crate::backends::rust_interpreter::exec_ir::ExecConstId(0),
                },
                ExecInstruction::UnaryOp {
                    operand: operand_local,
                    operator,
                    destination: result_local,
                },
            ],
            ExecTerminator::Return {
                value: Some(result_local),
            },
        )],
        vec![
            user_local(0, ExecStorageType::Int),
            user_local(1, ExecStorageType::Int),
        ],
        vec![exec_const_int(0, value)],
        vec![],
    );

    RuntimeEngine::new(program, InterpreterExecutionPolicy::NormalHeadless).execute_start()
}

fn assert_integer_overflow(
    result: Result<crate::backends::rust_interpreter::value::Value, InterpreterBackendError>,
) {
    match result.expect_err("operation should return a runtime error") {
        InterpreterBackendError::Execution { message } => {
            assert!(
                message.contains("Integer overflow"),
                "error should mention integer overflow, got: {message}"
            );
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}
