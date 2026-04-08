//! Linkage, import, and export metadata for the Wasm LIR module.

use crate::backends::wasm::lir::types::{WasmImportId, WasmLirFunctionId, WasmLirSignature};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmFunctionLinkage {
    /// Not externally visible.
    Internal,
    /// Synthetic exported wrapper function.
    ExportedWrapper,
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
}
