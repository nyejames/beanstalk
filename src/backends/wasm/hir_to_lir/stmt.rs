//! Statement lowering for HIR -> Wasm LIR.

use crate::backends::wasm::hir_to_lir::context::WasmFunctionLoweringContext;
use crate::backends::wasm::hir_to_lir::expr::lower_expression;
use crate::backends::wasm::hir_to_lir::imports::resolve_host_call_import;
use crate::backends::wasm::lir::instructions::{WasmCalleeRef, WasmLirStmt};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirPlace, HirStatement, HirStatementKind};
use crate::compiler_frontend::host_functions::CallTarget;

pub(crate) fn lower_statement(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
    statement: &HirStatement,
    statements: &mut Vec<WasmLirStmt>,
) -> Result<(), CompilerError> {
    // Statement lowering is explicitly side-effecting: expressions append LIR
    // statements directly to preserve HIR evaluation order.
    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            lower_assignment(context, target, value, statements)
        }
        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            let mut lowered_args = Vec::with_capacity(args.len());
            for arg in args {
                let lowered = lower_expression(context, arg, statements)?;
                lowered_args.push(lowered.value);
            }

            let callee = match target {
                CallTarget::UserFunction(function_id) => {
                    // User calls stay function-id based after semantic lowering.
                    let function_id = context
                        .module_context
                        .function_map
                        .get(function_id)
                        .copied()
                        .ok_or_else(|| {
                            CompilerError::lir_transformation(format!(
                                "Wasm lowering missing function id mapping for {function_id:?}"
                            ))
                        })?;
                    WasmCalleeRef::Function(function_id)
                }
                CallTarget::HostFunction(_) => {
                    // Host calls lower to deterministic import ids.
                    let import_id = resolve_host_call_import(context.module_context, target)?;
                    WasmCalleeRef::Import(import_id)
                }
            };

            let dst = result
                .as_ref()
                .and_then(|local_id| context.local_map.get(local_id).copied());

            statements.push(WasmLirStmt::Call {
                dst,
                callee,
                args: lowered_args,
            });

            Ok(())
        }
        HirStatementKind::Expr(expression) => {
            let _ = lower_expression(context, expression, statements)?;
            Ok(())
        }
        HirStatementKind::Drop(local_id) => {
            // Keep explicit source-level drops in LIR when the value is handle-like.
            let mapped_local = context.local_map.get(local_id).copied().ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Wasm lowering could not resolve drop local {local_id:?}"
                ))
            })?;

            if context.is_handle_local(mapped_local) {
                statements.push(WasmLirStmt::DropIfOwned {
                    value: mapped_local,
                });
            }

            Ok(())
        }
    }
}

fn lower_assignment(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
    target: &HirPlace,
    value: &crate::compiler_frontend::hir::hir_nodes::HirExpression,
    statements: &mut Vec<WasmLirStmt>,
) -> Result<(), CompilerError> {
    // WHAT: preserve explicit move/copy distinction in LIR.
    // WHY: ownership optimization stays representable even under GC-first semantics.
    let HirPlace::Local(target_local) = target else {
        return Err(CompilerError::lir_transformation(
            "Wasm lowering currently supports assignments only to direct locals",
        ));
    };

    let dst = context
        .local_map
        .get(target_local)
        .copied()
        .ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve assignment target local {target_local:?}",
            ))
        })?;

    let lowered = lower_expression(context, value, statements)?;
    if lowered.value == dst {
        return Ok(());
    }

    if lowered.prefer_move {
        statements.push(WasmLirStmt::Move {
            dst,
            src: lowered.value,
        });
    } else {
        statements.push(WasmLirStmt::Copy {
            dst,
            src: lowered.value,
        });
    }

    Ok(())
}
