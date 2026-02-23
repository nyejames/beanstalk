use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;

pub(crate) fn resolve_host_function_path(
    path: &InternedPath,
    string_table: &StringTable,
) -> Option<&'static str> {
    let name = path.name_str(string_table)?;

    match name {
        "io" => Some("console.log"),
        _ => None,
    }
}
