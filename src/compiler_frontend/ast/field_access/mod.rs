//! Postfix/member parsing coordinator.
//!
//! WHAT: drives chained postfix parsing and dispatches each member step to focused handlers.
//! WHY: field access, receiver methods, and compiler-owned builtin members evolve independently,
//! so the chain driver should stay thin while policy lives in dedicated modules.

mod builtin_call_args;
mod collection_builtin;
mod error_builtin;
mod field_member;
mod parse_chain;
mod receiver_access;
mod receiver_calls;

pub use parse_chain::parse_field_access;
pub(crate) use parse_chain::{parse_field_access_with_receiver_access, parse_postfix_chain};

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

// --------------------------
//  Types
// --------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiverAccessMode {
    Shared,
    Mutable,
}

/// Shared parse state for one postfix member step.
#[derive(Clone)]
pub(super) struct MemberStepContext<'a> {
    pub receiver_node: AstNode,
    pub receiver_type: &'a DataType,
    pub member_name: StringId,
    pub member_location: SourceLocation,
    pub receiver_access_mode: ReceiverAccessMode,
    pub scope_context: &'a ScopeContext,
}
