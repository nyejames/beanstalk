//! Stable identifiers for external symbols across all compiler stages and backends.
//!
//! WHAT: defines the ID types that represent external functions, types, constants, and call
//! targets in HIR and backend lowering. These IDs must remain stable across frontend,
//! analysis, and backend passes.
//! WHY: backends and the borrow checker need to reference external symbols without
//! repeating package-scoped name resolution.

use crate::compiler_frontend::hir::ids::FunctionId;

pub const IO_FUNC_NAME: &str = "io";
pub const IO_TYPE_NAME: &str = "IO";
pub const COLLECTION_GET_HOST_NAME: &str = "__bs_collection_get";
pub const COLLECTION_PUSH_HOST_NAME: &str = "__bs_collection_push";
pub const COLLECTION_REMOVE_HOST_NAME: &str = "__bs_collection_remove";
pub const COLLECTION_LENGTH_HOST_NAME: &str = "__bs_collection_length";
pub const ERROR_WITH_LOCATION_HOST_NAME: &str = "__bs_error_with_location";
pub const ERROR_PUSH_TRACE_HOST_NAME: &str = "__bs_error_push_trace";
pub const ERROR_BUBBLE_HOST_NAME: &str = "__bs_error_bubble";

/// Stable identifier for an external function across all compiler stages and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalFunctionId {
    Io,
    CollectionGet,
    CollectionPush,
    CollectionRemove,
    CollectionLength,
    ErrorWithLocation,
    ErrorPushTrace,
    ErrorBubble,
    /// Synthetic functions registered by tests. Never emitted by production parsers.
    Synthetic(u32),
}

impl ExternalFunctionId {
    /// Human-readable name for diagnostics and HIR display.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Io => IO_FUNC_NAME,
            Self::CollectionGet => COLLECTION_GET_HOST_NAME,
            Self::CollectionPush => COLLECTION_PUSH_HOST_NAME,
            Self::CollectionRemove => COLLECTION_REMOVE_HOST_NAME,
            Self::CollectionLength => COLLECTION_LENGTH_HOST_NAME,
            Self::ErrorWithLocation => ERROR_WITH_LOCATION_HOST_NAME,
            Self::ErrorPushTrace => ERROR_PUSH_TRACE_HOST_NAME,
            Self::ErrorBubble => ERROR_BUBBLE_HOST_NAME,
            Self::Synthetic(_) => "<synthetic>",
        }
    }
}

/// Stable identifier for an external type across all compiler stages and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalTypeId(pub u32);

/// Stable identifier for an external constant across all compiler stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExternalConstantId(pub u32);

/// Unified identifier for an external symbol visible from a single file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalSymbolId {
    Function(ExternalFunctionId),
    Type(ExternalTypeId),
    Constant(ExternalConstantId),
}

/// Call target for a function invocation in HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallTarget {
    UserFunction(FunctionId),
    ExternalFunction(ExternalFunctionId),
}
