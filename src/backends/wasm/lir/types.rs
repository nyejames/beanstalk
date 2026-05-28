//! Core LIR identifiers and ABI-facing primitive types.
//!
//! These ids and types are the stable backend-local contract between HIR lowering and byte
//! emission. A few variants are ahead of production lowering but are already exercised by emitter
//! tests so future Wasm work can extend lowering without changing the ABI vocabulary.

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
    #[allow(dead_code)] // Wasm roadmap: f32 ABI lanes are emitter-tested before HIR maps to them.
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
    /// Reserved for static-data pointer materialization.
    #[allow(dead_code)] // Wasm roadmap: static-data pointer locals are emitter-tested first.
    StaticPtr,
    /// Reserved for static-data length materialization.
    #[allow(dead_code)] // Wasm roadmap: static-data length locals are emitter-tested first.
    Length,
}
