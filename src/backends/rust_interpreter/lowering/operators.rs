//! HIR operator to Exec IR operator mapping.
//!
//! WHAT: provides mapping functions to convert HIR operators to Exec IR operators.
//! WHY: centralizes operator mapping logic to keep lowering code clean and maintainable.

use crate::backends::rust_interpreter::exec_ir::{ExecBinaryOperator, ExecUnaryOperator};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirBinOp, HirUnaryOp};

/// Maps a HIR binary operator to the corresponding Exec IR binary operator.
///
/// Returns an error if the HIR operator is not yet supported by the interpreter.
pub(crate) fn map_binary_operator(hir_op: HirBinOp) -> Result<ExecBinaryOperator, CompilerError> {
    match hir_op {
        HirBinOp::Add => Ok(ExecBinaryOperator::Add),
        HirBinOp::Sub => Ok(ExecBinaryOperator::Subtract),
        HirBinOp::Mul => Ok(ExecBinaryOperator::Multiply),
        HirBinOp::Div => Ok(ExecBinaryOperator::Divide),
        HirBinOp::IntDiv => Ok(ExecBinaryOperator::IntDivide),
        HirBinOp::Mod => Ok(ExecBinaryOperator::Modulo),
        HirBinOp::Eq => Ok(ExecBinaryOperator::Equal),
        HirBinOp::Ne => Ok(ExecBinaryOperator::NotEqual),
        HirBinOp::Lt => Ok(ExecBinaryOperator::LessThan),
        HirBinOp::Le => Ok(ExecBinaryOperator::LessThanOrEqual),
        HirBinOp::Gt => Ok(ExecBinaryOperator::GreaterThan),
        HirBinOp::Ge => Ok(ExecBinaryOperator::GreaterThanOrEqual),
        HirBinOp::And => Ok(ExecBinaryOperator::And),
        HirBinOp::Or => Ok(ExecBinaryOperator::Or),
        HirBinOp::Exponent => Err(CompilerError::compiler_error(format!(
            "Binary operator {:?} is not yet supported by the interpreter",
            hir_op
        ))),
    }
}

/// Maps a HIR unary operator to the corresponding Exec IR unary operator.
///
/// Returns an error if the HIR operator is not yet supported by the interpreter.
pub(crate) fn map_unary_operator(hir_op: HirUnaryOp) -> Result<ExecUnaryOperator, CompilerError> {
    match hir_op {
        HirUnaryOp::Neg => Ok(ExecUnaryOperator::Negate),
        HirUnaryOp::Not => Ok(ExecUnaryOperator::Not),
    }
}
