//! Results produced by the Wasm backend.
//!
//! The backend always returns LIR for inspection/debugging. When emission is enabled it also
//! returns the encoded `.wasm` module bytes.

use crate::backends::wasm::lir::module::WasmLirModule;

#[derive(Debug, Clone)]
pub(crate) struct WasmLirBackendResult {
    /// Canonical lowered module used as the stable seam before byte emission.
    /// WHAT: this is always returned, even when emission is enabled.
    /// WHY: diagnostics, debug output, and HTML/Wasm integration still need direct LIR access.
    pub lir_module: WasmLirModule,
    /// Emitted Wasm bytes when module emission is enabled.
    pub wasm_bytes: Option<Vec<u8>>,
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
    /// Wasm section counts and order summary.
    pub wasm_sections_text: Option<String>,
    /// Type/function/global/data index maps.
    pub wasm_indices_text: Option<String>,
    /// Static-data offsets and heap-base layout summary.
    pub wasm_data_layout_text: Option<String>,
    /// Wasm validation diagnostics.
    pub wasm_validation_text: Option<String>,
}
