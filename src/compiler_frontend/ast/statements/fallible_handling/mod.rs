//! Fallible suffix parsing helpers.
//!
//! WHAT: parses postfix propagation plus `catch` recovery handler suffixes for fallible
//! expressions and calls.
//! WHY: fallible handling has its own control-flow rules and statement-body parsing, which would
//! otherwise make the general expression parser too large and too coupled to function bodies.

mod catch_handler;
mod parser;
mod success_types;
mod validation;

// --------------------------
//  Re-exports
// --------------------------

pub(crate) use parser::{
    FallibleCallSite, FallibleHostCallSite, HandledFallibleCall, HandledFallibleHostCall,
    fallible_catch_allowed_in_context, parse_fallible_handling_suffix_for_call,
    parse_fallible_handling_suffix_for_expression, parse_fallible_handling_suffix_for_host_call,
};

const FUNCTION_CALL_STAGE: &str = "Function Call Parsing";
const EXPRESSION_STAGE: &str = "Expression Parsing";

#[cfg(test)]
#[path = "../tests/fallible_handling_tests.rs"]
mod fallible_handling_tests;
