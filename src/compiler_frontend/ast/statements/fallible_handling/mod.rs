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

use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

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

/// Returns true when the current token starts a fallible-handling suffix (`!`, `catch`,
/// or a symbol followed by `!`).
///
/// WHAT: keeps suffix detection shared by free calls, receiver calls, collection builtins,
///       and generic expression result handling.
/// WHY: these entrypoints construct fallible carriers in different parser modules, but the
///      syntax that consumes those carriers must stay identical.
pub(crate) fn token_stream_starts_fallible_handling_suffix(token_stream: &FileTokens) -> bool {
    token_stream.current_token_kind() == &TokenKind::Bang
        || token_stream.current_token_kind() == &TokenKind::Catch
        || (matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang))
}

#[cfg(test)]
#[path = "../tests/fallible_handling_tests.rs"]
mod fallible_handling_tests;
