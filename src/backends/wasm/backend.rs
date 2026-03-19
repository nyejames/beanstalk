//! Top-level orchestration for HIR -> Wasm LIR lowering.

use crate::backends::wasm::debug::build_debug_outputs;
use crate::backends::wasm::hir_to_lir::module::lower_hir_module_to_lir;
use crate::backends::wasm::request::WasmBackendRequest;
use crate::backends::wasm::result::WasmLirBackendResult;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerError, CompilerMessages,
};
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirModule};
use std::collections::HashSet;

pub(crate) fn lower_hir_to_wasm_lir(
    hir_module: &HirModule,
    borrow_facts: &BorrowFacts,
    request: &WasmBackendRequest,
) -> Result<WasmLirBackendResult, CompilerMessages> {
    // WHAT: fail fast on builder/backend contract issues.
    // WHY: avoid partial lowering and keep diagnostics deterministic.
    validate_request(hir_module, request).map_err(single_error)?;

    // WHAT: perform full module lowering using HIR + borrow side tables.
    // WHY: phase-1 establishes the stable HIR->LIR seam for later Wasm emission.
    let lir_module = lower_hir_module_to_lir(hir_module, borrow_facts, request)?;
    // WHAT: collect optional debug text with zero impact on lowering semantics.
    let debug_outputs = build_debug_outputs(request, &lir_module);

    Ok(WasmLirBackendResult {
        lir_module,
        debug_outputs,
    })
}

fn validate_request(
    hir_module: &HirModule,
    request: &WasmBackendRequest,
) -> Result<(), CompilerError> {
    // Phase-1 note:
    // This is strict by design even before Wasm emission exists. It prevents
    // builder drift and makes export policy failures obvious immediately.
    let mut seen = HashSet::new();
    let mut export_name_set = HashSet::new();

    for function_id in &request.export_policy.exported_functions {
        if !seen.insert(function_id.0) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request contains duplicate export target {:?}",
                function_id
            )));
        }

        if !contains_function(hir_module, *function_id) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request references missing function {:?}",
                function_id
            )));
        }

        if !request.export_policy.export_names.contains_key(function_id) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request missing stable export name for {:?}",
                function_id
            )));
        }

        let export_name = request
            .export_policy
            .export_names
            .get(function_id)
            .expect("checked contains_key above");
        if !export_name_set.insert(export_name.clone()) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request contains duplicate export name '{}'",
                export_name
            )));
        }
    }

    for (function_id, export_name) in &request.export_policy.export_names {
        if !contains_function(hir_module, *function_id) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request has export name entry for unknown function {:?}",
                function_id
            )));
        }

        if export_name.trim().is_empty() {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request contains an empty export name for {:?}",
                function_id
            )));
        }

        if !seen.contains(&function_id.0) {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm backend request has export name entry for {:?} but it is not in exported_functions",
                function_id
            )));
        }
    }

    Ok(())
}

fn contains_function(hir_module: &HirModule, function_id: FunctionId) -> bool {
    hir_module
        .functions
        .iter()
        .any(|function| function.id == function_id)
}

fn single_error(error: CompilerError) -> CompilerMessages {
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
    }
}
