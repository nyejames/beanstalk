//! Builder-to-interpreter request contract.
//!
//! WHAT: defines the narrow backend input surface for interpreter lowering and optional execution.
//! WHY: keeping one explicit request shape prevents frontend/runtime coupling from spreading.

use crate::compiler_frontend::hir::ids::FunctionId;

#[derive(Debug, Clone, Default)]
pub(crate) struct InterpreterBackendRequest {
    /// Controls whether the backend only lowers to Exec IR or also tries to execute.
    pub execution_mode: InterpreterExecutionMode,
    /// Optional debug text outputs aligned with the compiler's existing debug workflow.
    pub debug_flags: InterpreterDebugFlags,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum InterpreterExecutionMode {
    /// Lower HIR to Exec IR only.
    #[default]
    LowerOnly,
    /// Lower and then execute a selected entrypoint.
    Execute {
        entry: InterpreterEntrypoint,
        policy: InterpreterExecutionPolicy,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InterpreterEntrypoint {
    /// Execute the module entry start function.
    Start,
    /// Reserved for future targeted execution (for example CTFE-selected functions).
    Function(FunctionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum InterpreterExecutionPolicy {
    /// Standard deterministic, headless runtime execution.
    #[default]
    NormalHeadless,
    /// Restricted execution intended for compile-time evaluation.
    Ctfe,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct InterpreterDebugFlags {
    /// Emit a high-level lowering summary.
    pub show_lowering_plan: bool,
    /// Emit a full textual Exec IR dump.
    pub show_exec_ir: bool,
    /// Emit function/local layout summaries.
    pub show_function_layouts: bool,
    /// Emit execution trace output once runtime dispatch exists.
    pub show_execution_trace: bool,
    /// Emit the final returned runtime value.
    pub show_final_value: bool,
}
