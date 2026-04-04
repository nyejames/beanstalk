//! Frontend-owned builtin language surfaces.
//!
//! WHAT: groups canonical builtin type manifests used by AST/HIR construction.
//! WHY: keeps language-owned builtin declarations out of parser orchestration modules.

/// Builtin receiver-method kinds recognized by parser and HIR lowering.
///
/// WHAT: identifies compiler-owned receiver methods that bypass normal user-method resolution.
/// WHY: receiver method parsing/lowering/backends need one shared enum to avoid drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMethodKind {
    CollectionGet,
    CollectionSet,
    CollectionPush,
    CollectionRemove,
    CollectionLength,
    ErrorWithLocation,
    ErrorPushTrace,
    ErrorBubble,
}

pub(crate) mod error_type;
pub(crate) mod expression_parsing;
