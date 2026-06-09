//! Raw file I/O for Beanstalk source files.
//!
//! Reads source file content from disk with structured error diagnostics.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_file_error;

use std::fs;
use std::path::Path;

// -------------------------
//  Source Extraction
// -------------------------

/// Reads the contents of a source file from disk.
///
/// WHAT: performs UTF-8 file read with structured `CompilerError` diagnostics for common
///       failure modes (not found, permission denied).
/// WHY: every source file entering the compiler pipeline goes through this single boundary
///      so I/O failures are reported uniformly instead of leaking `std::io::Error`.
pub fn extract_source_code(
    file_path: &Path,
    string_table: &mut StringTable,
) -> Result<String, CompilerError> {
    match fs::read_to_string(file_path) {
        Ok(content) => Ok(content),

        Err(e) => {
            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the file exists at the specified path"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to read this file"
            } else {
                "Verify the file is accessible and not corrupted"
            };

            return_file_error!(
                string_table,
                &file_path,
                format!("Error reading file when adding new bst files to parse: {:?}", e),
                {
                    CompilationStage => String::from("File System"),
                    PrimarySuggestion => String::from(suggestion),
                }
            )
        }
    }
}
