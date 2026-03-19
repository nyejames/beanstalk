//! Results produced by the phase-1 Wasm backend.
//!
//! In this phase there is no `.wasm` emission yet; the primary artifact is LIR.

use crate::backends::wasm::lir::module::WasmLirModule;

#[derive(Debug, Clone)]
pub(crate) struct WasmLirBackendResult {
    /// Canonical lowered module used as the phase-1 seam for phase-2 Wasm emission.
    pub lir_module: WasmLirModule,
    /// Optional debug text payloads controlled by [`WasmDebugFlags`].
    pub debug_outputs: WasmDebugOutputs,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmDebugOutputs {
    /// Human-readable lowering plan.
    pub plan_text: Option<String>,
    /// Textual LIR dump.
    pub lir_text: Option<String>,
    /// Export mapping summary.
    pub exports_text: Option<String>,
    /// Runtime memory/layout summary.
    pub runtime_layout_text: Option<String>,
}
