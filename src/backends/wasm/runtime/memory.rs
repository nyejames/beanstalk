//! Runtime memory planning structures.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeapBaseStrategy {
    /// Heap starts after aligned static-data end.
    StaticDataEndAligned,
    /// Reserved for explicit mutable-global heap base in later phases.
    #[allow(dead_code)] // todo
    ExplicitGlobal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmMemoryPlan {
    /// Initial memory page count (64KiB pages).
    pub initial_pages: u32,
    /// Optional max page cap.
    pub max_pages: Option<u32>,
    /// Base address for static segment placement.
    pub static_data_base: u32,
    /// Strategy used to compute runtime heap base.
    pub heap_base_strategy: HeapBaseStrategy,
}

impl Default for WasmMemoryPlan {
    fn default() -> Self {
        Self {
            initial_pages: 1,
            max_pages: None,
            static_data_base: 0,
            heap_base_strategy: HeapBaseStrategy::StaticDataEndAligned,
        }
    }
}
