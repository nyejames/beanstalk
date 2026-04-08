//! HIR -> Exec IR lowering.
//!
//! WHAT: owns deterministic lowering from validated HIR into interpreter-facing Exec IR.
//! WHY: the interpreter should execute a backend-specific runtime IR rather than raw HIR or Wasm LIR.

mod context;
mod expressions;
mod functions;
mod module;
mod statements;
mod terminators;

pub(crate) use module::lower_hir_module_to_exec_program;
