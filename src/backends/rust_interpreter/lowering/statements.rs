//! Statement lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::{ExecInstruction, ExecValue};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirBlock, HirPlace, HirStatementKind};

pub(crate) fn lower_block_statements(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    hir_block: &HirBlock,
) -> Result<Vec<ExecInstruction>, CompilerError> {
    let mut instructions = Vec::new();

    for statement in &hir_block.statements {
        match &statement.kind {
            HirStatementKind::Assign { target, value } => match target {
                HirPlace::Local(local_id) => {
                    let Some(exec_target) = layout.exec_local_by_hir_local.get(local_id).copied()
                    else {
                        return Err(CompilerError::compiler_error(format!(
                            "Rust interpreter lowering could not resolve assignment target local {local_id:?}"
                        )));
                    };

                    let value_result = lower_expression(context, layout, &mut instructions, value)?;

                    // Materialize the value to the target local
                    match value_result {
                        ExecValue::Local(source) => {
                            instructions.push(ExecInstruction::CopyLocal {
                                target: exec_target,
                                source,
                            });
                        }
                        ExecValue::Literal(const_value) => {
                            let const_id = context.intern_const(const_value);
                            instructions.push(ExecInstruction::LoadConst {
                                target: exec_target,
                                const_id,
                            });
                        }
                    }
                }

                _ => {
                    return Err(CompilerError::compiler_error(format!(
                        "Rust interpreter lowering does not support non-local assignment targets yet: {target:?}"
                    )));
                }
            },

            HirStatementKind::Expr(expression) => {
                let value_result =
                    lower_expression(context, layout, &mut instructions, expression)?;

                // For expression statements, we need to materialize the value to discard it
                // This ensures side effects are preserved even if the value isn't used
                match value_result {
                    ExecValue::Local(_) => {
                        // Value is already in a local, no need to do anything
                    }
                    ExecValue::Literal(const_value) => {
                        // Materialize to scratch local to ensure consistent behavior
                        let const_id = context.intern_const(const_value);
                        instructions.push(ExecInstruction::LoadConst {
                            target: layout.scratch_local_id,
                            const_id,
                        });
                    }
                }
            }

            HirStatementKind::Call { .. } => {
                return Err(CompilerError::compiler_error(
                    "Rust interpreter lowering does not support call statements yet",
                ));
            }

            HirStatementKind::Drop(_) => {
                // GC-first runtime semantics treat explicit drop as a no-op for now.
            }

            HirStatementKind::PushRuntimeFragment { .. } => {
                // PushRuntimeFragment is only valid inside entry start().
                // The Rust interpreter does not support HTML fragment assembly.
                return Err(CompilerError::compiler_error(
                    "Rust interpreter: PushRuntimeFragment lowering is not supported",
                ));
            }
        }
    }

    Ok(instructions)
}
