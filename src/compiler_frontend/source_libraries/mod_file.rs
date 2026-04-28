//! Source-library facade file identity.
//!
//! WHAT: centralizes `#mod.bst` path checks and import-path detection.
//! WHY: `#mod.bst` is a boundary marker, not a normal implementation file. Keeping
//! the spelling and matching rules here prevents discovery, headers, sorting, and import
//! binding from drifting.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) const MOD_FILE_NAME: &str = "#mod.bst";
pub(crate) const MOD_FILE_IMPORT_COMPONENT: &str = "#mod";

pub(crate) fn file_name_is_mod_file(file_name: &str) -> bool {
    file_name == MOD_FILE_NAME
}

pub(crate) fn path_is_mod_file(path: &InternedPath, string_table: &StringTable) -> bool {
    path.name_str(string_table)
        .is_some_and(file_name_is_mod_file)
}

pub(crate) fn import_path_references_mod_file(
    path: &InternedPath,
    string_table: &StringTable,
) -> bool {
    path.as_components().iter().any(|component| {
        let segment = string_table.resolve(*component);
        segment == MOD_FILE_IMPORT_COMPONENT || segment == MOD_FILE_NAME
    })
}
