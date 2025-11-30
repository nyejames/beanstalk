// Native system call IO backend
//
// Provides direct access to native system calls and file operations.
// This will probably use Wasi to generate the correct glue code for Wasmer

use crate::compiler::compiler_errors::CompilerError;
use crate::runtime::io::io::{IoConfig, IoInterface};

pub struct NativeIoBackend {
    _config: IoConfig,
}

impl NativeIoBackend {
    pub fn new(config: IoConfig) -> Self {
        Self { _config: config }
    }
}

impl IoInterface for NativeIoBackend {
    fn print(&self, message: &str) -> Result<(), CompilerError> {
        println!("{}", message);
        Ok(())
    }

    fn read_input(&self) -> Result<String, CompilerError> {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| CompilerError::compiler_error(&format!("Failed to read input: {}", e)))?;
        Ok(line.trim().to_string())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), CompilerError> {
        std::fs::write(path, content).map_err(|e| {
            let error_msg: &'static str =
                Box::leak(format!("Failed to write file: {}", e).into_boxed_str());
            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to write to this file"
            } else if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the directory exists for this file path"
            } else {
                "Verify the file path is valid and the disk has space"
            };

            CompilerError::new_file_error(std::path::Path::new(path), error_msg, {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage,
                    "Runtime IO",
                );
                map.insert(
                    crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                    suggestion,
                );
                map
            })
        })
    }

    fn read_file(&self, path: &str) -> Result<String, CompilerError> {
        std::fs::read_to_string(path).map_err(|e| {
            let error_msg: &'static str =
                Box::leak(format!("Failed to read file: {}", e).into_boxed_str());
            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the file exists at the specified path"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to read this file"
            } else {
                "Verify the file is accessible and not corrupted"
            };

            CompilerError::new_file_error(std::path::Path::new(path), error_msg, {
                let mut map = std::collections::HashMap::new();
                map.insert(
                    crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage,
                    "Runtime IO",
                );
                map.insert(
                    crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
                    suggestion,
                );
                map
            })
        })
    }
}
