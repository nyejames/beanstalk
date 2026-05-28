//! Wasm-oriented low-level IR used between HIR lowering and Wasm emission.
//!
//! WHAT: models backend-local functions, blocks, locals, static data, imports, exports, and
//! Wasm-shaped instructions after frontend semantics and borrow side tables have been consumed.
//! WHY: keeping this IR explicit lets the experimental Wasm backend validate and debug lowering
//! separately from binary encoding.

pub(crate) mod debug_dump;
pub(crate) mod function;
pub(crate) mod instructions;
pub(crate) mod linkage;
pub(crate) mod module;
pub(crate) mod types;
