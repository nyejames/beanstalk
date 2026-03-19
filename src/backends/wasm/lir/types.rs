//! Core LIR identifiers and ABI-facing primitive types.
//!
//! Phase-1 note:
//! these ids/types are stable scaffolding for HIR->LIR only; binary Wasm encoding
//! and full ABI adaptation are introduced in later phases.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WasmLirFunctionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WasmLirBlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WasmLirLocalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WasmImportId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WasmStaticDataId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WasmLirSignature {
    /// Ordered ABI parameter list.
    pub params: Vec<WasmAbiType>,
    /// Ordered ABI result list.
    pub results: Vec<WasmAbiType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WasmAbiType {
    I32,
    I64,
    #[allow(dead_code)] // todo
    F32,
    F64,
    Handle,
    /// Explicit "no value" marker used for unit-return functions.
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmLirLocal {
    pub id: WasmLirLocalId,
    /// Optional debug-only local name.
    pub name: Option<String>,
    /// ABI-level storage class.
    pub ty: WasmAbiType,
    /// Why this local exists (source param/user local/temp/runtime helper).
    pub role: WasmLocalRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmLocalRole {
    /// User-declared function parameter.
    Param,
    /// User-declared local variable.
    UserLocal,
    /// Compiler-introduced temporary.
    Temp,
    /// Runtime string buffer handle.
    BufferHandle,
    /// Runtime value/string handle.
    ValueHandle,
    /// Reserved for phase-2 static pointer materialization.
    #[allow(dead_code)] // todo
    StaticPtr,
    /// Reserved for phase-2 static length materialization.
    #[allow(dead_code)] // todo
    Length,
}
