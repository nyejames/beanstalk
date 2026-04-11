//! Terminator lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::{
    ExecConstValue, ExecInstruction, ExecStorageType, ExecTerminator, ExecValue,
};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirTerminator};

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

            // Materialize the condition to a local if it's a literal
            let condition_local = match condition_value {
                ExecValue::Local(local_id) => local_id,
                ExecValue::Literal(const_value) => {
                    let temp_local = layout.allocate_temp_local(ExecStorageType::Bool);
                    let const_id = context.intern_const(const_value);
                    instructions.push(ExecInstruction::LoadConst {
                        target: temp_local,
                        const_id,
                    });
                    temp_local
                }
            };

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

                // Materialize the return value to a local if it's a literal
                let return_local = match return_value {
                    ExecValue::Local(local_id) => local_id,
                    ExecValue::Literal(const_value) => {
                        // Infer storage type from the constant value
                        let storage_type = match &const_value {
                            ExecConstValue::Unit => ExecStorageType::Unit,
                            ExecConstValue::Bool(_) => ExecStorageType::Bool,
                            ExecConstValue::Int(_) => ExecStorageType::Int,
                            ExecConstValue::Float(_) => ExecStorageType::Float,
                            ExecConstValue::Char(_) => ExecStorageType::Char,
                            ExecConstValue::String(_) => ExecStorageType::HeapHandle,
                        };
                        let temp_local = layout.allocate_temp_local(storage_type);
                        let const_id = context.intern_const(const_value);
                        instructions.push(ExecInstruction::LoadConst {
                            target: temp_local,
                            const_id,
                        });
                        temp_local
                    }
                };

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
