//! Runtime ABI constants and helper enums.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RuntimeHandleKind {
    /// Generic runtime-owned value handle.
    Value,
    /// String handle understood by runtime string helpers.
    String,
    /// Builder handle used while concatenating string fragments.
    Buffer,
}
