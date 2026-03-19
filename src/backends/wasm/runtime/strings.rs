//! Runtime string helper identifiers.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum WasmRuntimeHelper {
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
    StringLen,
    /// Reserved release/drop helpers for ownership tuning.
    Release,
    DropIfOwned,
}
