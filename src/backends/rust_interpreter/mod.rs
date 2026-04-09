#![allow(dead_code)] // While this backend is unused - remove once wired in

//! Rust interpreter backend entry points.
//!
//! WHAT: owns the compiler-internal interpreter pipeline used for Exec IR lowering,
//! headless execution, and future CTFE integration.
//! WHY: the interpreter needs a backend-local seam that stays separate from Wasm lowering
//! while remaining reusable by future CTFE and later embedding layers.

pub(crate) mod backend;
pub(crate) mod ctfe;
pub(crate) mod debug;
pub(crate) mod error;
pub(crate) mod exec_ir;
pub(crate) mod heap;
pub(crate) mod lowering;
pub(crate) mod request;
pub(crate) mod result;
pub(crate) mod runtime;
pub(crate) mod value;

#[cfg(test)]
mod tests;
