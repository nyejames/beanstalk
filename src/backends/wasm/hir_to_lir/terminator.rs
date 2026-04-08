//! Terminator lowering for HIR -> Wasm LIR.

use crate::backends::wasm::hir_to_lir::context::{WasmFunctionLoweringContext, lower_type_to_abi};
use crate::backends::wasm::hir_to_lir::expr::lower_expression;
use crate::backends::wasm::lir::instructions::{WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::types::WasmAbiType;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{BlockId, HirTerminator};

pub(crate) fn lower_terminator(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
    terminator: &HirTerminator,
    statements: &mut Vec<WasmLirStmt>,
) -> Result<WasmLirTerminator, CompilerError> {
    // Phase-1 note:
    // unsupported high-level control-flow forms intentionally error here so
    // lowering failures are structured and visible.
    match terminator {
        HirTerminator::Jump { target, .. }
        | HirTerminator::Break { target }
        | HirTerminator::Continue { target } => {
            Ok(WasmLirTerminator::Jump(resolve_block_id(context, *target)?))
        }
        HirTerminator::If {
            condition,
            then_block,
            else_block,
        } => {
            let lowered_condition = lower_expression(context, condition, statements)?;
            Ok(WasmLirTerminator::Branch {
                condition: lowered_condition.value,
                then_block: resolve_block_id(context, *then_block)?,
                else_block: resolve_block_id(context, *else_block)?,
            })
        }
        HirTerminator::Return(value) => {
            // Preserve unit-return as `Return(None)` to keep ABI shape explicit.
            let return_abi = lower_type_to_abi(context.module_context, value.ty);
            if matches!(return_abi, WasmAbiType::Void) {
                return Ok(WasmLirTerminator::Return { value: None });
            }

            let lowered_value = lower_expression(context, value, statements)?;
            Ok(WasmLirTerminator::Return {
                value: Some(lowered_value.value),
            })
        }
        HirTerminator::Panic { .. } => Ok(WasmLirTerminator::Trap),
        HirTerminator::Match { .. } => Err(CompilerError::lir_transformation(
            "Wasm lowering does not yet support this terminator",
        )),
    }
}

fn resolve_block_id(
    context: &WasmFunctionLoweringContext<'_, '_>,
    block_id: BlockId,
) -> Result<crate::backends::wasm::lir::types::WasmLirBlockId, CompilerError> {
    context.block_map.get(&block_id).copied().ok_or_else(|| {
        CompilerError::lir_transformation(format!(
            "Wasm lowering could not resolve block id {block_id:?}"
        ))
    })
}
