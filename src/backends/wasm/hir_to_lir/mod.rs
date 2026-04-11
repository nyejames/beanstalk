//! HIR -> Wasm LIR lowering modules.
//!
//! Phase-1 note:
//! this layer validates lowering contracts and produces structured LIR only.
//! Binary Wasm emission is intentionally out of scope here.

pub(crate) mod context;
pub(crate) mod exports;
pub(crate) mod expr;
pub(crate) mod function;
pub(crate) mod imports;
pub(crate) mod module;
pub(crate) mod ownership;
pub(crate) mod static_data;
pub(crate) mod stmt;
pub(crate) mod terminator;
