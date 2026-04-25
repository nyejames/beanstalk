//! Expression lowering for the interpreter backend.
//!
//! WHAT: lowers a restricted subset of HIR expressions into Exec IR instructions.
//! WHY: phase 1 needs a tiny executable core before broader language support is added.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBinaryOperator, ExecConstValue, ExecInstruction, ExecStorageType, ExecUnaryOperator,
};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::materialize::{
    LoweredExpressionValue, local_for_expression_value,
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
) -> Result<LoweredExpressionValue, CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(value) => {
            Ok(LoweredExpressionValue::Literal(ExecConstValue::Int(*value)))
        }

        HirExpressionKind::Float(value) => Ok(LoweredExpressionValue::Literal(
            ExecConstValue::Float(*value),
        )),

        HirExpressionKind::Bool(value) => Ok(LoweredExpressionValue::Literal(
            ExecConstValue::Bool(*value),
        )),

        HirExpressionKind::Char(value) => Ok(LoweredExpressionValue::Literal(
            ExecConstValue::Char(*value),
        )),

        HirExpressionKind::StringLiteral(text) => Ok(LoweredExpressionValue::Literal(
            ExecConstValue::String(text.to_owned()),
        )),

        HirExpressionKind::Load(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {local_id:?} for load expression"
                )));
            };

            Ok(LoweredExpressionValue::LocalRead(source))
        }

        HirExpressionKind::Copy(HirPlace::Local(local_id)) => {
            let Some(source) = layout.exec_local_by_hir_local.get(local_id).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve local {local_id:?} for copy expression"
                )));
            };

            Ok(LoweredExpressionValue::LocalCopy(source))
        }

        HirExpressionKind::Load(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local load places yet: {place:?}"
        ))),

        HirExpressionKind::Copy(place) => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support non-local copy places yet: {place:?}"
        ))),

        HirExpressionKind::TupleConstruct { elements } if elements.is_empty() => {
            Ok(LoweredExpressionValue::Literal(ExecConstValue::Unit))
        }

        HirExpressionKind::BinOp { left, op, right } => {
            let left_value = lower_expression(context, layout, instructions, left)?;
            let right_value = lower_expression(context, layout, instructions, right)?;

            let left_storage_type = context.lower_storage_type(left.ty);
            let left_local = local_for_expression_value(
                context,
                layout,
                instructions,
                left_value,
                left_storage_type,
            )?;
            let right_storage_type = context.lower_storage_type(right.ty);
            let right_local = local_for_expression_value(
                context,
                layout,
                instructions,
                right_value,
                right_storage_type,
            )?;

            let exec_operator = map_binary_operator(*op)?;
            let result_storage_type =
                storage_type_for_binary_result(context, expression, exec_operator);
            let result_local = layout.allocate_temporary_local(result_storage_type);

            instructions.push(ExecInstruction::BinaryOp {
                left: left_local,
                operator: exec_operator,
                right: right_local,
                destination: result_local,
            });

            Ok(LoweredExpressionValue::LocalRead(result_local))
        }

        HirExpressionKind::UnaryOp { op, operand } => {
            let operand_value = lower_expression(context, layout, instructions, operand)?;
            let operand_storage_type = context.lower_storage_type(operand.ty);
            let operand_local = local_for_expression_value(
                context,
                layout,
                instructions,
                operand_value,
                operand_storage_type,
            )?;

            let exec_operator = map_unary_operator(*op)?;
            let result_storage_type =
                storage_type_for_unary_result(context, expression, exec_operator);
            let result_local = layout.allocate_temporary_local(result_storage_type);

            instructions.push(ExecInstruction::UnaryOp {
                operand: operand_local,
                operator: exec_operator,
                destination: result_local,
            });

            Ok(LoweredExpressionValue::LocalRead(result_local))
        }

        _ => Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering does not support HIR expression kind yet: {:?}",
            expression.kind
        ))),
    }
}

fn storage_type_for_binary_result(
    context: &LoweringContext<'_>,
    expression: &HirExpression,
    operator: ExecBinaryOperator,
) -> ExecStorageType {
    match operator {
        ExecBinaryOperator::Equal
        | ExecBinaryOperator::NotEqual
        | ExecBinaryOperator::LessThan
        | ExecBinaryOperator::LessThanOrEqual
        | ExecBinaryOperator::GreaterThan
        | ExecBinaryOperator::GreaterThanOrEqual
        | ExecBinaryOperator::And
        | ExecBinaryOperator::Or => ExecStorageType::Bool,
        _ => context.lower_storage_type(expression.ty),
    }
}

fn storage_type_for_unary_result(
    context: &LoweringContext<'_>,
    expression: &HirExpression,
    operator: ExecUnaryOperator,
) -> ExecStorageType {
    match operator {
        ExecUnaryOperator::Not => ExecStorageType::Bool,
        ExecUnaryOperator::Negate => context.lower_storage_type(expression.ty),
    }
}
