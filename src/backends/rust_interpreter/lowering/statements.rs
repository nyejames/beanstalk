//! Statement lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::ExecInstruction;
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
use crate::backends::rust_interpreter::lowering::materialize::{
    lower_expression_to_temporary, materialize_expression_value,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;

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

                    materialize_expression_value(
                        context,
                        &mut instructions,
                        exec_target,
                        value_result,
                    )?;
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

                let storage_type = context.lower_storage_type(expression.ty);
                lower_expression_to_temporary(
                    context,
                    layout,
                    &mut instructions,
                    value_result,
                    storage_type,
                )?;
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
