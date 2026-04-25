//! Terminator lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::{ExecInstruction, ExecTerminator};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
use crate::backends::rust_interpreter::lowering::materialize::local_for_expression_value;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind};
use crate::compiler_frontend::hir::terminators::HirTerminator;

pub(crate) fn lower_block_terminator(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    instructions: &mut Vec<ExecInstruction>,
    terminator: &HirTerminator,
) -> Result<ExecTerminator, CompilerError> {
    match terminator {
        HirTerminator::Jump { target, .. }
        | HirTerminator::Break { target }
        | HirTerminator::Continue { target } => {
            let Some(exec_target) = layout.exec_block_by_hir_block.get(target).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve jump target block {target:?}"
                )));
            };

            Ok(ExecTerminator::Jump {
                target: exec_target,
            })
        }

        HirTerminator::If {
            condition,
            then_block,
            else_block,
        } => {
            let condition_value = lower_expression(context, layout, instructions, condition)?;

            let condition_storage_type = context.lower_storage_type(condition.ty);
            let condition_local = local_for_expression_value(
                context,
                layout,
                instructions,
                condition_value,
                condition_storage_type,
            )?;

            let Some(exec_then_block) = layout.exec_block_by_hir_block.get(then_block).copied()
            else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve then-block {then_block:?}"
                )));
            };

            let Some(exec_else_block) = layout.exec_block_by_hir_block.get(else_block).copied()
            else {
                return Err(CompilerError::compiler_error(format!(
                    "Rust interpreter lowering could not resolve else-block {else_block:?}"
                )));
            };

            Ok(ExecTerminator::BranchBool {
                condition: condition_local,
                then_block: exec_then_block,
                else_block: exec_else_block,
            })
        }

        HirTerminator::Return(expression) => {
            if is_unit_expression(expression) {
                Ok(ExecTerminator::Return { value: None })
            } else {
                let return_value = lower_expression(context, layout, instructions, expression)?;
                let return_storage_type = context.lower_storage_type(expression.ty);
                let return_local = local_for_expression_value(
                    context,
                    layout,
                    instructions,
                    return_value,
                    return_storage_type,
                )?;

                Ok(ExecTerminator::Return {
                    value: Some(return_local),
                })
            }
        }

        HirTerminator::Match { .. } => Ok(ExecTerminator::PendingLowering {
            description: "match terminator lowering is not implemented yet".to_owned(),
        }),

        HirTerminator::Panic { .. } => Ok(ExecTerminator::PendingLowering {
            description: "panic terminator lowering is not implemented yet".to_owned(),
        }),
    }
}

fn is_unit_expression(expression: &HirExpression) -> bool {
    matches!(
        &expression.kind,
        HirExpressionKind::TupleConstruct { elements } if elements.is_empty()
    )
}
