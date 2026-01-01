//! Code Generation Module
//!
//! This module contains all code generation backends for the Beanstalk compiler.
//! Currently supports WASM as the primary target.

pub mod wasm;

// Re-export the main WASM encode function for convenience
pub use wasm::encode_wasm;