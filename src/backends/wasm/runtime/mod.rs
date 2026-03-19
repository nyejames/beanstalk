//! Runtime-facing type contracts used by the Wasm LIR layer.
//!
//! Phase-1 note:
//! these are planning types only; concrete runtime implementation/wire format
//! is phase-2/3 scope.

pub(crate) mod abi;
pub(crate) mod imports;
pub(crate) mod layout;
pub(crate) mod memory;
pub(crate) mod strings;
