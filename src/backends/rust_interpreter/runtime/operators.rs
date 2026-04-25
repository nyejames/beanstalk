//! Runtime operator execution for the Rust interpreter.
//!
//! WHAT: evaluates Exec IR unary and binary operators over already-read runtime values.
//! WHY: the engine should stay focused on frames, blocks, and instruction dispatch while
//! operator semantics stay isolated and panic-safe.

use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{ExecBinaryOperator, ExecUnaryOperator};
use crate::backends::rust_interpreter::value::Value;

pub(crate) fn execute_binary_operator(
    left_value: Value,
    operator: ExecBinaryOperator,
    right_value: Value,
) -> Result<Value, InterpreterBackendError> {
    match operator {
        ExecBinaryOperator::Add => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => {
                checked_int_binary(left, right, "Add", |left, right| left.checked_add(right))
            }
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left + right)),
            (left, right) => type_mismatch("Add", "expected Int or Float operands", left, right),
        },

        ExecBinaryOperator::Subtract => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => {
                checked_int_binary(left, right, "Subtract", |left, right| {
                    left.checked_sub(right)
                })
            }
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left - right)),
            (left, right) => {
                type_mismatch("Subtract", "expected Int or Float operands", left, right)
            }
        },

        ExecBinaryOperator::Multiply => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => {
                checked_int_binary(left, right, "Multiply", |left, right| {
                    left.checked_mul(right)
                })
            }
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left * right)),
            (left, right) => {
                type_mismatch("Multiply", "expected Int or Float operands", left, right)
            }
        },

        ExecBinaryOperator::Divide => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Float(left as f64 / right as f64)),
            (Value::Int(left), Value::Float(right)) => Ok(Value::Float(left as f64 / right)),
            (Value::Float(left), Value::Int(right)) => Ok(Value::Float(left / right as f64)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Float(left / right)),
            (left, right) => type_mismatch("Divide", "expected Int or Float operands", left, right),
        },

        ExecBinaryOperator::IntDivide => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => checked_int_divide(left, right),
            (left, right) => type_mismatch("IntDivide", "expected Int operands", left, right),
        },

        ExecBinaryOperator::Modulo => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => checked_int_modulo(left, right),
            (Value::Float(left), Value::Float(right)) => {
                if right == 0.0 {
                    return execution_error("Modulo by zero");
                }
                Ok(Value::Float(left.rem_euclid(right)))
            }
            (left, right) => type_mismatch("Modulo", "expected Int or Float operands", left, right),
        },

        ExecBinaryOperator::Equal => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left == right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left == right)),
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left == right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left == right)),
            (left, right) => type_mismatch(
                "Equal",
                "operands must have the same supported scalar type",
                left,
                right,
            ),
        },

        ExecBinaryOperator::NotEqual => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left != right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left != right)),
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left != right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left != right)),
            (left, right) => type_mismatch(
                "NotEqual",
                "operands must have the same supported scalar type",
                left,
                right,
            ),
        },

        ExecBinaryOperator::LessThan => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left < right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left < right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left < right)),
            (left, right) => type_mismatch(
                "LessThan",
                "expected Int, Float, or Char operands",
                left,
                right,
            ),
        },

        ExecBinaryOperator::LessThanOrEqual => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left <= right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left <= right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left <= right)),
            (left, right) => type_mismatch(
                "LessThanOrEqual",
                "expected Int, Float, or Char operands",
                left,
                right,
            ),
        },

        ExecBinaryOperator::GreaterThan => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left > right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left > right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left > right)),
            (left, right) => type_mismatch(
                "GreaterThan",
                "expected Int, Float, or Char operands",
                left,
                right,
            ),
        },

        ExecBinaryOperator::GreaterThanOrEqual => match (left_value, right_value) {
            (Value::Int(left), Value::Int(right)) => Ok(Value::Bool(left >= right)),
            (Value::Float(left), Value::Float(right)) => Ok(Value::Bool(left >= right)),
            (Value::Char(left), Value::Char(right)) => Ok(Value::Bool(left >= right)),
            (left, right) => type_mismatch(
                "GreaterThanOrEqual",
                "expected Int, Float, or Char operands",
                left,
                right,
            ),
        },

        ExecBinaryOperator::And => match (left_value, right_value) {
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left && right)),
            (left, right) => type_mismatch("And", "expected Bool operands", left, right),
        },

        ExecBinaryOperator::Or => match (left_value, right_value) {
            (Value::Bool(left), Value::Bool(right)) => Ok(Value::Bool(left || right)),
            (left, right) => type_mismatch("Or", "expected Bool operands", left, right),
        },
    }
}

pub(crate) fn execute_unary_operator(
    operator: ExecUnaryOperator,
    operand_value: Value,
) -> Result<Value, InterpreterBackendError> {
    match operator {
        ExecUnaryOperator::Negate => match operand_value {
            Value::Int(value) => value
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| arithmetic_overflow("Negate")),
            Value::Float(value) => Ok(Value::Float(-value)),
            other => execution_error(format!(
                "Type mismatch in Negate operation: expected Int or Float operand, found {other:?}",
            )),
        },

        ExecUnaryOperator::Not => match operand_value {
            Value::Bool(value) => Ok(Value::Bool(!value)),
            other => execution_error(format!(
                "Type mismatch in Not operation: expected Bool operand, found {other:?}",
            )),
        },
    }
}

fn checked_int_binary(
    left: i64,
    right: i64,
    operation_name: &'static str,
    operation: impl FnOnce(i64, i64) -> Option<i64>,
) -> Result<Value, InterpreterBackendError> {
    operation(left, right)
        .map(Value::Int)
        .ok_or_else(|| arithmetic_overflow(operation_name))
}

fn checked_int_divide(left: i64, right: i64) -> Result<Value, InterpreterBackendError> {
    if right == 0 {
        return execution_error("Division by zero");
    }

    left.checked_div(right)
        .map(Value::Int)
        .ok_or_else(|| arithmetic_overflow("IntDivide"))
}

fn checked_int_modulo(left: i64, right: i64) -> Result<Value, InterpreterBackendError> {
    if right == 0 {
        return execution_error("Modulo by zero");
    }

    left.checked_rem_euclid(right)
        .map(Value::Int)
        .ok_or_else(|| arithmetic_overflow("Modulo"))
}

fn type_mismatch(
    operation_name: &'static str,
    expected: &'static str,
    left: Value,
    right: Value,
) -> Result<Value, InterpreterBackendError> {
    execution_error(format!(
        "Type mismatch in {operation_name} operation: {expected}, found {left:?} and {right:?}",
    ))
}

fn arithmetic_overflow(operation_name: &'static str) -> InterpreterBackendError {
    InterpreterBackendError::Execution {
        message: format!("Integer overflow in {operation_name} operation"),
    }
}

fn execution_error<T>(message: impl Into<String>) -> Result<T, InterpreterBackendError> {
    Err(InterpreterBackendError::Execution {
        message: message.into(),
    })
}
