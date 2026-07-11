//! Generic module-root and project-config filename identity.
//!
//! WHAT: classifies canonical root/config filenames and their extensionless import components.
//! WHY: Stage 0, header import validation, and diagnostic rendering must share one filename
//!      policy while existing facade discovery keeps its temporary `#mod.bst` behavior.

use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::CONFIG_FILE_NAME;

/// Temporary cosmetic root name used by the current facade role.
pub(crate) const MOD_FILE_NAME: &str = "#mod.bst";

/// Whether a filesystem filename is a non-config Beanstalk module root.
pub(crate) fn file_name_is_hash_root_file(file_name: &str) -> bool {
    let Some(root_name) = file_name.strip_prefix('#') else {
        return false;
    };
    let Some(root_name) = root_name.strip_suffix(".bst") else {
        return false;
    };

    !root_name.is_empty()
}

/// Whether a filesystem filename is the canonical project configuration file.
pub(crate) fn file_name_is_config_file(file_name: &str) -> bool {
    file_name == CONFIG_FILE_NAME
}

/// Whether a root filename has the temporary current facade name.
pub(crate) fn file_name_is_mod_file(file_name: &str) -> bool {
    file_name == MOD_FILE_NAME
}

/// Whether an interned source path has the temporary current facade name.
pub(crate) fn path_is_mod_file(path: &InternedPath, string_table: &StringTable) -> bool {
    path.name_str(string_table)
        .is_some_and(file_name_is_mod_file)
}

/// Whether an extensionless import component identifies a hash-root file.
pub(crate) fn import_component_is_hash_root_file(component: &str) -> bool {
    file_name_is_hash_root_file(component)
        || (component.starts_with('#') && !component.contains('.') && component.len() > 1)
}

/// Return the canonical root filename represented by an import component.
pub(crate) fn hash_root_file_name_from_import_component(component: &str) -> Option<String> {
    if file_name_is_hash_root_file(component) {
        return Some(component.to_owned());
    }

    import_component_is_hash_root_file(component).then(|| format!("{component}.bst"))
}

/// Whether an import component identifies the canonical project config file.
pub(crate) fn import_component_is_config_file(component: &str) -> bool {
    component == "config" || file_name_is_config_file(component)
}

/// Whether a direct import's source component is a hash-root file.
pub(crate) fn import_path_references_hash_root_file(
    path: &InternedPath,
    from_grouped_import: bool,
    string_table: &StringTable,
) -> bool {
    import_source_component(path, from_grouped_import, string_table)
        .is_some_and(import_component_is_hash_root_file)
}

/// Whether a direct import's source component is the canonical project config file.
pub(crate) fn import_path_references_config_file(
    path: &InternedPath,
    from_grouped_import: bool,
    string_table: &StringTable,
) -> bool {
    import_source_component(path, from_grouped_import, string_table)
        .is_some_and(import_component_is_config_file)
}

fn import_source_component<'a>(
    path: &'a InternedPath,
    from_grouped_import: bool,
    string_table: &'a StringTable,
) -> Option<&'a str> {
    let source_component_offset = if from_grouped_import { 2 } else { 1 };
    path.len()
        .checked_sub(source_component_offset)
        .and_then(|index| path.as_components().get(index))
        .map(|component| string_table.resolve(*component))
}
