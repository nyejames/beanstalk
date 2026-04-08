//! Wasm backend entry points.
//!
//! Phase-1 exposes HIR -> LIR lowering.
//! Phase-2 adds deterministic LIR -> core Wasm emission.
//! The API remains crate-internal while backend integration stabilizes.
pub(crate) mod backend;
pub(crate) mod debug;
pub(crate) mod emit;
pub(crate) mod hir_to_lir;
pub(crate) mod lir;
pub(crate) mod request;
pub(crate) mod result;
pub(crate) mod runtime;

#[cfg(test)]
mod tests;
