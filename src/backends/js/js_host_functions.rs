//! Host-function mapping helpers for the JavaScript backend.
//!
//! This keeps backend-specific JS host bindings isolated from general HIR emission logic.

use crate::compiler_frontend::external_packages::{
    COLLECTION_GET_HOST_NAME, COLLECTION_LENGTH_HOST_NAME, COLLECTION_PUSH_HOST_NAME,
    COLLECTION_REMOVE_HOST_NAME, ERROR_BUBBLE_HOST_NAME, ERROR_PUSH_TRACE_HOST_NAME,
    ERROR_WITH_LOCATION_HOST_NAME, IO_FUNC_NAME,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn resolve_host_function_path(
    path: &InternedPath,
    string_table: &StringTable,
) -> Option<&'static str> {
    let name = path.name_str(string_table)?;

    match name {
        IO_FUNC_NAME => Some("__bs_io"),
        COLLECTION_GET_HOST_NAME => Some(COLLECTION_GET_HOST_NAME),
        COLLECTION_PUSH_HOST_NAME => Some(COLLECTION_PUSH_HOST_NAME),
        COLLECTION_REMOVE_HOST_NAME => Some(COLLECTION_REMOVE_HOST_NAME),
        COLLECTION_LENGTH_HOST_NAME => Some(COLLECTION_LENGTH_HOST_NAME),
        ERROR_WITH_LOCATION_HOST_NAME => Some(ERROR_WITH_LOCATION_HOST_NAME),
        ERROR_PUSH_TRACE_HOST_NAME => Some(ERROR_PUSH_TRACE_HOST_NAME),
        ERROR_BUBBLE_HOST_NAME => Some(ERROR_BUBBLE_HOST_NAME),
        _ => None,
    }
}
