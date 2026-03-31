//! Statement and terminator instructions for Wasm LIR.
//!
//! Phase-1 note:
//! this instruction set is intentionally narrow and tuned for lowering validation,
//! not for direct binary encoding yet.

use crate::backends::wasm::lir::types::{
    WasmImportId, WasmLirBlockId, WasmLirFunctionId, WasmLirLocalId, WasmStaticDataId,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WasmLirStmt {
    /// Materialize immediate scalar constants.
    ConstI32 {
        dst: WasmLirLocalId,
        value: i32,
    },
    ConstI64 {
        dst: WasmLirLocalId,
        value: i64,
    },
    #[allow(dead_code)] // Planned: f32 literal lowering support.
    ConstF32 {
        dst: WasmLirLocalId,
        value: f32,
    },
    ConstF64 {
        dst: WasmLirLocalId,
        value: f64,
    },
    #[allow(dead_code)] // Planned: static-data pointer materialization.
    ConstStaticPtr {
        dst: WasmLirLocalId,
        data: WasmStaticDataId,
    },
    #[allow(dead_code)] // Planned: static-data byte-length materialization.
    ConstLength {
        dst: WasmLirLocalId,
        value: u32,
    },
    /// Explicit copy/move separation keeps ownership-optimization intent visible.
    Copy {
        dst: WasmLirLocalId,
        src: WasmLirLocalId,
    },
    Move {
        dst: WasmLirLocalId,
        src: WasmLirLocalId,
    },
    Call {
        /// Optional destination local for non-void calls.
        dst: Option<WasmLirLocalId>,
        callee: WasmCalleeRef,
        args: Vec<WasmLirLocalId>,
    },
    /// Runtime-template/string-building primitives.
    StringNewBuffer {
        dst: WasmLirLocalId,
    },
    StringPushLiteral {
        buffer: WasmLirLocalId,
        data: WasmStaticDataId,
    },
    StringPushHandle {
        buffer: WasmLirLocalId,
        handle: WasmLirLocalId,
    },
    StringFinish {
        dst: WasmLirLocalId,
        buffer: WasmLirLocalId,
    },
    DropIfOwned {
        value: WasmLirLocalId,
    },
    /// Reserved for future ownership tuning.
    #[allow(dead_code)] // Planned: explicit handle-retain operations for ownership tuning.
    RetainHandle {
        value: WasmLirLocalId,
    },
    IntEq {
        dst: WasmLirLocalId,
        lhs: WasmLirLocalId,
        rhs: WasmLirLocalId,
    },
    IntNe {
        dst: WasmLirLocalId,
        lhs: WasmLirLocalId,
        rhs: WasmLirLocalId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmCalleeRef {
    /// Direct call to another lowered function.
    Function(WasmLirFunctionId),
    /// Call through imported host function.
    Import(WasmImportId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WasmLirTerminator {
    /// Unconditional branch.
    Jump(WasmLirBlockId),
    /// Two-way conditional branch.
    Branch {
        condition: WasmLirLocalId,
        then_block: WasmLirBlockId,
        else_block: WasmLirBlockId,
    },
    /// Function return.
    Return { value: Option<WasmLirLocalId> },
    /// Fallback hard stop for unsupported/unreachable paths.
    Trap,
}
