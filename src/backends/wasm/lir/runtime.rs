//! Runtime-facing LIR metadata.
//!
//! Phase-1 note:
//! this summary is planning metadata for debug/inspection; final runtime section
//! construction happens in later backend phases.

use crate::backends::wasm::runtime::memory::{HeapBaseStrategy, WasmMemoryPlan};

#[allow(dead_code)] // Planned: runtime layout debug summaries for phase-2+ diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmRuntimeLayoutSummary {
    pub memory_plan: WasmMemoryPlan,
    pub heap_base_strategy: HeapBaseStrategy,
}

impl From<&WasmMemoryPlan> for WasmRuntimeLayoutSummary {
    fn from(memory_plan: &WasmMemoryPlan) -> Self {
        Self {
            memory_plan: memory_plan.clone(),
            heap_base_strategy: memory_plan.heap_base_strategy,
        }
    }
}
