//! Shared frontend utility helpers.
//!
//! These are small cross-cutting helpers that predate some newer subsystem boundaries and are
//! still reused in parsing and path-validation code.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::return_file_error;
use std::path::{Path, PathBuf};
use std::fmt::Write;

pub fn is_valid_var_char(char: &char) -> bool {
    char.is_alphanumeric() || *char == '_'
}

// WHAT: validate a user-provided filesystem path and normalize separators on Windows.
// WHY: build-system path settings are user-facing input, so they must produce structured file
//      diagnostics instead of leaking platform-specific path quirks downstream.
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
            CompilationStage => String::from("Build system path checking")
        });
    }

    Ok(path.to_path_buf())
}

// For Windows compatability
pub fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();

    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

// Turns any path to a local file into the correct format for a URL
// Needed particularly for windows compatability
pub fn file_url_from_path(path: &Path, encoded: bool) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    let path_string = path.to_string_lossy();

    #[cfg(windows)]
    let path_string = path_string.strip_prefix(r"\\?\").unwrap_or(&path_string);

    let mut path_string = path_string.replace('\\', "/");
    
    if !path_string.starts_with('/') {
        path_string = format!("/{path_string}");
    }

    if encoded {
        path_string = percent_encode_file_url_path(&path_string)
    }

    // Browsers expect file links to be URL-safe, so encode the filesystem path before embedding it.
    format!("file://{}", path_string)
}

fn percent_encode_file_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());

    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                encoded.push(byte as char)
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }

    encoded
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
