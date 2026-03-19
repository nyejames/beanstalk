use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::return_file_error;
use std::path::{Path, PathBuf};

pub fn is_valid_var_char(char: &char) -> bool {
    char.is_alphanumeric() || *char == '_'
}

// Checks the path and converts it to a PathBuf
// Resolves mixing unix and windows paths
pub fn check_if_valid_path(path: &str) -> Result<PathBuf, CompilerError> {
    // If it contains Unix-style slashes, convert them
    let path = if cfg!(windows) && path.contains('/') {
        // Replace forward slashes with backslashes
        &path.replace('/', "\\")
    } else {
        path
    };

    let path = Path::new(path);

    // Check if the path exists
    if !path.exists() {
        return_file_error!(path, "Path does not exist", {
            CompilationStage => "Build system path checking"
        });
    }

    Ok(path.to_path_buf())
}

// Traits for builtin types to help with parsing
pub trait NumericalParsing {
    fn is_non_newline_whitespace(&self) -> bool;
    fn is_bracket(&self) -> bool;
}
impl NumericalParsing for char {
    fn is_non_newline_whitespace(&self) -> bool {
        self.is_whitespace() && self != &'\n'
    }
    fn is_bracket(&self) -> bool {
        matches!(self, '(' | ')' | '{' | '}' | '[' | ']')
    }
}
