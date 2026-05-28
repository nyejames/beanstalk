//! Runtime-facing type contracts used by the Wasm LIR layer.
//!
//! WHAT: defines host imports, linear-memory layout constants, and runtime string contracts shared
//! by lowering and byte emission.
//! WHY: runtime policy belongs here rather than in frontend HIR or project-builder code.

pub(crate) mod imports;
pub(crate) mod memory;
pub(crate) mod strings;
