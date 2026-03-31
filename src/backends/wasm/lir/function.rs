//! Function and block definitions for Wasm LIR.

use crate::backends::wasm::lir::instructions::{WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::linkage::WasmFunctionLinkage;
use crate::backends::wasm::lir::types::{
    WasmLirBlockId, WasmLirFunctionId, WasmLirLocal, WasmLirSignature,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WasmLirFunction {
    pub id: WasmLirFunctionId,
    /// Debug-only name used in dumps/tests.
    pub debug_name: String,
    /// Semantic origin from frontend/build-system perspective.
    pub origin: WasmLirFunctionOrigin,
    pub signature: WasmLirSignature,
    pub locals: Vec<WasmLirLocal>,
    /// Block list in deterministic lowered order.
    pub blocks: Vec<WasmLirBlock>,
    /// Linkage classification (internal/export wrapper/etc).
    pub linkage: WasmFunctionLinkage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmLirFunctionOrigin {
    /// User-defined function.
    Normal,
    /// Entry-file implicit start function.
    EntryStart,
    /// Non-entry file implicit start function.
    FileStart,
    /// Runtime template fragment function.
    RuntimeTemplate,
    /// Synthetic wrapper created by export policy.
    ExportWrapper,
    /// Reserved for runtime helper functions (phase-2+).
    #[allow(dead_code)] // Planned: synthesized runtime helper function origins.
    RuntimeHelper,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WasmLirBlock {
    pub id: WasmLirBlockId,
    pub statements: Vec<WasmLirStmt>,
    pub terminator: WasmLirTerminator,
}
