//! Wasm backend phase-1 entry points.
//!
//! This module defines a Wasm-oriented LIR and a HIR -> LIR lowering pipeline.
//! It intentionally does not emit binary Wasm bytes in this phase.
//! The API remains crate-internal while phase-1 scaffolding stabilizes.
#![allow(dead_code)]

pub(crate) mod backend;
pub(crate) mod debug;
pub(crate) mod hir_to_lir;
pub(crate) mod lir;
pub(crate) mod request;
pub(crate) mod result;
pub(crate) mod runtime;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub(crate) use backend::lower_hir_to_wasm_lir;
#[allow(unused_imports)]
pub(crate) use request::{
    WasmBackendRequest, WasmDebugFlags, WasmExportPolicy, WasmTargetFeatures,
};
#[allow(unused_imports)]
pub(crate) use result::{WasmDebugOutputs, WasmLirBackendResult};
