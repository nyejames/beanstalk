// Native system call IO backend
//
// Provides direct access to native system calls and file operations.
// This will probably use Wasi to generate the correct glue code for Wasmer

use crate::compiler::compiler_errors::CompileError;
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
    fn print(&self, message: &str) -> Result<(), CompileError> {
        println!("{}", message);
        Ok(())
    }

    fn read_input(&self) -> Result<String, CompileError> {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin
            .lock()
            .read_line(&mut line)
            .map_err(|e| CompileError::compiler_error(&format!("Failed to read input: {}", e)))?;
        Ok(line.trim().to_string())
    }

    fn write_file(&self, path: &str, content: &str) -> Result<(), CompileError> {
        std::fs::write(path, content).map_err(|e| {
            CompileError::file_error(
                std::path::Path::new(path),
                &format!("Failed to write file: {}", e),
            )
        })
    }

    fn read_file(&self, path: &str) -> Result<String, CompileError> {
        std::fs::read_to_string(path).map_err(|e| {
            CompileError::file_error(
                std::path::Path::new(path),
                &format!("Failed to read file: {}", e),
            )
        })
    }
}
