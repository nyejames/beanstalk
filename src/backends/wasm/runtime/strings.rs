//! Runtime string helper identifiers.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WasmRuntimeHelper {
    /// Raw runtime allocator helper returning an address in linear memory.
    Alloc,
    /// Create runtime string buffer.
    StringNewBuffer,
    /// Append interned static literal bytes.
    StringPushLiteral,
    /// Append runtime handle/string value.
    StringPushHandle,
    /// Materialize final runtime string from buffer.
    StringFinish,
    /// Reserved pointer/length helpers for host ABI bridging.
    StringPtr,
    /// Returns the byte length of a finalized runtime string handle.
    StringLen,
    /// Reserved release/drop helpers for ownership tuning.
    Release,
    /// Conditional drop hook used at `possible_drop` sites.
    DropIfOwned,
}
