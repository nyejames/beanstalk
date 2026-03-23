//! Module-level orchestration for HIR -> LIR lowering.

use crate::backends::wasm::hir_to_lir::context::WasmLirLoweringContext;
use crate::backends::wasm::hir_to_lir::exports::synthesize_export_wrappers;
use crate::backends::wasm::hir_to_lir::function::lower_function;
use crate::backends::wasm::hir_to_lir::imports::register_required_host_imports;
use crate::backends::wasm::lir::types::WasmLirFunctionId;
use crate::backends::wasm::request::WasmBackendRequest;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerError, CompilerMessages,
};
use crate::compiler_frontend::hir::hir_nodes::HirModule;

pub(crate) fn lower_hir_module_to_lir(
    hir_module: &HirModule,
    borrow_facts: &BorrowFacts,
    request: &WasmBackendRequest,
) -> Result<crate::backends::wasm::lir::module::WasmLirModule, CompilerMessages> {
    // WHAT: one mutable context carries all per-module lowering state.
    // WHY: keeps interning/ids/import planning coherent and deterministic.
    let mut context = WasmLirLoweringContext::new(hir_module, borrow_facts, request);

    // WHAT: register stable function mappings before lowering any bodies.
    // WHY: call lowering depends on these mappings even for forward references.
    register_function_maps(&mut context).map_err(CompilerMessages::from_error)?;
    // WHAT: pre-register host imports required by the module.
    // WHY: keeps import ids deterministic and avoids re-scan during statement lowering.
    register_required_host_imports(&mut context).map_err(CompilerMessages::from_error)?;

    // WHAT: sort functions by id.
    // WHY: ensures deterministic output regardless of source container ordering.
    let mut functions = hir_module.functions.clone();
    functions.sort_by_key(|function| function.id.0);

    for hir_function in &functions {
        let lowered = lower_function(&mut context, hir_function).map_err(CompilerMessages::from_error)?;
        context.lir_module.functions.push(lowered);
    }

    // WHAT: synthesize post-lowering export wrapper functions.
    // WHY: wrapper boundary keeps user bodies internal while export ABI stays stable.
    synthesize_export_wrappers(&mut context).map_err(CompilerMessages::from_error)?;

    Ok(context.lir_module)
}

fn register_function_maps(context: &mut WasmLirLoweringContext<'_>) -> Result<(), CompilerError> {
    // Phase-1 note:
    // this mapping is intentionally explicit scaffolding so later phases can
    // reuse identical ids for code section ordering and debug correlation.
    for (next_id, function) in context.hir_module.functions.iter().enumerate() {
        context
            .function_map
            .insert(function.id, WasmLirFunctionId(next_id as u32));

        let Some(path) = context
            .hir_module
            .side_table
            .function_name_path(function.id)
        else {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve function path for {:?}",
                function.id
            )));
        };

        context
            .function_id_by_path
            .insert(path.clone(), function.id);
    }

    Ok(())
}

