//! User-provided build path validation.
//!
//! WHAT: validates filesystem paths supplied to project/build commands.
//! WHY: this is build-system input handling, not compiler frontend semantics, so
//! file diagnostics should stay at the project orchestration boundary.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_file_error;
use std::path::{Path, PathBuf};

pub(crate) fn check_if_valid_path(
    path: &str,
    string_table: &mut StringTable,
) -> Result<PathBuf, CompilerError> {
    // If it contains Unix-style slashes, convert them on Windows before existence checks.
    let path = if cfg!(windows) && path.contains('/') {
        &path.replace('/', "\\")
    } else {
        path
    };

    let path = Path::new(path);

    if !path.exists() {
        return_file_error!(string_table, path, "Path does not exist", {
            CompilationStage => String::from("Build system path checking")
        });
    }

    Ok(path.to_path_buf())
}
