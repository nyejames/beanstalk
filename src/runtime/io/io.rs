// IO abstraction layer for Beanstalk runtime
//
// Provides a generic IO interface that can be mapped to different backends:
// - WASI for standard I/O
// - Custom hooks for embedded scenarios  
// - JS bindings for web targets
use crate::compiler::compiler_errors::CompileError;

/// Generic IO interface that all backends must implement
/// TODO: Add more IO functions, just barebones for now to get it working
pub trait IoInterface {
    /// Print a string to the output
    fn print(&self, message: &str) -> Result<(), CompileError>;
    
    /// Read input from the user/environment
    fn read_input(&self) -> Result<String, CompileError>;
    
    /// Write to a file (if supported by backend)
    fn write_file(&self, path: &str, content: &str) -> Result<(), CompileError>;
    
    /// Read from a file (if supported by backend)
    fn read_file(&self, path: &str) -> Result<String, CompileError>;
}

/// IO backend configuration
#[derive(Debug, Clone)]
pub struct IoConfig {
    /// Backend-specific configuration
    pub backend_config: String,
    /// Whether to enable verbose IO logging
    pub verbose: bool,
    /// Custom IO mappings
    pub custom_mappings: Vec<IoMapping>,
}

#[derive(Debug, Clone)]
pub struct IoMapping {
    /// Function name in WASM
    pub wasm_function: String,
    /// Target function in the IO backend
    pub target_function: String,
    /// Function signature for type checking
    pub signature: FunctionSignature,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<IoType>,
    /// Return type
    pub return_type: Option<IoType>,
}

#[derive(Debug, Clone)]
pub enum IoType {
    I32,
    I64,
    F32,
    F64,
    String,
    Bytes,
}

impl Default for IoConfig {
    fn default() -> Self {
        Self {
            backend_config: String::new(),
            verbose: false,
            custom_mappings: vec![],
        }
    }
}