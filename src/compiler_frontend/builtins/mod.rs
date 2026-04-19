//! Frontend-owned builtin language surfaces.
//!
//! WHAT: groups canonical builtin type manifests used by AST/HIR construction.
//! WHY: keeps language-owned builtin declarations out of parser orchestration modules.

/// Compiler-owned collection builtin operation kinds.
///
/// WHAT: identifies collection operations that are language builtins, not user receiver methods.
/// WHY: parser and lowering stages need one explicit operation surface for collection semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionBuiltinOp {
    Get,
    Set,
    Push,
    Remove,
    Length,
}

/// Builtin receiver-method kinds recognized by parser and HIR lowering.
///
/// WHAT: identifies compiler-owned receiver methods that keep method-call surface syntax.
/// WHY: error helper method parsing/lowering/backends need one shared enum to avoid drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMethodKind {
    WithLocation,
    PushTrace,
    Bubble,
}

pub(crate) mod error_type;
pub(crate) mod expression_parsing;
