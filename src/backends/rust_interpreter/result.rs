//! Results produced by the Rust interpreter backend.
//!
//! WHAT: packages lowered Exec IR, optional execution output, and optional debug text.
//! WHY: callers need one stable backend result shape while the runtime grows in phases.

use crate::backends::rust_interpreter::exec_ir::ExecProgram;
use crate::backends::rust_interpreter::request::{
    InterpreterEntrypoint, InterpreterExecutionPolicy,
};
use crate::backends::rust_interpreter::value::Value;

#[derive(Debug, Clone)]
pub(crate) struct InterpreterBackendResult {
    /// Canonical lowered program used by the interpreter runtime.
    pub exec_program: ExecProgram,
    /// Present only when execution was requested and completed successfully.
    pub execution_result: Option<InterpreterExecutionResult>,
    /// Metadata describing the execution mode that produced `execution_result`.
    pub execution_metadata: Option<InterpreterExecutionMetadata>,
    /// Human-readable debug payloads controlled by request flags.
    pub debug_outputs: InterpreterDebugOutputs,
}

#[derive(Debug, Clone)]
pub(crate) struct InterpreterExecutionResult {
    /// Final value returned by the selected entrypoint.
    pub returned_value: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct InterpreterExecutionMetadata {
    pub entry: InterpreterEntrypoint,
    pub policy: InterpreterExecutionPolicy,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InterpreterDebugOutputs {
    /// Human-readable lowering plan summary.
    pub plan_text: Option<String>,
    /// Full textual Exec IR dump.
    pub exec_ir_text: Option<String>,
    /// Function and local-slot layout summary.
    pub function_layouts_text: Option<String>,
    /// Runtime trace text.
    pub execution_trace_text: Option<String>,
    /// Final runtime value text.
    pub final_value_text: Option<String>,
}
