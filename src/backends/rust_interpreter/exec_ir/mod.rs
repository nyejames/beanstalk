//! Interpreter executable IR.
//!
//! WHAT: defines the runtime-oriented IR executed by the Rust interpreter.
//! WHY: the interpreter should lower from HIR into a semantic execution format, not reuse Wasm-shaped LIR.

mod instructions;
mod types;

pub(crate) use instructions::{ExecInstruction, ExecTerminator};
pub(crate) use types::{
    ExecBinaryOperator, ExecBlock, ExecBlockId, ExecConst, ExecConstId, ExecConstValue,
    ExecFunction, ExecFunctionFlags, ExecFunctionId, ExecLocal, ExecLocalId, ExecLocalRole,
    ExecModule, ExecProgram, ExecStorageType, ExecUnaryOperator, ExecValue,
};

#[cfg(test)]
pub(crate) use types::ExecModuleId;
