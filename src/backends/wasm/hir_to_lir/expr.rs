//! Expression lowering helpers for HIR -> Wasm LIR.

use crate::backends::wasm::hir_to_lir::context::WasmFunctionLoweringContext;
use crate::backends::wasm::hir_to_lir::static_data::intern_static_utf8;
use crate::backends::wasm::lir::instructions::WasmLirStmt;
use crate::backends::wasm::lir::types::{WasmAbiType, WasmLirLocalId, WasmLocalRole};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    HirBinOp, HirExpression, HirExpressionKind, HirPlace,
};

/// Result of lowering a single HIR expression into LIR statements and a destination local.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExprLoweringOutput {
    /// Local containing the lowered expression value/handle.
    pub value: WasmLirLocalId,
    /// Advisory move hint for assignment/call-site lowering.
    /// WHY: phase-1 still distinguishes move/copy at LIR level.
    pub prefer_move: bool,
}

pub(crate) fn lower_expression(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
    expression: &HirExpression,
    statements: &mut Vec<WasmLirStmt>,
) -> Result<ExprLoweringOutput, CompilerError> {
    // Phase-1 note:
    // this matcher is intentionally partial. Unsupported HIR expression kinds
    // return structured LirTransformation errors instead of panicking.
    match &expression.kind {
        HirExpressionKind::Int(value) => {
            let dst = context.alloc_temp(WasmAbiType::I64);
            statements.push(WasmLirStmt::ConstI64 { dst, value: *value });
            Ok(ExprLoweringOutput {
                value: dst,
                prefer_move: false,
            })
        }
        HirExpressionKind::Float(value) => {
            let dst = context.alloc_temp(WasmAbiType::F64);
            statements.push(WasmLirStmt::ConstF64 { dst, value: *value });
            Ok(ExprLoweringOutput {
                value: dst,
                prefer_move: false,
            })
        }
        HirExpressionKind::Bool(value) => {
            let dst = context.alloc_temp(WasmAbiType::I32);
            statements.push(WasmLirStmt::ConstI32 {
                dst,
                value: if *value { 1 } else { 0 },
            });
            Ok(ExprLoweringOutput {
                value: dst,
                prefer_move: false,
            })
        }
        HirExpressionKind::Char(value) => {
            let dst = context.alloc_temp(WasmAbiType::I32);
            statements.push(WasmLirStmt::ConstI32 {
                dst,
                value: *value as i32,
            });
            Ok(ExprLoweringOutput {
                value: dst,
                prefer_move: false,
            })
        }
        HirExpressionKind::StringLiteral(value) => {
            // String literal lowering goes through runtime buffer ops so the
            // same model can be reused for runtime template fragments.
            let data_id = intern_static_utf8(context.module_context, value, "hir.string_literal");
            let buffer =
                context.alloc_local(None, WasmAbiType::Handle, WasmLocalRole::BufferHandle);
            statements.push(WasmLirStmt::StringNewBuffer { dst: buffer });
            statements.push(WasmLirStmt::StringPushLiteral {
                buffer,
                data: data_id,
            });

            let dst = context.alloc_local(None, WasmAbiType::Handle, WasmLocalRole::ValueHandle);
            statements.push(WasmLirStmt::StringFinish { dst, buffer });

            Ok(ExprLoweringOutput {
                value: dst,
                prefer_move: false,
            })
        }
        HirExpressionKind::Load(place) => {
            let local = lower_place_local(context, place)?;
            Ok(ExprLoweringOutput {
                value: local,
                prefer_move: true,
            })
        }
        HirExpressionKind::Copy(place) => {
            let local = lower_place_local(context, place)?;
            Ok(ExprLoweringOutput {
                value: local,
                prefer_move: false,
            })
        }
        HirExpressionKind::BinOp { left, op, right } => {
            let lhs = lower_expression(context, left, statements)?;
            let rhs = lower_expression(context, right, statements)?;

            match op {
                HirBinOp::Eq => {
                    let dst = context.alloc_temp(WasmAbiType::I32);
                    statements.push(WasmLirStmt::IntEq {
                        dst,
                        lhs: lhs.value,
                        rhs: rhs.value,
                    });
                    Ok(ExprLoweringOutput {
                        value: dst,
                        prefer_move: false,
                    })
                }
                HirBinOp::Ne => {
                    let dst = context.alloc_temp(WasmAbiType::I32);
                    statements.push(WasmLirStmt::IntNe {
                        dst,
                        lhs: lhs.value,
                        rhs: rhs.value,
                    });
                    Ok(ExprLoweringOutput {
                        value: dst,
                        prefer_move: false,
                    })
                }
                _ => Err(CompilerError::lir_transformation(format!(
                    "Wasm lowering does not yet support binary operator {:?}",
                    op
                ))),
            }
        }
        HirExpressionKind::UnaryOp { op, .. } => Err(CompilerError::lir_transformation(format!(
            "Wasm lowering does not yet support unary operator {:?}",
            op
        ))),
        HirExpressionKind::StructConstruct { .. }
        | HirExpressionKind::Collection(_)
        | HirExpressionKind::Range { .. }
        | HirExpressionKind::TupleConstruct { .. }
        | HirExpressionKind::TupleGet { .. }
        | HirExpressionKind::OptionConstruct { .. }
        | HirExpressionKind::ResultConstruct { .. }
        | HirExpressionKind::ResultPropagate { .. }
        | HirExpressionKind::ResultIsOk { .. }
        | HirExpressionKind::ResultUnwrapOk { .. }
        | HirExpressionKind::ResultUnwrapErr { .. } => Err(CompilerError::lir_transformation(
            "Wasm lowering does not yet support this expression construct",
        )),
    }
}

fn lower_place_local(
    context: &WasmFunctionLoweringContext<'_, '_>,
    place: &HirPlace,
) -> Result<WasmLirLocalId, CompilerError> {
    // WHAT: place lowering currently supports direct locals only.
    // WHY: field/index projections require additional memory model work (phase-2+).
    match place {
        HirPlace::Local(local_id) => context.local_map.get(local_id).copied().ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve local {:?}",
                local_id
            ))
        }),
        HirPlace::Field { .. } | HirPlace::Index { .. } => Err(CompilerError::lir_transformation(
            "Wasm lowering currently supports only direct local places",
        )),
    }
}
