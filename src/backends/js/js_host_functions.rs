//! Host-function mapping helpers for the JavaScript backend.
//!
//! This keeps backend-specific JS host bindings isolated from general HIR emission logic.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;

pub(crate) fn resolve_host_function_path(
    path: &InternedPath,
    string_table: &StringTable,
) -> Option<&'static str> {
    let name = path.name_str(string_table)?;

    match name {
        "io" => Some("__bs_io"),
        "__bs_collection_get" => Some("__bs_collection_get"),
        "__bs_collection_push" => Some("__bs_collection_push"),
        "__bs_collection_remove" => Some("__bs_collection_remove"),
        "__bs_collection_length" => Some("__bs_collection_length"),
        "__bs_error_with_location" => Some("__bs_error_with_location"),
        "__bs_error_push_trace" => Some("__bs_error_push_trace"),
        "__bs_error_bubble" => Some("__bs_error_bubble"),
        _ => None,
    }
}
