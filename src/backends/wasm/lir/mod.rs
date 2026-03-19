//! Wasm-oriented low-level IR used between HIR lowering and Wasm emission.

pub(crate) mod debug_dump;
pub(crate) mod function;
pub(crate) mod instructions;
pub(crate) mod linkage;
pub(crate) mod module;
pub(crate) mod runtime;
pub(crate) mod types;
