//! Expression lowering for the interpreter backend.
//!
//! WHAT: lowers a restricted subset of HIR expressions into Exec IR instructions.
//! WHY: phase 1 needs a tiny executable core before broader language support is added.

use crate::backends::rust_interpreter::exec_ir::{
    ExecConstValue, ExecInstruction, ExecLocalId, ExecStorageType, ExecValue,
};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::operators::{
    map_binary_operator, map_unary_operator,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirPlace};

pub(crate) fn lower_expression(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    expression: &HirExpression,
) -> Result<ExecValue, CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(value) => Ok(ExecValue::Literal(ExecConstValue::Int(*value))),

        HirExpressionKind::Float(value) => Ok(ExecValue::Literal(ExecConstValue::Float(*value))),

        HirExpressionKind::Bool(value) => Ok(ExecValue::Literal(ExecConstValue::Bool(*value))),

        HirExpressionKind::Char(value) => Ok(ExecValue::Literal(ExecConstValue::Char(*value))),

        HirExpressionKind::StringLiteral(text) => {
            Ok(ExecValue::Literal(ExecConstValue::String(text.to_owned())))
        }

        HirExpressionKind::Load(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {local_id:?} for load expression"
                )));
            };

            Ok(ExecValue::Local(source))
        }

        HirExpressionKind::Copy(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {local_id:?} for copy expression"
                )));
            };

            Ok(ExecValue::Local(source))
        }

        HirExpressionKind::Load(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local load places yet: {place:?}"
        ))),

        HirExpressionKind::Copy(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local copy places yet: {place:?}"
        ))),

        HirExpressionKind::TupleConstruct { elements } if elements.is_empty() => {
            Ok(ExecValue::Literal(ExecConstValue::Unit))
        }

        HirExpressionKind::BinOp { left, op, right } => {
            // Recursively lower left operand.
            let left_value = lower_expression(context, layout, instructions, left)?;

            // Recursively lower right operand.
            let right_value = lower_expression(context, layout, instructions, right)?;

            // Ensure both operands are in locals (allocate temporaries if needed).
            let left_local = ensure_value_in_local(context, layout, instructions, left_value)?;
            let right_local = ensure_value_in_local(context, layout, instructions, right_value)?;

            // Map HIR operator to Exec operator.
            let exec_operator = map_binary_operator(*op)?;

            // Allocate temporary for result.
            // WHAT: determine result storage type based on operator.
            // WHY: comparison operators produce Bool, arithmetic operators preserve operand type.
            let result_storage_type = match exec_operator {
                crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::Equal
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::NotEqual
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::LessThan
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::LessThanOrEqual
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::GreaterThan
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::GreaterThanOrEqual
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::And
                | crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator::Or => {
                    ExecStorageType::Bool
                }
                _ => {
                    // For arithmetic operators, use the expression's result type.
                    context.lower_storage_type(expression.ty)
                }
            };

            let result_local = layout.allocate_temp_local(result_storage_type);

            // Emit BinaryOp instruction.
            instructions.push(ExecInstruction::BinaryOp {
                left: left_local,
                operator: exec_operator,
                right: right_local,
                destination: result_local,
            });

            // Return result local reference.
            Ok(ExecValue::Local(result_local))
        }

        HirExpressionKind::UnaryOp { op, operand } => {
            // Recursively lower operand to ExecValue.
            let operand_value = lower_expression(context, layout, instructions, operand)?;

            // Ensure operand is in a local (allocate temporary if needed).
            let operand_local =
                ensure_value_in_local(context, layout, instructions, operand_value)?;

            // Map HIR operator to Exec operator using map_unary_operator.
            let exec_operator = map_unary_operator(*op)?;

            // Allocate temporary for result.
            // WHAT: determine result storage type based on operator.
            // WHY: Not operator produces Bool, Negate operator preserves operand type.
            let result_storage_type = match exec_operator {
                crate::backends::rust_interpreter::exec_ir::ExecUnaryOperator::Not => {
                    ExecStorageType::Bool
                }
                crate::backends::rust_interpreter::exec_ir::ExecUnaryOperator::Negate => {
                    // For Negate operator, use the expression's result type.
                    context.lower_storage_type(expression.ty)
                }
            };

            let result_local = layout.allocate_temp_local(result_storage_type);

            // Emit UnaryOp instruction.
            instructions.push(ExecInstruction::UnaryOp {
                operand: operand_local,
                operator: exec_operator,
                destination: result_local,
            });

            // Return ExecValue::Local with result temporary.
            Ok(ExecValue::Local(result_local))
        }

        _ => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support HIR expression kind yet: {:?}",
            expression.kind
        ))),
    }
}

/// WHAT: ensures a value is materialized in a local, allocating a temporary if needed.
/// WHY: binary operations require operands to be in locals, but literals can be returned
///      directly from expression lowering to avoid unnecessary temporaries.
fn ensure_value_in_local(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    value: ExecValue,
) -> Result<ExecLocalId, CompilerError> {
    match value {
        ExecValue::Local(local_id) => Ok(local_id),
        ExecValue::Literal(const_value) => {
            // Allocate a temporary local for the literal.
            let storage_type = match &const_value {
                ExecConstValue::Unit => ExecStorageType::Unit,
                ExecConstValue::Bool(_) => ExecStorageType::Bool,
                ExecConstValue::Int(_) => ExecStorageType::Int,
                ExecConstValue::Float(_) => ExecStorageType::Float,
                ExecConstValue::Char(_) => ExecStorageType::Char,
                ExecConstValue::String(_) => ExecStorageType::HeapHandle,
            };

            let temp_local = layout.allocate_temp_local(storage_type);

            // Intern the constant and emit LoadConst instruction.
            let const_id = context.intern_const(const_value);
            instructions.push(ExecInstruction::LoadConst {
                target: temp_local,
                const_id,
            });

            Ok(temp_local)
        }
    }
}
