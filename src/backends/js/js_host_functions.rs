//! Host-function mapping helpers for the JavaScript backend.
//!
//! This keeps backend-specific JS host bindings isolated from general HIR emission logic.

use crate::compiler_frontend::external_packages::ExternalFunctionId;

pub(crate) fn resolve_host_function_id(id: ExternalFunctionId) -> Option<&'static str> {
    match id {
        ExternalFunctionId::Io => Some("__bs_io"),
        ExternalFunctionId::CollectionGet => Some("__bs_collection_get"),
        ExternalFunctionId::CollectionPush => Some("__bs_collection_push"),
        ExternalFunctionId::CollectionRemove => Some("__bs_collection_remove"),
        ExternalFunctionId::CollectionLength => Some("__bs_collection_length"),
        ExternalFunctionId::ErrorWithLocation => Some("__bs_error_with_location"),
        ExternalFunctionId::ErrorPushTrace => Some("__bs_error_push_trace"),
        ExternalFunctionId::ErrorBubble => Some("__bs_error_bubble"),
        ExternalFunctionId::Synthetic(_) => None,
    }
}
