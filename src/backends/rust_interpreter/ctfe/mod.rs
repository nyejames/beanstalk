//! Compile-time evaluation policy scaffolding.
//!
//! WHAT: reserves the interpreter-local CTFE policy and result surface.
//! WHY: CTFE should reuse the same engine with a stricter policy, not become a second interpreter.

use crate::backends::rust_interpreter::value::Value;

#[derive(Debug, Clone)]
pub(crate) struct CtfeEvaluator;

#[derive(Debug, Clone)]
pub(crate) struct CtfePolicy {
    pub max_steps: usize,
    pub max_call_depth: usize,
}

impl Default for CtfePolicy {
    fn default() -> Self {
        Self {
            max_steps: 10_000,
            max_call_depth: 128,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CtfeResult {
    pub value: Value,
}
