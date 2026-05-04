//! Result-call suffix parsing helpers.
//!
//! WHAT: parses fallback and named-handler suffixes for calls to functions with error return
//! slots.
//! WHY: result handling has its own control-flow rules and statement-body parsing, which would
//! otherwise make the general function-call parser too large and too coupled to function bodies.

mod fallback;
mod named_handler;
mod parser;
mod propagation;
mod termination;
mod validation;

// --------------------------
//  Re-exports
// --------------------------

pub(crate) use self::fallback::parse_result_fallback_values;
pub(crate) use self::propagation::is_result_propagation_boundary;
pub(crate) use parser::{
    ResultHandledCall, parse_named_result_handler_call, parse_result_handling_suffix_for_expression,
};

const FUNCTION_CALL_STAGE: &str = "Function Call Parsing";
const EXPRESSION_STAGE: &str = "Expression Parsing";

#[cfg(test)]
#[path = "../tests/result_handling_tests.rs"]
mod result_handling_tests;
