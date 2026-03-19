//! Runtime-template specific lowering helpers.

use crate::backends::wasm::hir_to_lir::context::WasmFunctionLoweringContext;
use crate::backends::wasm::hir_to_lir::expr::lower_expression;
use crate::backends::wasm::hir_to_lir::ownership::insert_advisory_drops;
use crate::backends::wasm::hir_to_lir::static_data::intern_static_utf8;
use crate::backends::wasm::hir_to_lir::stmt::lower_statement;
use crate::backends::wasm::lir::instructions::{WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::types::{WasmAbiType, WasmLocalRole};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    HirBinOp, HirExpression, HirExpressionKind, HirTerminator,
};

pub(crate) fn lower_runtime_template_function(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
) -> Result<(), CompilerError> {
    // WHAT: lower runtime template functions into explicit string-buffer ops.
    // WHY: template runtime behavior must become explicit statements before Wasm emission.
    //
    // Phase-1 note:
    // this implementation only supports concat-like template shapes and keeps
    // unsupported constructs as transformation errors.
    let hir_entry = context.hir_function.entry;
    if !context.block_map.contains_key(&hir_entry) {
        return Err(CompilerError::lir_transformation(format!(
            "Runtime template lowering requires pre-allocated block mapping for {:?}",
            hir_entry
        )));
    }

    let hir_block = context
        .module_context
        .hir_module
        .blocks
        .iter()
        .find(|block| block.id == hir_entry)
        .ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Runtime template lowering could not find entry block {:?}",
                hir_entry
            ))
        })?
        .clone();

    let mut lowered_statements = Vec::new();
    for statement in &hir_block.statements {
        lower_statement(context, statement, &mut lowered_statements)?;
    }

    let HirTerminator::Return(value) = &hir_block.terminator else {
        return Err(CompilerError::lir_transformation(
            "Runtime template function entry block must end with return",
        ));
    };

    // Flatten nested binary-add concatenation into source-order chunks.
    let mut chunks = Vec::new();
    collect_concat_chunks(value, &mut chunks);

    let buffer = context.alloc_local(None, WasmAbiType::Handle, WasmLocalRole::BufferHandle);
    lowered_statements.push(WasmLirStmt::StringNewBuffer { dst: buffer });

    for chunk in chunks {
        // Const literals become interned static segments; dynamic parts become handles.
        match &chunk.kind {
            HirExpressionKind::StringLiteral(literal) => {
                let static_id =
                    intern_static_utf8(context.module_context, literal, "runtime.template.literal");
                lowered_statements.push(WasmLirStmt::StringPushLiteral {
                    buffer,
                    data: static_id,
                });
            }
            _ => {
                let lowered = lower_expression(context, chunk, &mut lowered_statements)?;
                lowered_statements.push(WasmLirStmt::StringPushHandle {
                    buffer,
                    handle: lowered.value,
                });
            }
        }
    }

    let result = context.alloc_local(None, WasmAbiType::Handle, WasmLocalRole::ValueHandle);
    lowered_statements.push(WasmLirStmt::StringFinish {
        dst: result,
        buffer,
    });

    insert_advisory_drops(context, hir_block.id, &mut lowered_statements);

    let lir_block = context.block_mut(hir_block.id).ok_or_else(|| {
        CompilerError::lir_transformation(format!(
            "Runtime template lowering could not resolve LIR block for {:?}",
            hir_block.id
        ))
    })?;
    lir_block.statements = lowered_statements;
    lir_block.terminator = WasmLirTerminator::Return {
        value: Some(result),
    };

    Ok(())
}

fn collect_concat_chunks<'a>(expression: &'a HirExpression, out: &mut Vec<&'a HirExpression>) {
    // WHAT: recursively linearize `a + b + c` into `[a, b, c]`.
    // WHY: preserves deterministic push order in runtime template lowering.
    if let HirExpressionKind::BinOp { left, op, right } = &expression.kind
        && matches!(op, HirBinOp::Add)
    {
        collect_concat_chunks(left, out);
        collect_concat_chunks(right, out);
        return;
    }

    out.push(expression);
}
