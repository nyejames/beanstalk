//! Compiler-owned map builtin operation kinds and metadata.
//!
//! WHAT: identifies map operations that are language builtins, not user receiver methods.
//! WHY: parser and lowering stages need one explicit operation surface for map semantics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapBuiltinOp {
    Get,
    Contains,
    Set,
    Remove,
    Clear,
    Length,
}

/// Each map operation is characterized by four static attributes:
///
/// - `arity`: positional arguments required at the call site
/// - `requires_mutable_receiver`: borrow-checker mutability classification
/// - `is_fallible`: whether the operation may produce a runtime error
/// - `is_property`: whether the operation is parsed without call parentheses
impl MapBuiltinOp {
    /// Resolves a source member name to a compiler-owned map operation.
    pub(crate) fn from_source_name(name: &str) -> Option<Self> {
        match name {
            "get" => Some(MapBuiltinOp::Get),
            "contains" => Some(MapBuiltinOp::Contains),
            "set" => Some(MapBuiltinOp::Set),
            "remove" => Some(MapBuiltinOp::Remove),
            "clear" => Some(MapBuiltinOp::Clear),
            "length" => Some(MapBuiltinOp::Length),
            _ => None,
        }
    }

    /// Returns the source member name for this operation.
    pub fn source_name(self) -> &'static str {
        match self {
            MapBuiltinOp::Get => "get",
            MapBuiltinOp::Contains => "contains",
            MapBuiltinOp::Set => "set",
            MapBuiltinOp::Remove => "remove",
            MapBuiltinOp::Clear => "clear",
            MapBuiltinOp::Length => "length",
        }
    }

    /// Returns the expected positional argument count.
    pub fn arity(self) -> usize {
        match self {
            // Single-argument key accessors.
            MapBuiltinOp::Get | MapBuiltinOp::Contains | MapBuiltinOp::Remove => 1,
            // Two arguments: key and value.
            MapBuiltinOp::Set => 2,
            // No-argument operations.
            MapBuiltinOp::Clear | MapBuiltinOp::Length => 0,
        }
    }

    /// Whether the receiver must be accessed mutably.
    pub fn requires_mutable_receiver(self) -> bool {
        // Operations that modify map contents.
        matches!(
            self,
            MapBuiltinOp::Set | MapBuiltinOp::Remove | MapBuiltinOp::Clear
        )
    }

    /// Whether the operation is fallible and must be handled.
    pub fn is_fallible(self) -> bool {
        matches!(
            self,
            MapBuiltinOp::Get | MapBuiltinOp::Set | MapBuiltinOp::Remove
        )
    }

    /// Whether this operation is parsed as a property (no parentheses).
    pub fn is_property(self) -> bool {
        // Length is accessed field-like, without call parentheses.
        matches!(self, MapBuiltinOp::Length)
    }
}
