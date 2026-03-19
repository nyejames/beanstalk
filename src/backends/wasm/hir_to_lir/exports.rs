//! Export-wrapper synthesis for phase-1 Wasm lowering.

use crate::backends::wasm::hir_to_lir::context::WasmLirLoweringContext;
use crate::backends::wasm::lir::function::{WasmLirBlock, WasmLirFunction, WasmLirFunctionOrigin};
use crate::backends::wasm::lir::instructions::{WasmCalleeRef, WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::linkage::{WasmExport, WasmExportKind, WasmFunctionLinkage};
use crate::backends::wasm::lir::types::{
    WasmLirBlockId, WasmLirFunctionId, WasmLirLocal, WasmLirLocalId, WasmLocalRole,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use rustc_hash::FxHashSet;

pub(crate) fn synthesize_export_wrappers(
    context: &mut WasmLirLoweringContext<'_>,
) -> Result<(), CompilerError> {
    // WHAT: create synthetic wrapper functions for requested exports only.
    // WHY: keeps internal function linkage separate from externally stable names.
    //
    // Phase-1 note:
    // wrapper bodies are intentionally minimal (single call + return). ABI adaptation
    // and richer marshaling are phase-2/3 work.
    if context.request.export_policy.exported_functions.is_empty() {
        return Ok(());
    }

    let mut wrapper_id = context.lir_module.functions.len() as u32;
    let mut seen_export_names = FxHashSet::default();

    for function_id in &context.request.export_policy.exported_functions {
        let export_name = context
            .request
            .export_policy
            .export_names
            .get(function_id)
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Wasm export wrapper synthesis missing export name for {:?}",
                    function_id
                ))
            })?;

        if !seen_export_names.insert(export_name.clone()) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm export wrapper synthesis encountered duplicate export name '{}'",
                export_name
            )));
        }

        let target_lir_id = context
            .function_map
            .get(function_id)
            .copied()
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Wasm export wrapper synthesis could not resolve target function {:?}",
                    function_id
                ))
            })?;

        // Target function must already be lowered before wrapper synthesis.
        let target_signature = context
            .lir_module
            .functions
            .iter()
            .find(|function| function.id == target_lir_id)
            .map(|function| function.signature.clone())
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Wasm export wrapper synthesis missing lowered target function {:?}",
                    target_lir_id
                ))
            })?;

        if target_signature.results.len() > 1 {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm export wrapper synthesis does not yet support multi-value returns for {:?}",
                target_lir_id
            )));
        }

        // Wrapper local layout mirrors target signature params/results.
        let mut locals = Vec::new();
        let mut args = Vec::new();
        let mut next_local_id = 0u32;

        for (index, abi) in target_signature.params.iter().enumerate() {
            let local_id = WasmLirLocalId(next_local_id);
            next_local_id += 1;
            locals.push(WasmLirLocal {
                id: local_id,
                name: Some(format!("arg{index}")),
                ty: *abi,
                role: WasmLocalRole::Param,
            });
            args.push(local_id);
        }

        let result_local = target_signature.results.first().copied().map(|result_abi| {
            let local_id = WasmLirLocalId(next_local_id);
            locals.push(WasmLirLocal {
                id: local_id,
                name: Some("result".to_owned()),
                ty: result_abi,
                role: WasmLocalRole::Temp,
            });
            local_id
        });

        let statements = vec![WasmLirStmt::Call {
            dst: result_local,
            callee: WasmCalleeRef::Function(target_lir_id),
            args,
        }];

        let wrapper_function_id = WasmLirFunctionId(wrapper_id);
        wrapper_id += 1;

        context.lir_module.functions.push(WasmLirFunction {
            id: wrapper_function_id,
            debug_name: format!("export_wrapper::{export_name}"),
            origin: WasmLirFunctionOrigin::ExportWrapper,
            signature: target_signature,
            locals,
            blocks: vec![WasmLirBlock {
                id: WasmLirBlockId(0),
                statements,
                terminator: WasmLirTerminator::Return {
                    value: result_local,
                },
            }],
            linkage: WasmFunctionLinkage::ExportedWrapper,
        });

        context.lir_module.exports.push(WasmExport {
            export_name: export_name.to_owned(),
            kind: WasmExportKind::Function(wrapper_function_id),
        });
    }

    Ok(())
}
