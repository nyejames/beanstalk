//! File-writing scaffold logic.
//!
//! WHAT: Performs the actual directory and file creation for a new HTML project.
//! WHY: Separates IO side effects from command parsing and user prompting.
//!
//! Note: This module still carries legacy path-handling behaviour that will be
//! replaced by `target.rs` in Phase 3 and full template rendering in Phase 5.

use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::{env, fs, path::PathBuf};

/// Legacy scaffold write path.
///
/// Creates directories and writes a minimal `#config.bst` plus `dev/` and `release/`.
/// Returns the fully resolved project directory path.
pub(crate) fn write_legacy_scaffold(
    user_project_path: String,
    project_name: &str,
) -> Result<PathBuf, String> {
    let current_dir = env::current_dir().map_err(|e| e.to_string())?;

    let mut string_table = StringTable::new();
    let valid_path = match check_if_valid_path(&user_project_path, &mut string_table) {
        Ok(path) => path,
        Err(e) => return Err(e.msg),
    };

    let name = if project_name.is_empty() {
        "Beanstalk Project"
    } else {
        project_name
    };

    let full_path = current_dir.join(valid_path).join(project_name);

    fs::create_dir_all(&full_path).map_err(|e| e.to_string())?;

    let config_content = format!(
        "#project_name = \"{name}\"\n\
         #entry_root = \"src\"\n\
         #dev_folder = \"dev\"\n\
         #output_folder = \"release\"\n\
         #page_url_style = \"trailing_slash\"\n\
         #redirect_index_html = true\n\
         #name = \"html_project\"\n\
         #version = \"0.1.0\"\n\
         #author = \"\"\n\
         #license = \"MIT\"\n"
    );
    fs::write(full_path.join("#config.bst"), config_content).map_err(|e| e.to_string())?;

    fs::create_dir(full_path.join("../..")).map_err(|e| e.to_string())?;
    fs::create_dir(full_path.join("release")).map_err(|e| e.to_string())?;
    fs::create_dir(full_path.join("dev")).map_err(|e| e.to_string())?;

    println!("Project created at: {:?}", &full_path);

    Ok(full_path)
}
