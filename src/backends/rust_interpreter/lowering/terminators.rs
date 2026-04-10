//! Terminator lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::{ExecInstruction, ExecTerminator};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression_into;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirTerminator};

pub(crate) fn lower_block_terminator(
    context: &mut LoweringContext<'_>,
    layout: &FunctionLoweringLayout,
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
            lower_expression_into(
                context,
                layout,
                instructions,
                layout.scratch_local_id,
                condition,
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
                condition: layout.scratch_local_id,
                then_block: exec_then_block,
                else_block: exec_else_block,
            })
        }

        HirTerminator::Return(expression) => {
            if is_unit_expression(expression) {
                Ok(ExecTerminator::Return { value: None })
            } else {
                lower_expression_into(
                    context,
                    layout,
                    instructions,
                    layout.scratch_local_id,
                    expression,
                )?;

                Ok(ExecTerminator::Return {
                    value: Some(layout.scratch_local_id),
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
