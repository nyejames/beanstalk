//! Linkage, import, and export metadata for the Wasm LIR module.

use crate::backends::wasm::lir::types::{WasmImportId, WasmLirFunctionId, WasmLirSignature};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmFunctionLinkage {
    /// Not externally visible.
    Internal,
    /// Synthetic exported wrapper function.
    ExportedWrapper,
    /// Reserved: needed once helpers get explicit linkage metadata instead of side-table tracking.
    #[allow(dead_code)] // needed for helper linkage classification
    RuntimeHelper,
    /// Reserved: needed once imported host thunks carry linkage classification.
    #[allow(dead_code)] // needed for import linkage classification
    ImportedHost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmImport {
    pub id: WasmImportId,
    pub module_name: String,
    pub item_name: String,
    pub kind: WasmImportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WasmImportKind {
    /// Function import with explicit signature.
    Function(WasmLirSignature),
    /// Reserved for future memory imports.
    #[allow(dead_code)] // todo
    Memory(WasmMemoryImport),
    /// Reserved for future global imports.
    #[allow(dead_code)] // todo
    Global(WasmGlobalImport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmMemoryImport {
    pub min_pages: u32,
    pub max_pages: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmGlobalImport {
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmExport {
    pub export_name: String,
    pub kind: WasmExportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmExportKind {
    /// Exported function symbol.
    Function(WasmLirFunctionId),
    /// Reserved for memory export integration.
    #[allow(dead_code)] // todo
    Memory,
}
