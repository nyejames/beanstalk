//! File-writing scaffold logic.
//!
//! WHAT: Performs the actual directory and file creation for a new HTML project.
//! WHY: Separates IO side effects from command parsing and user prompting.
//!
//! Note: This module still carries legacy template content that will be
//! replaced by `templates.rs` in Phase 5.

use std::{fs, path::PathBuf};

/// Legacy scaffold write path.
///
/// Creates directories and writes a minimal `#config.bst` plus `dev/` and `release/`.
/// Returns the fully resolved project directory path.
///
/// Phase 3: now receives a pre-resolved path from `target.rs` instead of doing
/// its own validation.
pub(crate) fn write_legacy_scaffold(
    project_dir: PathBuf,
    project_name: &str,
) -> Result<PathBuf, String> {
    let name = if project_name.is_empty() {
        "Beanstalk Project"
    } else {
        project_name
    };

    fs::create_dir_all(&project_dir).map_err(|e| e.to_string())?;

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
    fs::write(project_dir.join("#config.bst"), config_content).map_err(|e| e.to_string())?;

    fs::create_dir(project_dir.join("src")).map_err(|e| e.to_string())?;
    fs::create_dir(project_dir.join("lib")).map_err(|e| e.to_string())?;
    fs::create_dir(project_dir.join("release")).map_err(|e| e.to_string())?;
    fs::create_dir(project_dir.join("dev")).map_err(|e| e.to_string())?;

    println!("Project created at: {:?}", &project_dir);

    Ok(project_dir)
}
