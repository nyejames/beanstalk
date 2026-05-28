//! Root entry-file discovery for directory projects.
//!
//! WHAT: finds canonical `#*.bst` entry files below the configured entry root.
//! WHY: module inventory should consume a stable list of entry points without also owning
//! filesystem traversal details.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::source_libraries::mod_file::file_name_is_mod_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::{self, BEANSTALK_FILE_EXTENSION};

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

/// BFS scan for all files matching the `#*.bst` entry pattern under the entry root.
pub(super) fn discover_root_entry_files(
    source_root: &Path,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, CompilerError> {
    let mut discovered = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(source_root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let entries = fs::read_dir(&dir).map_err(|error| {
            CompilerError::file_error(
                &dir,
                format!("Failed to read directory while discovering modules: {error}"),
                string_table,
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CompilerError::file_error(
                    &dir,
                    format!("Failed to read directory entry while discovering modules: {error}"),
                    string_table,
                )
            })?;

            let path = entry.path();

            if path.is_dir() {
                queue.push_back(path);
                continue;
            }

            if !path_is_entry_file(&path) {
                continue;
            }

            discovered.push(fs::canonicalize(&path).map_err(|error| {
                CompilerError::file_error(
                    &path,
                    format!("Failed to canonicalize module entry path: {error}"),
                    string_table,
                )
            })?);
        }
    }

    discovered.sort();
    Ok(discovered)
}

fn path_is_entry_file(path: &Path) -> bool {
    if path.extension().and_then(|extension| extension.to_str()) != Some(BEANSTALK_FILE_EXTENSION) {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with('#')
        && file_name != settings::CONFIG_FILE_NAME
        && !file_name_is_mod_file(file_name)
}
