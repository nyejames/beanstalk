//! Builder-to-backend request contract for phase-1 Wasm lowering.
//!
//! This is intentionally internal and incomplete for early backend bring-up.
//! The shape is designed so phase-2/3 can extend it without breaking callers.

use crate::compiler_frontend::hir::hir_nodes::FunctionId;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmBackendRequest {
    /// Builder-selected export set and stable names.
    /// WHY: export naming must stay under build-system control.
    pub export_policy: WasmExportPolicy,
    /// Feature toggles for planned Wasm emission/runtime behavior.
    /// WHY: lets lowering plan for capabilities before binary emission exists.
    pub target_features: WasmTargetFeatures,
    /// Optional debug dumps aligned with existing compiler debug workflow.
    pub debug_flags: WasmDebugFlags,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmExportPolicy {
    /// Function IDs that should become externally visible exports.
    /// Order is preserved to keep debug output deterministic.
    pub exported_functions: Vec<FunctionId>,
    /// Stable export names keyed by source function id.
    /// Required even in phase-1 to lock down external API contracts early.
    pub export_names: FxHashMap<FunctionId, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct WasmTargetFeatures {
    /// Placeholder for future GC proposal-specific codegen branches.
    pub use_wasm_gc: bool,
    /// Enables ownership-aware runtime scaffolding planning.
    /// Phase-1 remains conservative/GC-first either way.
    pub enable_runtime_ownership: bool,
    /// Reserved for phase-2 memory/data-segment emit behavior.
    pub enable_bulk_memory: bool,
    /// Reserved for phase-2 function signature and call lowering.
    pub enable_multi_value: bool,
    /// Reserved for reference-type-capable host/runtime ABIs.
    pub enable_reference_types: bool,
}

impl Default for WasmTargetFeatures {
    fn default() -> Self {
        Self {
            use_wasm_gc: false,
            enable_runtime_ownership: false,
            enable_bulk_memory: false,
            enable_multi_value: false,
            enable_reference_types: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmDebugFlags {
    /// Emit high-level lowering plan summary text.
    pub show_wasm_plan: bool,
    /// Emit full textual LIR dump.
    pub show_lir: bool,
    /// Emit requested export -> lowered symbol mapping.
    pub show_wasm_exports: bool,
    /// Emit runtime memory/layout summary.
    pub show_wasm_runtime_layout: bool,
}
