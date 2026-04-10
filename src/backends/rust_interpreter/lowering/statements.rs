//! Statement lowering for the interpreter backend.

use crate::backends::rust_interpreter::exec_ir::ExecInstruction;
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::expressions::lower_expression_into;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirBlock, HirPlace, HirStatementKind};

pub(crate) fn lower_block_statements(
    context: &mut LoweringContext<'_>,
    layout: &FunctionLoweringLayout,
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

                    lower_expression_into(context, layout, &mut instructions, exec_target, value)?;
                }

                _ => {
                    return Err(CompilerError::compiler_error(format!(
                        "Rust interpreter lowering does not support non-local assignment targets yet: {target:?}"
                    )));
                }
            },

            HirStatementKind::Expr(expression) => {
                lower_expression_into(
                    context,
                    layout,
                    &mut instructions,
                    layout.scratch_local_id,
                    expression,
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
        }
    }

    Ok(instructions)
}
