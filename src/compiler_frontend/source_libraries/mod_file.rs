//! Special module file identity.
//!
//! WHAT: centralizes `#mod.bst` path checks plus direct-import detection for special files.
//! WHY: `#mod.bst`, `#page.bst`, and `#config.bst` are boundary/build files, not normal
//! import surfaces. Keeping the spellings and matching rules here prevents discovery,
//! headers, sorting, and import binding from drifting.

use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) const MOD_FILE_NAME: &str = "#mod.bst";
pub(crate) const MOD_FILE_IMPORT_COMPONENT: &str = "#mod";

pub(crate) const PAGE_FILE_NAME: &str = "#page.bst";
pub(crate) const PAGE_FILE_IMPORT_COMPONENT: &str = "#page";

pub(crate) const CONFIG_FILE_NAME: &str = "#config.bst";
pub(crate) const CONFIG_FILE_IMPORT_COMPONENT: &str = "#config";

pub(crate) fn file_name_is_mod_file(file_name: &str) -> bool {
    file_name == MOD_FILE_NAME
}

pub(crate) fn path_is_mod_file(path: &InternedPath, string_table: &StringTable) -> bool {
    path.name_str(string_table)
        .is_some_and(file_name_is_mod_file)
}

/// Whether an import path directly references a special file (`#mod`, `#page`, or `#config`).
///
/// WHAT: rejects imports that bypass module boundaries by targeting special files directly.
/// WHY: special files are not normal import surfaces; their declarations are exposed only
/// through module facades or internal file resolution, not through direct import paths.
pub(crate) fn import_path_references_special_file(
    path: &InternedPath,
    string_table: &StringTable,
) -> bool {
    path.as_components().iter().any(|component| {
        let segment = string_table.resolve(*component);
        segment == MOD_FILE_IMPORT_COMPONENT
            || segment == MOD_FILE_NAME
            || segment == PAGE_FILE_IMPORT_COMPONENT
            || segment == PAGE_FILE_NAME
            || segment == CONFIG_FILE_IMPORT_COMPONENT
            || segment == CONFIG_FILE_NAME
    })
}
