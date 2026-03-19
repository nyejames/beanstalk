//! Debug output builders for the Wasm backend.
//!
//! These are intentionally text-first and lightweight for phase-1 bring-up.
//! They are not a stability guarantee for external tooling yet.

use crate::backends::wasm::lir::debug_dump::dump_lir_module;
use crate::backends::wasm::lir::module::WasmLirModule;
use crate::backends::wasm::request::WasmBackendRequest;
use crate::backends::wasm::result::WasmDebugOutputs;
use std::fmt::Write as _;

pub(crate) fn build_debug_outputs(
    request: &WasmBackendRequest,
    module: &WasmLirModule,
) -> WasmDebugOutputs {
    // WHAT: gate each output independently so callers only pay for requested views.
    let mut outputs = WasmDebugOutputs::default();

    if request.debug_flags.show_wasm_plan {
        outputs.plan_text = Some(render_plan_text(request, module));
    }

    if request.debug_flags.show_lir {
        outputs.lir_text = Some(dump_lir_module(module));
    }

    if request.debug_flags.show_wasm_exports {
        outputs.exports_text = Some(render_exports_text(module));
    }

    if request.debug_flags.show_wasm_runtime_layout {
        outputs.runtime_layout_text = Some(render_runtime_layout_text(module));
    }

    outputs
}

fn render_plan_text(request: &WasmBackendRequest, module: &WasmLirModule) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "Wasm Lowering/Emission Plan");
    let _ = writeln!(
        out,
        "  requested_exports={} imports={} functions={} static_data={}",
        request.export_policy.exported_functions.len(),
        module.imports.len(),
        module.functions.len(),
        module.static_data.len(),
    );
    let _ = writeln!(
        out,
        "  target_features: wasm_gc={} runtime_ownership={} bulk_memory={} multi_value={} reference_types={}",
        request.target_features.use_wasm_gc,
        request.target_features.enable_runtime_ownership,
        request.target_features.enable_bulk_memory,
        request.target_features.enable_multi_value,
        request.target_features.enable_reference_types,
    );
    let _ = writeln!(
        out,
        "  emit_options: emit_wasm_module={} validate={} name_section={} cfg_strategy={:?}",
        request.emit_options.emit_wasm_module,
        request.emit_options.validate_emitted_module,
        request.emit_options.emit_name_section,
        request.emit_options.cfg_lowering_strategy,
    );

    out
}

fn render_exports_text(module: &WasmLirModule) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Wasm exports");

    for export in &module.exports {
        let _ = writeln!(out, "  {} -> {:?}", export.export_name, export.kind);
    }

    out
}

fn render_runtime_layout_text(module: &WasmLirModule) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "Wasm runtime layout summary");
    let _ = writeln!(
        out,
        "  memory: initial_pages={} max_pages={:?} static_data_base={} heap_base_strategy={:?}",
        module.memory_plan.initial_pages,
        module.memory_plan.max_pages,
        module.memory_plan.static_data_base,
        module.memory_plan.heap_base_strategy,
    );
    let _ = writeln!(out, "  static segments={}", module.static_data.len());

    out
}
