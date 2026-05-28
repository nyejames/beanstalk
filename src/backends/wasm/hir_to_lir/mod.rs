//! HIR -> Wasm LIR lowering modules.
//!
//! WHAT: consumes HIR plus borrow side-table facts and produces structured Wasm LIR.
//! WHY: binary emission stays in `emit/` so lowering remains about backend semantics, imports,
//! runtime helper contracts, ownership hooks, and static data layout.

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
