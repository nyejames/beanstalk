//! HIR -> Exec IR lowering.
//!
//! WHAT: owns deterministic lowering from validated HIR into interpreter-facing Exec IR.
//! WHY: the interpreter should execute a backend-specific runtime IR rather than raw HIR or Wasm LIR.

pub(crate) mod context;
pub(crate) mod expressions;
mod functions;
pub(crate) mod materialize;
mod module;
pub(crate) mod operators;
mod statements;
pub(crate) mod terminators;

pub(crate) use module::lower_hir_module_to_exec_program;
