//! Builder-to-backend request contract for Wasm lowering and emission.
//!
//! WHAT: this is the only input seam from project-build orchestration into the Wasm backend.
//! WHY: keeping one explicit request object preserves stage separation and makes option growth
//! predictable as phase-3 HTML integration and richer Wasm features are added.

use crate::compiler_frontend::hir::ids::FunctionId;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmBackendRequest {
    /// Builder-selected export set and stable names.
    /// WHY: export naming must stay under build-system control.
    pub export_policy: WasmExportPolicy,
    /// Feature toggles for planned Wasm emission/runtime behavior.
    /// WHY: lets lowering plan for capabilities before binary emission exists.
    pub target_features: WasmTargetFeatures,
    /// Controls LIR -> Wasm emission behavior.
    /// WHY: phase-2 keeps emission policy explicit and testable.
    pub emit_options: WasmEmitOptions,
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
    /// Helper exports required by builder-side interop contracts.
    pub helper_exports: WasmHelperExportPolicy,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WasmHelperExportPolicy {
    /// Export memory as `memory`.
    pub export_memory: bool,
    /// Export runtime string-pointer helper as `bst_str_ptr`.
    pub export_str_ptr: bool,
    /// Export runtime string-length helper as `bst_str_len`.
    pub export_str_len: bool,
    /// Export runtime Vec handle constructor as `bst_vec_new`.
    pub export_vec_new: bool,
    /// Export runtime Vec append helper as `bst_vec_push`.
    pub export_vec_push: bool,
    /// Export runtime Vec length helper as `bst_vec_len`.
    pub export_vec_len: bool,
    /// Export runtime Vec element reader as `bst_vec_get`.
    pub export_vec_get: bool,
    /// Export runtime release helper as `bst_release`.
    pub export_release: bool,
}

/// Controls how CFG is mapped to Wasm structured control flow.
///
/// WHAT: this is an explicit strategy seam for function-body structuring.
/// WHY: phase-2 uses dispatcher-loop lowering today, but we want a stable request contract
/// so a richer block/loop structuring algorithm can be introduced without changing callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WasmCfgLoweringStrategy {
    /// Uses an internal dispatch local with `block + loop + br` to represent arbitrary CFG.
    /// This is the current phase-2 implementation.
    #[default]
    DispatcherLoop,
    /// Reserved for future direct structured lowering (if/else/loop region construction).
    #[allow(dead_code)] // Planned: direct structured CFG lowering.
    Structured,
}

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone)]
pub(crate) struct WasmEmitOptions {
    /// Enables LIR -> Wasm emission in phase-2 orchestration.
    pub emit_wasm_module: bool,
    /// Runs in-process Wasm validation after emission.
    pub validate_emitted_module: bool,
    /// Emits the custom name section for debug readability.
    pub emit_name_section: bool,
    /// Selects the CFG lowering strategy used by function emission.
    pub cfg_lowering_strategy: WasmCfgLoweringStrategy,
}

impl Default for WasmEmitOptions {
    fn default() -> Self {
        Self {
            emit_wasm_module: true,
            validate_emitted_module: true,
            emit_name_section: false,
            cfg_lowering_strategy: WasmCfgLoweringStrategy::DispatcherLoop,
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
    /// Emit Wasm section ordering and counts.
    pub show_wasm_sections: bool,
    /// Emit Wasm type/function/global/data index maps.
    pub show_wasm_indices: bool,
    /// Emit deterministic static-data and heap-base placement.
    pub show_wasm_data_layout: bool,
    /// Emit validation result details.
    pub show_wasm_validation: bool,
}
