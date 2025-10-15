// Custom IO backend for embedded scenarios
//
// Allows custom IO implementations to be plugged in for specific use cases.
// This might use Wasi for native and custom bindings for web during the build process

use crate::compiler::compiler_errors::CompileError;
use crate::runtime::io::io::{IoConfig, IoInterface};

pub struct CustomIoBackend {
    _config: IoConfig,
}

impl CustomIoBackend {
    pub fn new(config: IoConfig) -> Self {
        Self { _config: config }
    }
}

impl IoInterface for CustomIoBackend {
    fn print(&self, message: &str) -> Result<(), CompileError> {
        // Custom print implementation - could be redirected to logs, GUI, etc.
        eprintln!("Custom IO: {}", message);
        Ok(())
    }

    fn read_input(&self) -> Result<String, CompileError> {
        // Custom input implementation
        Err(CompileError::compiler_error("Custom input not implemented"))
    }

    fn write_file(&self, _path: &str, _content: &str) -> Result<(), CompileError> {
        // Custom file writing - might not be supported in embedded scenarios
        Err(CompileError::compiler_error(
            "File writing not supported in custom IO backend",
        ))
    }

    fn read_file(&self, _path: &str) -> Result<String, CompileError> {
        // Custom file reading - might not be supported in embedded scenarios
        Err(CompileError::compiler_error(
            "File reading not supported in custom IO backend",
        ))
    }
}
