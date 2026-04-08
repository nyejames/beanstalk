//! Runtime-facing type contracts used by the Wasm LIR layer.
//!
//! Phase-1 scope:
//! this module only carries runtime contracts actively used by the current
//! HIR->LIR->Wasm emission path.

pub(crate) mod imports;
pub(crate) mod memory;
pub(crate) mod strings;
