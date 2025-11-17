use crate::compiler::compiler_errors::CompileError;
use crate::return_compiler_error;
use std::collections::HashMap;
use wasm_encoder::ValType;
use wasmer::Value;

/// Native WASIX function implementation
pub type WasixNativeFunction = fn(&mut WasixContext, &[Value]) -> Result<Vec<Value>, WasixError>;

/// Defines a WASIX function that can be imported and called from WASM
#[derive(Debug, Clone)]
pub struct WasixFunctionDef {
    /// WASIX module name (e.g., "wasix_32v1")
    pub module: String,
    /// Function name (e.g., "fd_write")
    pub name: String,
    /// Parameter types in WASM format
    pub parameters: Vec<ValType>,
    /// Return types in WASM format
    pub returns: Vec<ValType>,
    /// Native implementation if available
    pub native_impl: Option<WasixNativeFunction>,
    /// WASM function index after import (set during codegen)
    pub func_index: Option<u32>,
    /// Human-readable description for documentation
    pub description: String,
}

impl WasixFunctionDef {
    /// Create a new WASIX function definition
    pub fn new(
        module: &str,
        name: &str,
        parameters: Vec<ValType>,
        returns: Vec<ValType>,
        description: &str,
    ) -> Self {
        WasixFunctionDef {
            module: module.to_string(),
            name: name.to_string(),
            parameters,
            returns,
            native_impl: None,
            func_index: None,
            description: description.to_string(),
        }
    }

    /// Create a new WASIX function definition with native implementation
    pub fn new_with_native(
        module: &str,
        name: &str,
        parameters: Vec<ValType>,
        returns: Vec<ValType>,
        native_impl: WasixNativeFunction,
        description: &str,
    ) -> Self {
        WasixFunctionDef {
            module: module.to_string(),
            name: name.to_string(),
            parameters,
            returns,
            native_impl: Some(native_impl),
            func_index: None,
            description: description.to_string(),
        }
    }

    /// Set the WASM function index for this WASIX function
    pub fn set_function_index(&mut self, index: u32) {
        self.func_index = Some(index);
    }

    /// Get the WASM function index, returning an error if not set
    pub fn get_function_index(&self) -> Result<u32, CompileError> {
        match self.func_index {
            Some(index) => Ok(index),
            None => {
                return_compiler_error!(
                    "WASIX function '{}' does not have a WASM function index assigned. This should be set during import generation.",
                    self.name
                );
            }
        }
    }

    /// Check if this function has a native implementation
    pub fn has_native_impl(&self) -> bool {
        self.native_impl.is_some()
    }
}

/// Registry for managing WASIX function definitions with native implementations
#[derive(Debug, Clone)]
pub struct WasixFunctionRegistry {
    /// Map from Beanstalk function name to WASIX function definition
    functions: HashMap<String, WasixFunctionDef>,
    /// Map from function name to native implementation
    native_functions: HashMap<String, WasixNativeFunction>,
}

impl WasixFunctionRegistry {
    /// Create a new empty WASIX registry
    pub fn new() -> Self {
        WasixFunctionRegistry {
            functions: HashMap::new(),
            native_functions: HashMap::new(),
        }
    }

    /// Get a WASIX function definition by Beanstalk function name
    pub fn get_function(&self, beanstalk_name: &str) -> Option<&WasixFunctionDef> {
        self.functions.get(beanstalk_name)
    }

    /// Get a mutable reference to a WASIX function definition
    pub fn get_function_mut(&mut self, beanstalk_name: &str) -> Option<&mut WasixFunctionDef> {
        self.functions.get_mut(beanstalk_name)
    }

    /// Register a new WASIX function mapping
    pub fn register_function(
        &mut self,
        beanstalk_name: &str,
        wasix_function: WasixFunctionDef,
    ) -> Result<(), CompileError> {
        // Validate the function definition first
        validate_wasix_function_def(&wasix_function)?;

        if self.functions.contains_key(beanstalk_name) {
            return_compiler_error!(
                "WASIX function mapping for '{}' is already registered. This is a compiler bug - duplicate function registration.",
                beanstalk_name
            );
        }

        // If the function has a native implementation, register it separately
        if let Some(native_func) = wasix_function.native_impl {
            self.native_functions
                .insert(beanstalk_name.to_string(), native_func);
        }

        self.functions
            .insert(beanstalk_name.to_string(), wasix_function);
        Ok(())
    }

    /// Register a native WASIX function implementation
    pub fn register_native_function(&mut self, name: &str, func: WasixNativeFunction) {
        self.native_functions.insert(name.to_string(), func);

        // Update the function definition if it exists
        if let Some(function_def) = self.functions.get_mut(name) {
            function_def.native_impl = Some(func);
        }
    }

    /// Get a native function implementation
    pub fn get_native_function(&self, name: &str) -> Option<&WasixNativeFunction> {
        self.native_functions.get(name)
    }

    /// List all registered WASIX functions
    pub fn list_functions(&self) -> Vec<(&String, &WasixFunctionDef)> {
        self.functions.iter().collect()
    }

    /// Check if a Beanstalk function has a WASIX mapping
    pub fn has_function(&self, beanstalk_name: &str) -> bool {
        self.functions.contains_key(beanstalk_name)
    }

    /// Get the number of registered WASIX functions
    pub fn count(&self) -> usize {
        self.functions.len()
    }

    /// Get all WASIX functions that need to be imported
    pub fn get_import_functions(&self) -> Vec<&WasixFunctionDef> {
        self.functions.values().collect()
    }
}

impl Default for WasixFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry populated with standard WASIX functions for Beanstalk
pub fn create_wasix_registry() -> Result<WasixFunctionRegistry, CompileError> {
    let mut registry = WasixFunctionRegistry::new();

    // Register fd_write function for print() support with native implementation
    // WASIX fd_write signature: (fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    // Use "wasi_snapshot_preview1" module name for compatibility with wasmer-wasix
    let fd_write_function = WasixFunctionDef::new_with_native(
        "wasi_snapshot_preview1",
        "fd_write",
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], // fd, iovs, iovs_len, nwritten
        vec![ValType::I32],                                           // errno result
        native_fd_write,
        "Write data to a file descriptor. Used to implement print() function with native WASIX implementation.",
    );

    registry.register_function("print", fd_write_function.clone())?;
    
    // Also register template_output with the same fd_write implementation
    // template_output is the new way to output text in Beanstalk (replaces print)
    registry.register_function("template_output", fd_write_function)?;

    // Validate all registered functions
    validate_wasix_registry(&registry)?;

    Ok(registry)
}

/// Native implementation of WASIX fd_write function
/// This function is called by the JIT runtime with proper memory access
/// Full implementation requires JIT runtime integration
fn native_fd_write(_context: &mut WasixContext, _args: &[Value]) -> Result<Vec<Value>, WasixError> {
    // This is a placeholder implementation
    // The actual implementation will be done in the JIT runtime where we have access to memory and store
    // When properly integrated, this should:
    // 1. Extract memory and store from the runtime context
    // 2. Call context.fd_write() with the extracted parameters
    // 3. Return the errno code as a WASM i32 value
    Err(WasixError::EnvironmentError(
        "Native fd_write requires JIT runtime integration with memory access".to_string(),
    ))
}

/// Validate that all WASIX function definitions in the registry are correct
fn validate_wasix_registry(registry: &WasixFunctionRegistry) -> Result<(), CompileError> {
    for (_, function) in registry.list_functions() {
        validate_wasix_function_def(function)?;
    }
    Ok(())
}

/// Validate a single WASIX function definition
fn validate_wasix_function_def(function: &WasixFunctionDef) -> Result<(), CompileError> {
    // Validate function name is not empty
    if function.name.is_empty() {
        return_compiler_error!(
            "WASIX function has empty name. Function definitions must have valid names."
        );
    }

    // Validate module name follows WASIX conventions
    if function.module.is_empty() {
        return_compiler_error!(
            "WASIX function '{}' has empty module name. WASIX imports require valid module names.",
            function.name
        );
    }

    // Validate module name is a standard WASIX module
    match function.module.as_str() {
        "wasix_32v1"
        | "wasix_64v1"
        | "wasix_snapshot_preview1"
        | "wasi_snapshot_preview1"
        | "wasi_unstable" => {
            // Valid WASIX module names (including older WASI versions)
        }
        _ => {
            return_compiler_error!(
                "WASIX function '{}' uses invalid module '{}'. Valid WASIX modules are: wasix_32v1, wasix_64v1, wasix_snapshot_preview1, wasi_snapshot_preview1, wasi_unstable",
                function.name,
                function.module
            );
        }
    }

    // Validate that we have reasonable parameter counts (WASIX functions typically have 0-10 parameters)
    if function.parameters.len() > 10 {
        return_compiler_error!(
            "WASIX function '{}' has {} parameters, which exceeds the reasonable limit of 10. This may indicate an error in the function definition.",
            function.name,
            function.parameters.len()
        );
    }

    // Validate that we have reasonable return counts (WASIX functions typically return 0-2 values)
    if function.returns.len() > 2 {
        return_compiler_error!(
            "WASIX function '{}' has {} return values, which exceeds the reasonable limit of 2. This may indicate an error in the function definition.",
            function.name,
            function.returns.len()
        );
    }

    Ok(())
}

impl PartialEq for WasixFunctionDef {
    fn eq(&self, other: &Self) -> bool {
        self.module == other.module
            && self.name == other.name
            && self.parameters == other.parameters
            && self.returns == other.returns
            && self.description == other.description
    }
}

impl Eq for WasixFunctionDef {}

impl std::hash::Hash for WasixFunctionDef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.module.hash(state);
        self.name.hash(state);
        self.parameters.hash(state);
        self.returns.hash(state);
        self.description.hash(state);
    }
}

/// WASIX-specific error types
#[derive(Debug, Clone)]
pub enum WasixError {
    /// Invalid file descriptor
    InvalidFileDescriptor(u32),
    /// Memory out of bounds
    MemoryOutOfBounds,
    /// Invalid argument count
    InvalidArgumentCount,
    /// WASIX environment error
    EnvironmentError(String),
    /// Native function not found
    NativeFunctionNotFound(String),
    /// Enhanced invalid file descriptor with context
    InvalidFileDescriptorWithContext {
        fd: u32,
        function: String,
        context: String,
    },
    /// Enhanced memory out of bounds with detailed location information
    MemoryOutOfBoundsWithContext {
        address: u32,
        size: u32,
        function: String,
        context: String,
    },
    /// Enhanced invalid argument count with expected vs actual
    InvalidArgumentCountWithContext {
        expected: usize,
        actual: usize,
        function: String,
    },
    /// Enhanced WASIX environment setup or configuration error
    EnvironmentErrorWithContext {
        message: String,
        function: String,
        suggestion: Option<String>,
    },
    /// Enhanced native function not found during runtime
    NativeFunctionNotFoundWithContext {
        function: String,
        available_functions: Vec<String>,
    },
    /// Import resolution failure
    ImportResolutionError {
        module: String,
        function: String,
        reason: String,
        suggestion: String,
    },
    /// Memory allocation failure
    AllocationError {
        requested_size: u32,
        available_size: u32,
        function: String,
        suggestion: String,
    },
    /// IOVec validation error
    IOVecError {
        iovec_index: usize,
        ptr: u32,
        len: u32,
        reason: String,
    },
    /// String encoding error
    StringEncodingError {
        position: u32,
        encoding: String,
        context: String,
    },
    /// Runtime configuration error
    ConfigurationError {
        setting: String,
        value: String,
        expected: String,
        suggestion: String,
    },
}

impl WasixError {
    /// Convert WasixError to POSIX errno code with comprehensive mapping
    /// Returns the appropriate errno value for WASIX function returns
    pub fn to_errno(&self) -> u32 {
        match self {
            // Basic error formats
            WasixError::InvalidFileDescriptor(_) => 9, // EBADF - Bad file descriptor
            WasixError::MemoryOutOfBounds => 14,       // EFAULT - Bad address
            WasixError::InvalidArgumentCount => 22,    // EINVAL - Invalid argument
            WasixError::EnvironmentError(_) => 5,      // EIO - Input/output error
            WasixError::NativeFunctionNotFound(_) => 38, // ENOSYS - Function not implemented

            // Error formats with context
            WasixError::InvalidFileDescriptorWithContext { .. } => 9, // EBADF - Bad file descriptor
            WasixError::MemoryOutOfBoundsWithContext { .. } => 14,    // EFAULT - Bad address
            WasixError::InvalidArgumentCountWithContext { .. } => 22, // EINVAL - Invalid argument
            WasixError::EnvironmentErrorWithContext { .. } => 5,      // EIO - Input/output error
            WasixError::NativeFunctionNotFoundWithContext { .. } => 38, // ENOSYS - Function not implemented
            WasixError::ImportResolutionError { .. } => 2, // ENOENT - No such file or directory
            WasixError::AllocationError { .. } => 12,      // ENOMEM - Out of memory
            WasixError::IOVecError { .. } => 22,           // EINVAL - Invalid argument
            WasixError::StringEncodingError { .. } => 84,  // EILSEQ - Illegal byte sequence
            WasixError::ConfigurationError { .. } => 22,   // EINVAL - Invalid argument
        }
    }

    /// Get the error category for diagnostic purposes
    pub fn category(&self) -> &'static str {
        match self {
            WasixError::InvalidFileDescriptor(_)
            | WasixError::InvalidFileDescriptorWithContext { .. } => "File Descriptor Error",
            WasixError::MemoryOutOfBounds | WasixError::MemoryOutOfBoundsWithContext { .. } => {
                "Memory Error"
            }
            WasixError::InvalidArgumentCount
            | WasixError::InvalidArgumentCountWithContext { .. } => "Argument Error",
            WasixError::EnvironmentError(_) | WasixError::EnvironmentErrorWithContext { .. } => {
                "Environment Error"
            }
            WasixError::NativeFunctionNotFound(_)
            | WasixError::NativeFunctionNotFoundWithContext { .. } => "Function Resolution Error",
            WasixError::ImportResolutionError { .. } => "Import Resolution Error",
            WasixError::AllocationError { .. } => "Memory Allocation Error",
            WasixError::IOVecError { .. } => "IOVec Validation Error",
            WasixError::StringEncodingError { .. } => "String Encoding Error",
            WasixError::ConfigurationError { .. } => "Configuration Error",
        }
    }

    /// Get the function name associated with this error
    pub fn function_name(&self) -> &str {
        match self {
            WasixError::InvalidFileDescriptor(_) => "unknown",
            WasixError::MemoryOutOfBounds => "unknown",
            WasixError::InvalidArgumentCount => "unknown",
            WasixError::EnvironmentError(_) => "unknown",
            WasixError::NativeFunctionNotFound(_) => "unknown",
            WasixError::InvalidFileDescriptorWithContext { function, .. } => function,
            WasixError::MemoryOutOfBoundsWithContext { function, .. } => function,
            WasixError::InvalidArgumentCountWithContext { function, .. } => function,
            WasixError::EnvironmentErrorWithContext { function, .. } => function,
            WasixError::NativeFunctionNotFoundWithContext { function, .. } => function,
            WasixError::ImportResolutionError { function, .. } => function,
            WasixError::AllocationError { function, .. } => function,
            WasixError::IOVecError { .. } => "IOVec validation",
            WasixError::StringEncodingError { .. } => "String encoding",
            WasixError::ConfigurationError { .. } => "Configuration",
        }
    }

    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            WasixError::InvalidFileDescriptor(_)
            | WasixError::InvalidFileDescriptorWithContext { .. } => false,
            WasixError::MemoryOutOfBounds | WasixError::MemoryOutOfBoundsWithContext { .. } => {
                false
            }
            WasixError::InvalidArgumentCount
            | WasixError::InvalidArgumentCountWithContext { .. } => false,
            WasixError::EnvironmentError(_) | WasixError::EnvironmentErrorWithContext { .. } => {
                true
            } // May be fixable with configuration
            WasixError::NativeFunctionNotFound(_)
            | WasixError::NativeFunctionNotFoundWithContext { .. } => false,
            WasixError::ImportResolutionError { .. } => true, // May be fixable with runtime setup
            WasixError::AllocationError { .. } => true,       // May be fixable with more memory
            WasixError::IOVecError { .. } => false,
            WasixError::StringEncodingError { .. } => false,
            WasixError::ConfigurationError { .. } => true, // Fixable with correct configuration
        }
    }
}

impl std::fmt::Display for WasixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Basic error formats
            WasixError::InvalidFileDescriptor(fd) => {
                write!(
                    f,
                    "Invalid file descriptor: {}. WASIX supports stdout (1) and stderr (2)",
                    fd
                )
            }
            WasixError::MemoryOutOfBounds => {
                write!(
                    f,
                    "Memory access out of bounds in WASIX operation. Check memory allocation and pointer arithmetic"
                )
            }
            WasixError::InvalidArgumentCount => {
                write!(f, "Invalid argument count for WASIX function call")
            }
            WasixError::EnvironmentError(msg) => {
                write!(
                    f,
                    "WASIX environment error: {}. Ensure WASIX is properly configured",
                    msg
                )
            }
            WasixError::NativeFunctionNotFound(name) => {
                write!(
                    f,
                    "Native WASIX function '{}' not found. Check WASIX registry configuration",
                    name
                )
            }

            // Enhanced error formats
            WasixError::InvalidFileDescriptorWithContext {
                fd,
                function,
                context,
            } => {
                write!(
                    f,
                    "Invalid file descriptor {} in function '{}': {}. WASIX supports stdout (1), stderr (2), and stdin (0)",
                    fd, function, context
                )
            }
            WasixError::MemoryOutOfBoundsWithContext {
                address,
                size,
                function,
                context,
            } => {
                write!(
                    f,
                    "Memory access out of bounds in function '{}': tried to access {} bytes at address 0x{:x}. Context: {}",
                    function, size, address, context
                )
            }
            WasixError::InvalidArgumentCountWithContext {
                expected,
                actual,
                function,
            } => {
                write!(
                    f,
                    "Invalid argument count for WASIX function '{}': expected {} arguments, got {}",
                    function, expected, actual
                )
            }
            WasixError::EnvironmentErrorWithContext {
                message,
                function,
                suggestion,
            } => match suggestion {
                Some(hint) => write!(
                    f,
                    "WASIX environment error in '{}': {}. Suggestion: {}",
                    function, message, hint
                ),
                None => write!(
                    f,
                    "WASIX environment error in '{}': {}. Ensure WASIX is properly configured",
                    function, message
                ),
            },
            WasixError::NativeFunctionNotFoundWithContext {
                function,
                available_functions,
            } => {
                if available_functions.is_empty() {
                    write!(
                        f,
                        "Native WASIX function '{}' not found. No WASIX functions are currently registered",
                        function
                    )
                } else {
                    write!(
                        f,
                        "Native WASIX function '{}' not found. Available functions: {}",
                        function,
                        available_functions.join(", ")
                    )
                }
            }
            WasixError::ImportResolutionError {
                module,
                function,
                reason,
                suggestion,
            } => {
                write!(
                    f,
                    "Failed to resolve WASIX import '{}:{}': {}. Suggestion: {}",
                    module, function, reason, suggestion
                )
            }
            WasixError::AllocationError {
                requested_size,
                available_size,
                function,
                suggestion,
            } => {
                write!(
                    f,
                    "Memory allocation failed in '{}': requested {} bytes, only {} bytes available. Suggestion: {}",
                    function, requested_size, available_size, suggestion
                )
            }
            WasixError::IOVecError {
                iovec_index,
                ptr,
                len,
                reason,
            } => {
                write!(
                    f,
                    "IOVec validation failed at index {}: ptr=0x{:x}, len={}, reason: {}",
                    iovec_index, ptr, len, reason
                )
            }
            WasixError::StringEncodingError {
                position,
                encoding,
                context,
            } => {
                write!(
                    f,
                    "String encoding error at position {}: invalid {} encoding. Context: {}",
                    position, encoding, context
                )
            }
            WasixError::ConfigurationError {
                setting,
                value,
                expected,
                suggestion,
            } => {
                write!(
                    f,
                    "Configuration error: setting '{}' has value '{}', expected '{}'. Suggestion: {}",
                    setting, value, expected, suggestion
                )
            }
        }
    }
}

impl std::error::Error for WasixError {}

impl WasixError {
    /// Create an enhanced InvalidFileDescriptor error with context
    pub fn invalid_fd_with_context(fd: u32, function: &str, context: &str) -> Self {
        WasixError::InvalidFileDescriptorWithContext {
            fd,
            function: function.to_string(),
            context: context.to_string(),
        }
    }

    /// Create an enhanced MemoryOutOfBounds error with detailed location
    pub fn memory_out_of_bounds_with_context(
        address: u32,
        size: u32,
        function: &str,
        context: &str,
    ) -> Self {
        WasixError::MemoryOutOfBoundsWithContext {
            address,
            size,
            function: function.to_string(),
            context: context.to_string(),
        }
    }

    /// Create an enhanced InvalidArgumentCount error
    pub fn invalid_arg_count_with_context(expected: usize, actual: usize, function: &str) -> Self {
        WasixError::InvalidArgumentCountWithContext {
            expected,
            actual,
            function: function.to_string(),
        }
    }

    /// Create an enhanced EnvironmentError with optional suggestion
    pub fn environment_error_with_context(
        message: &str,
        function: &str,
        suggestion: Option<&str>,
    ) -> Self {
        WasixError::EnvironmentErrorWithContext {
            message: message.to_string(),
            function: function.to_string(),
            suggestion: suggestion.map(|s| s.to_string()),
        }
    }

    /// Create an enhanced NativeFunctionNotFound error with available functions list
    pub fn function_not_found_with_context(
        function: &str,
        available_functions: Vec<String>,
    ) -> Self {
        WasixError::NativeFunctionNotFoundWithContext {
            function: function.to_string(),
            available_functions,
        }
    }

    /// Create an ImportResolutionError with suggestion
    pub fn import_resolution_error(
        module: &str,
        function: &str,
        reason: &str,
        suggestion: &str,
    ) -> Self {
        WasixError::ImportResolutionError {
            module: module.to_string(),
            function: function.to_string(),
            reason: reason.to_string(),
            suggestion: suggestion.to_string(),
        }
    }

    /// Create an AllocationError with memory information
    pub fn allocation_error(
        requested_size: u32,
        available_size: u32,
        function: &str,
        suggestion: &str,
    ) -> Self {
        WasixError::AllocationError {
            requested_size,
            available_size,
            function: function.to_string(),
            suggestion: suggestion.to_string(),
        }
    }

    /// Create an IOVecError with validation details
    pub fn iovec_error(iovec_index: usize, ptr: u32, len: u32, reason: &str) -> Self {
        WasixError::IOVecError {
            iovec_index,
            ptr,
            len,
            reason: reason.to_string(),
        }
    }

    /// Create a StringEncodingError with position information
    pub fn string_encoding_error(position: u32, encoding: &str, context: &str) -> Self {
        WasixError::StringEncodingError {
            position,
            encoding: encoding.to_string(),
            context: context.to_string(),
        }
    }

    /// Create a ConfigurationError with expected value
    pub fn configuration_error(
        setting: &str,
        value: &str,
        expected: &str,
        suggestion: &str,
    ) -> Self {
        WasixError::ConfigurationError {
            setting: setting.to_string(),
            value: value.to_string(),
            expected: expected.to_string(),
            suggestion: suggestion.to_string(),
        }
    }
}

/// Represents a region of allocated memory in WASM linear memory
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    /// Start pointer in linear memory
    pub ptr: u32,
    /// Size in bytes
    pub size: u32,
}

impl MemoryRegion {
    /// Create a new memory region
    pub fn new(ptr: u32, size: u32) -> Self {
        MemoryRegion { ptr, size }
    }

    /// Create an empty memory region
    pub fn empty() -> Self {
        MemoryRegion { ptr: 0, size: 0 }
    }

    /// Check if this region is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Get the end pointer (exclusive) of this region
    pub fn end_ptr(&self) -> u32 {
        self.ptr + self.size
    }

    /// Check if this region overlaps with another region
    pub fn overlaps_with(&self, other: &MemoryRegion) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.ptr < other.end_ptr() && other.ptr < self.end_ptr()
    }

    /// Check if this region contains the given address
    pub fn contains_address(&self, addr: u32) -> bool {
        addr >= self.ptr && addr < self.end_ptr()
    }

    /// Check if this region fully contains another region
    pub fn contains_region(&self, other: &MemoryRegion) -> bool {
        if other.is_empty() {
            return true;
        }
        if self.is_empty() {
            return false;
        }
        other.ptr >= self.ptr && other.end_ptr() <= self.end_ptr()
    }

    /// Get the intersection of this region with another region
    pub fn intersect(&self, other: &MemoryRegion) -> Option<MemoryRegion> {
        if !self.overlaps_with(other) {
            return None;
        }

        let start = self.ptr.max(other.ptr);
        let end = self.end_ptr().min(other.end_ptr());

        Some(MemoryRegion::new(start, end - start))
    }

    /// Split this region at the given offset
    pub fn split_at(&self, offset: u32) -> Result<(MemoryRegion, MemoryRegion), WasixError> {
        if offset > self.size {
            return Err(WasixError::MemoryOutOfBounds);
        }

        let first = MemoryRegion::new(self.ptr, offset);
        let second = MemoryRegion::new(self.ptr + offset, self.size - offset);

        Ok((first, second))
    }

    /// Validate that this region has reasonable values
    pub fn validate(&self) -> Result<(), WasixError> {
        // Check for pointer overflow
        if self.ptr > 0 && self.size > 0 {
            self.ptr
                .checked_add(self.size)
                .ok_or(WasixError::MemoryOutOfBounds)?;
        }

        // Check for reasonable size limits (16MB max for individual regions)
        if self.size > 0x1000000 {
            return Err(WasixError::EnvironmentError(format!(
                "Memory region size {} exceeds reasonable limit of 16MB",
                self.size
            )));
        }

        Ok(())
    }
}

/// IOVec structure matching WASIX specification
/// Used for scatter-gather I/O operations like fd_write
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IOVec {
    /// Pointer to data in linear memory
    pub ptr: u32,
    /// Length of data in bytes
    pub len: u32,
}

impl IOVec {
    /// Create a new IOVec
    pub fn new(ptr: u32, len: u32) -> Self {
        IOVec { ptr, len }
    }

    /// Create an empty IOVec (null pointer, zero length)
    pub fn empty() -> Self {
        IOVec { ptr: 0, len: 0 }
    }

    /// Check if this IOVec is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get the memory region covered by this IOVec
    pub fn as_memory_region(&self) -> MemoryRegion {
        MemoryRegion::new(self.ptr, self.len)
    }

    /// Validate that this IOVec has reasonable values
    pub fn validate(&self) -> Result<(), CompileError> {
        // Check for null pointer (0 is typically invalid for data)
        if self.ptr == 0 && self.len > 0 {
            return_compiler_error!(
                "IOVec has null pointer but non-zero length {}. This is invalid for WASIX operations.",
                self.len
            );
        }

        // Check for reasonable length limits (1MB max for individual IOVec)
        if self.len > 0x100000 {
            return_compiler_error!(
                "IOVec length {} exceeds reasonable limit of 1MB. This may indicate a bug in string handling.",
                self.len
            );
        }

        // Check for pointer overflow
        if self.ptr > 0 && self.len > 0 {
            let end_ptr = self.ptr.checked_add(self.len);
            if end_ptr.is_none() {
                return_compiler_error!(
                    "IOVec pointer 0x{:x} + length {} would overflow. This indicates invalid memory layout.",
                    self.ptr,
                    self.len
                );
            }
        }

        Ok(())
    }

    /// Validate this IOVec against a memory manager's allocated regions
    pub fn validate_against_memory(
        &self,
        memory_manager: &WasixMemoryManager,
    ) -> Result<(), WasixError> {
        if self.is_empty() {
            return Ok(()); // Empty IOVecs are always valid
        }

        // Check if the IOVec points to a valid allocated region
        if !memory_manager.is_valid_address(self.ptr, self.len) {
            return Err(WasixError::MemoryOutOfBounds);
        }

        Ok(())
    }

    /// Get the size in bytes needed to store this IOVec structure in WASM memory
    /// IOVec is 8 bytes: 4 bytes for ptr + 4 bytes for len
    pub const fn struct_size() -> u32 {
        8
    }

    /// Get the required alignment for IOVec structures
    /// WASIX prefers 8-byte alignment for IOVec structures for enhanced performance
    pub const fn required_alignment() -> u32 {
        8
    }

    /// Convert this IOVec to bytes for writing to WASM linear memory
    /// Returns the IOVec structure as little-endian bytes
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&self.ptr.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.len.to_le_bytes());
        bytes
    }

    /// Create an IOVec from bytes read from WASM linear memory
    /// Expects little-endian byte order
    pub fn from_bytes(bytes: &[u8; 8]) -> Self {
        let ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        IOVec::new(ptr, len)
    }

    /// Create an IOVec from a slice of bytes (for reading from memory)
    pub fn from_slice(bytes: &[u8]) -> Result<Self, WasixError> {
        if bytes.len() < 8 {
            return Err(WasixError::MemoryOutOfBounds);
        }

        let ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        Ok(IOVec::new(ptr, len))
    }

    /// Write this IOVec structure to a byte buffer
    pub fn write_to_buffer(&self, buffer: &mut [u8]) -> Result<(), WasixError> {
        if buffer.len() < 8 {
            return Err(WasixError::MemoryOutOfBounds);
        }

        let bytes = self.to_bytes();
        buffer[0..8].copy_from_slice(&bytes);
        Ok(())
    }

    /// Calculate the total size of an array of IOVecs
    pub fn calculate_array_size(count: u32) -> Result<u32, WasixError> {
        count.checked_mul(8).ok_or(WasixError::MemoryOutOfBounds)
    }

    /// Get the end pointer of this IOVec (exclusive)
    pub fn end_ptr(&self) -> Option<u32> {
        self.ptr.checked_add(self.len)
    }

    /// Check if this IOVec overlaps with another IOVec
    pub fn overlaps_with(&self, other: &IOVec) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }

        let self_end = match self.end_ptr() {
            Some(end) => end,
            None => return false, // Overflow means no valid overlap
        };

        let other_end = match other.end_ptr() {
            Some(end) => end,
            None => return false,
        };

        self.ptr < other_end && other.ptr < self_end
    }
}

/// WASIX memory layout information for enhanced memory management
#[derive(Debug, Clone)]
pub struct WasixMemoryLayout {
    /// WASIX reserved area (0x0000 - 0x1000) - 4KB reserved for WASIX internals
    pub wasix_reserved_start: u32,
    pub wasix_reserved_size: u32,
    /// Stack area for WASM stack operations
    pub stack_start: u32,
    pub stack_size: u32,
    /// Heap area for dynamic allocations
    pub heap_start: u32,
    pub heap_size: u32,
    /// String data area for WASIX string operations
    pub string_data_start: u32,
    pub string_data_size: u32,
    /// IOVec area for WASIX I/O operations
    pub iovec_area_start: u32,
    pub iovec_area_size: u32,
}

impl WasixMemoryLayout {
    /// Create a new WASIX memory layout with default values
    pub fn new() -> Self {
        Self {
            // WASIX reserved area: 0x0000 - 0x1000 (4KB)
            wasix_reserved_start: 0x0000,
            wasix_reserved_size: 0x1000,

            // Stack area: 0x1000 - 0x4000 (12KB)
            stack_start: 0x1000,
            stack_size: 0x3000,

            // Heap area: 0x4000 - 0xC000 (32KB)
            heap_start: 0x4000,
            heap_size: 0x8000,

            // String data area: 0xC000 - 0xF000 (12KB)
            string_data_start: 0xC000,
            string_data_size: 0x3000,

            // IOVec area: 0xF000 - 0x10000 (4KB)
            iovec_area_start: 0xF000,
            iovec_area_size: 0x1000,
        }
    }

    /// Check if an address is in the WASIX reserved area
    pub fn is_in_reserved_area(&self, addr: u32) -> bool {
        addr >= self.wasix_reserved_start
            && addr < (self.wasix_reserved_start + self.wasix_reserved_size)
    }

    /// Get the total memory layout size
    pub fn total_size(&self) -> u32 {
        self.iovec_area_start + self.iovec_area_size
    }

    /// Validate that the memory layout is consistent
    pub fn validate(&self) -> Result<(), WasixError> {
        // Check that areas don't overlap
        let areas = [
            (
                "WASIX reserved",
                self.wasix_reserved_start,
                self.wasix_reserved_size,
            ),
            ("Stack", self.stack_start, self.stack_size),
            ("Heap", self.heap_start, self.heap_size),
            ("String data", self.string_data_start, self.string_data_size),
            ("IOVec area", self.iovec_area_start, self.iovec_area_size),
        ];

        for i in 0..areas.len() {
            for j in (i + 1)..areas.len() {
                let (name1, start1, size1) = areas[i];
                let (name2, start2, size2) = areas[j];

                let end1 = start1 + size1;
                let end2 = start2 + size2;

                if start1 < end2 && start2 < end1 {
                    return Err(WasixError::EnvironmentError(format!(
                        "Memory layout conflict: {} area overlaps with {} area",
                        name1, name2
                    )));
                }
            }
        }

        Ok(())
    }
}

impl Default for WasixMemoryLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for WASIX memory operations
#[derive(Debug, Clone)]
pub struct WasixMemoryStats {
    /// Total number of allocations performed
    pub total_allocations: u64,
    /// Total bytes allocated
    pub total_bytes_allocated: u64,
    /// Peak memory usage
    pub peak_memory_usage: u32,
    /// Number of alignment adjustments made
    pub alignment_adjustments: u64,
    /// Number of allocation failures
    pub allocation_failures: u64,
}

impl WasixMemoryStats {
    /// Create new memory statistics
    pub fn new() -> Self {
        Self {
            total_allocations: 0,
            total_bytes_allocated: 0,
            peak_memory_usage: 0,
            alignment_adjustments: 0,
            allocation_failures: 0,
        }
    }

    /// Record a successful allocation
    pub fn record_allocation(&mut self, size: u32, aligned_size: u32) {
        self.total_allocations += 1;
        self.total_bytes_allocated += size as u64;

        if aligned_size > size {
            self.alignment_adjustments += 1;
        }
    }

    /// Record an allocation failure
    pub fn record_failure(&mut self) {
        self.allocation_failures += 1;
    }

    /// Update peak memory usage
    pub fn update_peak_usage(&mut self, current_usage: u32) {
        if current_usage > self.peak_memory_usage {
            self.peak_memory_usage = current_usage;
        }
    }
}

impl Default for WasixMemoryStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Context for managing memory layout during WASIX function calls
#[derive(Debug, Clone)]
pub struct WasixCallContext {
    /// String data location in linear memory
    pub string_region: MemoryRegion,
    /// IOVec structure location in linear memory
    pub iovec_region: MemoryRegion,
    /// Result pointer location (for nwritten, etc.)
    pub result_region: MemoryRegion,
}

impl WasixCallContext {
    /// Create a new WASIX call context
    pub fn new(
        string_region: MemoryRegion,
        iovec_region: MemoryRegion,
        result_region: MemoryRegion,
    ) -> Self {
        WasixCallContext {
            string_region,
            iovec_region,
            result_region,
        }
    }
}

/// WASIX runtime context for managing WASIX runtime state and environment
pub struct WasixContext {
    /// WASIX environment state
    pub env: WasixEnv,
    /// Memory manager for WASIX operations
    pub memory_manager: WasixMemoryManager,
    /// File descriptor table
    pub fd_table: FdTable,
    /// Process information
    pub process_info: ProcessInfo,
}

/// WASIX environment wrapper
pub struct WasixEnv {
    /// Standard output handler
    pub stdout: Box<dyn std::io::Write + Send>,
    /// Standard error handler
    pub stderr: Box<dyn std::io::Write + Send>,
    /// Environment variables
    pub env_vars: std::collections::HashMap<String, String>,
}

/// File descriptor table for WASIX operations
#[derive(Debug, Clone)]
pub struct FdTable {
    /// Map from file descriptor to file info
    descriptors: std::collections::HashMap<u32, FileDescriptor>,
    /// Next available file descriptor
    next_fd: u32,
}

/// File descriptor information
#[derive(Debug, Clone)]
pub struct FileDescriptor {
    /// File descriptor number
    pub fd: u32,
    /// File type (stdout, stderr, file, etc.)
    pub fd_type: FileDescriptorType,
    /// File permissions
    pub permissions: FilePermissions,
}

/// Types of file descriptors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDescriptorType {
    /// Standard input
    Stdin,
    /// Standard output
    Stdout,
    /// Standard error
    Stderr,
    /// Regular file
    File(String),
    /// Directory
    Directory(String),
}

/// File permissions for WASIX operations
#[derive(Debug, Clone)]
pub struct FilePermissions {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
}

/// Process information for WASIX context
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Parent process ID
    pub ppid: u32,
    /// Process name
    pub name: String,
    /// Command line arguments
    pub args: Vec<String>,
    /// Working directory
    pub cwd: String,
}

/// Memory manager for WASIX operations with enhanced allocation strategies
#[derive(Debug, Clone)]
pub struct WasixMemoryManager {
    /// Current allocation pointer (next available address)
    current_ptr: u32,
    /// Minimum alignment for allocations (typically 8 bytes for WASIX)
    default_alignment: u32,
    /// Track all allocated regions for debugging and cleanup
    allocated_regions: Vec<MemoryRegion>,
    /// Starting address for WASIX allocations (avoid conflicts with other memory usage)
    base_address: u32,
    /// Memory layout information for WASIX operations
    layout: WasixMemoryLayout,
    /// Maximum memory size allowed for allocations
    max_memory_size: u32,
    /// Allocation statistics for debugging and optimization
    stats: WasixMemoryStats,
}

impl WasixContext {
    /// Create a new WASIX context with enhanced I/O capabilities
    pub fn new() -> Result<Self, WasixError> {
        let env = WasixEnv::new()?;
        let memory_manager = WasixMemoryManager::new();
        let fd_table = FdTable::new();
        let process_info = ProcessInfo::new();

        Ok(Self {
            env,
            memory_manager,
            fd_table,
            process_info,
        })
    }

    /// Write data to file descriptor (native implementation)
    /// This is the core WASIX fd_write implementation that accesses WASM memory
    /// Returns POSIX errno code (0 for success, positive values for errors)
    pub fn fd_write(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        fd: u32,
        iovs_ptr: u32,
        iovs_len: u32,
        nwritten_ptr: u32,
    ) -> u32 {
        // Handle the operation and convert any errors to errno codes
        match self.fd_write_impl(memory, store, fd, iovs_ptr, iovs_len, nwritten_ptr) {
            Ok(()) => 0,            // Success
            Err(e) => e.to_errno(), // Convert error to errno code
        }
    }

    /// Internal implementation of fd_write that returns WasixError for detailed error handling
    fn fd_write_impl(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        fd: u32,
        iovs_ptr: u32,
        iovs_len: u32,
        nwritten_ptr: u32,
    ) -> Result<(), WasixError> {
        // Validate file descriptor
        if !self.fd_table.is_valid_fd(fd) {
            return Err(WasixError::InvalidFileDescriptor(fd));
        }

        // Handle empty write
        if iovs_len == 0 {
            // Write 0 to nwritten_ptr and return success
            self.write_u32(memory, store, nwritten_ptr, 0)?;
            return Ok(());
        }

        // Read IOVec structures from WASM memory
        let iovecs = self.read_iovecs(memory, store, iovs_ptr, iovs_len)?;

        // Validate all IOVecs
        self.validate_iovecs(memory, store, &iovecs)?;

        // Validate total size across all IOVecs (prevent excessive memory usage)
        let total_size: u32 = iovecs.iter().map(|iov| iov.len).sum();
        if total_size > 0x1000000 {
            // 16MB limit for total write operation
            return Err(WasixError::EnvironmentError(format!(
                "Total write size {} across {} IOVecs exceeds 16MB limit",
                total_size,
                iovecs.len()
            )));
        }

        // Write data to the appropriate file descriptor
        let mut total_written = 0u32;

        // Process multiple IOVec entries with proper error handling
        let write_result = match fd {
            1 => {
                // stdout
                self.write_iovecs_to_stdout(memory, store, &iovecs, &mut total_written)
            }
            2 => {
                // stderr
                self.write_iovecs_to_stderr(memory, store, &iovecs, &mut total_written)
            }
            _ => {
                return Err(WasixError::InvalidFileDescriptor(fd));
            }
        };

        // Even if there was an error, write the partial bytes written count
        // This is important for WASIX compliance - caller needs to know how much was written
        if let Err(ref _e) = write_result {
            // Write partial count and return error
            self.write_u32(memory, store, nwritten_ptr, total_written)?;
            return write_result;
        }

        // Write the total bytes written to nwritten_ptr in WASM memory
        self.write_u32(memory, store, nwritten_ptr, total_written)?;

        Ok(()) // Success
    }

    /// Write multiple IOVec entries to stdout with proper error handling
    /// Updates total_written with the number of bytes successfully written
    fn write_iovecs_to_stdout(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovecs: &[IOVec],
        total_written: &mut u32,
    ) -> Result<(), WasixError> {
        for (i, iovec) in iovecs.iter().enumerate() {
            if iovec.len > 0 {
                // Read string data from this IOVec
                let string_data =
                    self.read_string_from_iovec(memory, store, iovec)
                        .map_err(|e| {
                            WasixError::EnvironmentError(format!(
                                "Failed to read IOVec {} data: {}",
                                i, e
                            ))
                        })?;

                // Write to stdout with error handling
                match std::io::Write::write_all(&mut self.env.stdout, string_data.as_bytes()) {
                    Ok(()) => {
                        *total_written += iovec.len;
                    }
                    Err(e) => {
                        // Return error but total_written reflects partial success
                        return Err(WasixError::EnvironmentError(format!(
                            "Failed to write IOVec {} to stdout after {} bytes written: {}",
                            i, *total_written, e
                        )));
                    }
                }
            }
        }

        // Flush stdout after all IOVecs are written
        std::io::Write::flush(&mut self.env.stdout)
            .map_err(|e| WasixError::EnvironmentError(format!("Failed to flush stdout: {}", e)))?;

        Ok(())
    }

    /// Write multiple IOVec entries to stderr with proper error handling
    /// Updates total_written with the number of bytes successfully written
    fn write_iovecs_to_stderr(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovecs: &[IOVec],
        total_written: &mut u32,
    ) -> Result<(), WasixError> {
        for (i, iovec) in iovecs.iter().enumerate() {
            if iovec.len > 0 {
                // Read string data from this IOVec
                let string_data =
                    self.read_string_from_iovec(memory, store, iovec)
                        .map_err(|e| {
                            WasixError::EnvironmentError(format!(
                                "Failed to read IOVec {} data: {}",
                                i, e
                            ))
                        })?;

                // Write to stderr with error handling
                match std::io::Write::write_all(&mut self.env.stderr, string_data.as_bytes()) {
                    Ok(()) => {
                        *total_written += iovec.len;
                    }
                    Err(e) => {
                        // Return error but total_written reflects partial success
                        return Err(WasixError::EnvironmentError(format!(
                            "Failed to write IOVec {} to stderr after {} bytes written: {}",
                            i, *total_written, e
                        )));
                    }
                }
            }
        }

        // Flush stderr after all IOVecs are written
        std::io::Write::flush(&mut self.env.stderr)
            .map_err(|e| WasixError::EnvironmentError(format!("Failed to flush stderr: {}", e)))?;

        Ok(())
    }

    /// Write multiple IOVec entries as concatenated output (alternative approach)
    /// This method concatenates all IOVec data before writing, which can be more efficient
    /// for some use cases but uses more memory
    #[allow(dead_code)] // May be used by future WASIX functions
    fn write_iovecs_concatenated(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovecs: &[IOVec],
        fd: u32,
    ) -> Result<u32, WasixError> {
        // Calculate total size and validate
        let total_size: u32 = iovecs.iter().map(|iov| iov.len).sum();
        if total_size == 0 {
            return Ok(0);
        }

        // Read and concatenate all IOVec data
        let concatenated_data = self.read_string_from_iovecs(memory, store, iovecs)?;

        // Write concatenated data to the appropriate file descriptor
        let bytes = concatenated_data.as_bytes();
        match fd {
            1 => {
                // stdout
                std::io::Write::write_all(&mut self.env.stdout, bytes).map_err(|e| {
                    WasixError::EnvironmentError(format!("Failed to write to stdout: {}", e))
                })?;
                std::io::Write::flush(&mut self.env.stdout).map_err(|e| {
                    WasixError::EnvironmentError(format!("Failed to flush stdout: {}", e))
                })?;
            }
            2 => {
                // stderr
                std::io::Write::write_all(&mut self.env.stderr, bytes).map_err(|e| {
                    WasixError::EnvironmentError(format!("Failed to write to stderr: {}", e))
                })?;
                std::io::Write::flush(&mut self.env.stderr).map_err(|e| {
                    WasixError::EnvironmentError(format!("Failed to flush stderr: {}", e))
                })?;
            }
            _ => {
                return Err(WasixError::InvalidFileDescriptor(fd));
            }
        }

        Ok(total_size)
    }

    /// Read bytes from WASM linear memory with bounds checking
    pub fn read_bytes(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        ptr: u32,
        len: u32,
    ) -> Result<Vec<u8>, WasixError> {
        // Validate memory bounds
        if len == 0 {
            return Ok(Vec::new());
        }

        let end_ptr = ptr.checked_add(len).ok_or(WasixError::MemoryOutOfBounds)?;

        // Get memory view and check bounds
        let memory_view = memory.view(store);
        let memory_size = memory_view.data_size() as u32;

        if end_ptr > memory_size {
            return Err(WasixError::MemoryOutOfBounds);
        }

        // Read bytes from memory
        let mut bytes = vec![0u8; len as usize];
        memory_view
            .read(ptr as u64, &mut bytes)
            .map_err(|_| WasixError::MemoryOutOfBounds)?;

        Ok(bytes)
    }

    /// Write bytes to WASM linear memory with bounds checking
    pub fn write_bytes(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        ptr: u32,
        data: &[u8],
    ) -> Result<(), WasixError> {
        if data.is_empty() {
            return Ok(());
        }

        let len = data.len() as u32;
        let end_ptr = ptr.checked_add(len).ok_or(WasixError::MemoryOutOfBounds)?;

        // Get memory view and check bounds
        let memory_view = memory.view(store);
        let memory_size = memory_view.data_size() as u32;

        if end_ptr > memory_size {
            return Err(WasixError::MemoryOutOfBounds);
        }

        // Write bytes to memory
        memory_view
            .write(ptr as u64, data)
            .map_err(|_| WasixError::MemoryOutOfBounds)?;

        Ok(())
    }

    /// Read a u32 value from WASM memory (little-endian)
    pub fn read_u32(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        ptr: u32,
    ) -> Result<u32, WasixError> {
        let bytes = self.read_bytes(memory, store, ptr, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Write a u32 value to WASM memory (little-endian)
    pub fn write_u32(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        ptr: u32,
        value: u32,
    ) -> Result<(), WasixError> {
        let bytes = value.to_le_bytes();
        self.write_bytes(memory, store, ptr, &bytes)
    }

    /// Read IOVec structures from WASM memory
    /// Each IOVec is 8 bytes: 4 bytes ptr + 4 bytes len (little-endian)
    pub fn read_iovecs(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovs_ptr: u32,
        iovs_len: u32,
    ) -> Result<Vec<IOVec>, WasixError> {
        if iovs_len == 0 {
            return Ok(Vec::new());
        }

        // Validate IOVec array bounds
        let total_size = iovs_len
            .checked_mul(8) // Each IOVec is 8 bytes
            .ok_or(WasixError::MemoryOutOfBounds)?;

        if iovs_ptr == 0 {
            return Err(WasixError::MemoryOutOfBounds);
        }

        // Read the entire IOVec array from memory
        let iovec_bytes = self.read_bytes(memory, store, iovs_ptr, total_size)?;

        // Parse each IOVec structure
        let mut iovecs = Vec::with_capacity(iovs_len as usize);
        for i in 0..iovs_len {
            let offset = (i * 8) as usize;

            // Ensure we have enough bytes for this IOVec
            if offset + 8 > iovec_bytes.len() {
                return Err(WasixError::MemoryOutOfBounds);
            }

            // Read ptr (first 4 bytes, little-endian)
            let ptr = u32::from_le_bytes([
                iovec_bytes[offset],
                iovec_bytes[offset + 1],
                iovec_bytes[offset + 2],
                iovec_bytes[offset + 3],
            ]);

            // Read len (next 4 bytes, little-endian)
            let len = u32::from_le_bytes([
                iovec_bytes[offset + 4],
                iovec_bytes[offset + 5],
                iovec_bytes[offset + 6],
                iovec_bytes[offset + 7],
            ]);

            let iovec = IOVec::new(ptr, len);

            // Validate IOVec bounds against memory
            if iovec.len > 0 {
                // Check that the IOVec data pointer is valid
                let data_end = iovec
                    .ptr
                    .checked_add(iovec.len)
                    .ok_or(WasixError::MemoryOutOfBounds)?;

                // Verify the data region is within memory bounds
                let memory_view = memory.view(store);
                let memory_size = memory_view.data_size() as u32;
                if data_end > memory_size {
                    return Err(WasixError::MemoryOutOfBounds);
                }
            }

            iovecs.push(iovec);
        }

        Ok(iovecs)
    }

    /// Validate IOVec array against memory bounds
    pub fn validate_iovecs(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovecs: &[IOVec],
    ) -> Result<(), WasixError> {
        let memory_view = memory.view(store);
        let memory_size = memory_view.data_size() as u32;

        for (i, iovec) in iovecs.iter().enumerate() {
            // Skip empty IOVecs
            if iovec.len == 0 {
                continue;
            }

            // Check for null pointer with non-zero length
            if iovec.ptr == 0 {
                return Err(WasixError::MemoryOutOfBounds);
            }

            // Check for pointer overflow
            let end_ptr = iovec
                .ptr
                .checked_add(iovec.len)
                .ok_or(WasixError::MemoryOutOfBounds)?;

            // Check bounds against memory size
            if end_ptr > memory_size {
                return Err(WasixError::MemoryOutOfBounds);
            }

            // Check for reasonable IOVec size (1MB limit per IOVec)
            if iovec.len > 0x100000 {
                return Err(WasixError::EnvironmentError(format!(
                    "IOVec {} has length {} which exceeds 1MB limit",
                    i, iovec.len
                )));
            }
        }

        Ok(())
    }

    /// Read string data from WASM memory using IOVec pointers
    /// Returns the string content with UTF-8 validation
    pub fn read_string_from_iovec(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovec: &IOVec,
    ) -> Result<String, WasixError> {
        if iovec.len == 0 {
            return Ok(String::new());
        }

        // Read raw bytes from memory
        let bytes = self.read_bytes(memory, store, iovec.ptr, iovec.len)?;

        // Validate and convert UTF-8
        String::from_utf8(bytes).map_err(|e| {
            WasixError::EnvironmentError(format!(
                "Invalid UTF-8 string data at ptr 0x{:x}: {}",
                iovec.ptr, e
            ))
        })
    }

    /// Read string data from multiple IOVecs and concatenate
    /// This handles the case where a single string is split across multiple IOVec entries
    pub fn read_string_from_iovecs(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        iovecs: &[IOVec],
    ) -> Result<String, WasixError> {
        if iovecs.is_empty() {
            return Ok(String::new());
        }

        // Calculate total size
        let total_size: u32 = iovecs.iter().map(|iovec| iovec.len).sum();

        if total_size == 0 {
            return Ok(String::new());
        }

        // Read and concatenate all IOVec data
        let mut all_bytes = Vec::with_capacity(total_size as usize);

        for iovec in iovecs {
            if iovec.len > 0 {
                let bytes = self.read_bytes(memory, store, iovec.ptr, iovec.len)?;
                all_bytes.extend_from_slice(&bytes);
            }
        }

        // Validate and convert UTF-8
        String::from_utf8(all_bytes).map_err(|e| {
            WasixError::EnvironmentError(format!("Invalid UTF-8 string data in IOVec array: {}", e))
        })
    }

    /// Read and validate string data with length limits
    /// Provides additional safety checks for string operations
    pub fn read_validated_string(
        &self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        ptr: u32,
        len: u32,
        max_len: u32,
    ) -> Result<String, WasixError> {
        // Check length limits
        if len > max_len {
            return Err(WasixError::EnvironmentError(format!(
                "String length {} exceeds maximum allowed length {}",
                len, max_len
            )));
        }

        if len == 0 {
            return Ok(String::new());
        }

        // Read bytes from memory
        let bytes = self.read_bytes(memory, store, ptr, len)?;

        // Validate UTF-8 with detailed error information
        match String::from_utf8(bytes) {
            Ok(string) => {
                // Additional validation: check for null bytes (not allowed in WASIX strings)
                if string.contains('\0') {
                    return Err(WasixError::EnvironmentError(
                        "String contains null bytes which are not allowed in WASIX string operations".to_string()
                    ));
                }

                Ok(string)
            }
            Err(e) => {
                // Provide detailed UTF-8 error information
                let error_pos = e.utf8_error().valid_up_to();
                Err(WasixError::EnvironmentError(format!(
                    "Invalid UTF-8 at byte position {} in string at ptr 0x{:x}: {}",
                    error_pos,
                    ptr,
                    e.utf8_error()
                )))
            }
        }
    }

    /// Write string data to WASM memory and return IOVec
    /// This is useful for preparing string data for WASIX operations
    pub fn write_string_to_memory(
        &mut self,
        memory: &wasmer::Memory,
        store: &impl wasmer::AsStoreRef,
        content: &str,
    ) -> Result<IOVec, WasixError> {
        let bytes = content.as_bytes();
        let len = bytes.len() as u32;

        if len == 0 {
            return Ok(IOVec::empty());
        }

        // Allocate memory for the string
        let ptr = self.memory_manager.allocate_string(content)?.0;

        // Write string data to memory
        self.write_bytes(memory, store, ptr, bytes)?;

        Ok(IOVec::new(ptr, len))
    }

    /// Validate string encoding and content for WASIX operations
    pub fn validate_string_content(&self, content: &str) -> Result<(), WasixError> {
        // Check for reasonable string length (1MB limit)
        if content.len() > 0x100000 {
            return Err(WasixError::EnvironmentError(format!(
                "String length {} exceeds maximum allowed length of 1MB",
                content.len()
            )));
        }

        // Check for null bytes (not allowed in WASIX strings)
        if content.contains('\0') {
            return Err(WasixError::EnvironmentError(
                "String contains null bytes which are not allowed in WASIX operations".to_string(),
            ));
        }

        // Validate UTF-8 encoding (should already be valid for Rust strings, but double-check)
        if !content.is_ascii() {
            // For non-ASCII strings, ensure they're valid UTF-8
            match std::str::from_utf8(content.as_bytes()) {
                Ok(_) => {} // Valid UTF-8
                Err(e) => {
                    return Err(WasixError::EnvironmentError(format!(
                        "Invalid UTF-8 encoding in string: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }
}

impl WasixEnv {
    /// Create a new WASIX environment with enhanced I/O capabilities
    pub fn new() -> Result<Self, WasixError> {
        let stdout = Box::new(std::io::stdout());
        let stderr = Box::new(std::io::stderr());
        let env_vars = std::env::vars().collect();

        Ok(Self {
            stdout,
            stderr,
            env_vars,
        })
    }

    /// Builder pattern for creating WASIX environment
    pub fn builder(program_name: &str) -> WasixEnvBuilder {
        WasixEnvBuilder::new(program_name)
    }
}

/// Builder for WASIX environment configuration
pub struct WasixEnvBuilder {
    program_name: String,
    stdout: Option<Box<dyn std::io::Write + Send>>,
    stderr: Option<Box<dyn std::io::Write + Send>>,
    env_vars: std::collections::HashMap<String, String>,
}

impl WasixEnvBuilder {
    /// Create a new WASIX environment builder
    pub fn new(program_name: &str) -> Self {
        Self {
            program_name: program_name.to_string(),
            stdout: None,
            stderr: None,
            env_vars: std::env::vars().collect(),
        }
    }

    /// Set stdout handler
    pub fn stdout(mut self, stdout: Box<dyn std::io::Write + Send>) -> Self {
        self.stdout = Some(stdout);
        self
    }

    /// Set stderr handler
    pub fn stderr(mut self, stderr: Box<dyn std::io::Write + Send>) -> Self {
        self.stderr = Some(stderr);
        self
    }

    /// Build the WASIX environment
    pub fn build(self) -> Result<WasixEnv, WasixError> {
        let stdout = self.stdout.unwrap_or_else(|| Box::new(std::io::stdout()));
        let stderr = self.stderr.unwrap_or_else(|| Box::new(std::io::stderr()));

        Ok(WasixEnv {
            stdout,
            stderr,
            env_vars: self.env_vars,
        })
    }
}

impl FdTable {
    /// Create a new file descriptor table with standard descriptors
    pub fn new() -> Self {
        let mut table = Self {
            descriptors: std::collections::HashMap::new(),
            next_fd: 3, // Start after stdin(0), stdout(1), stderr(2)
        };

        // Add standard file descriptors
        table.descriptors.insert(
            0,
            FileDescriptor {
                fd: 0,
                fd_type: FileDescriptorType::Stdin,
                permissions: FilePermissions {
                    read: true,
                    write: false,
                    execute: false,
                },
            },
        );

        table.descriptors.insert(
            1,
            FileDescriptor {
                fd: 1,
                fd_type: FileDescriptorType::Stdout,
                permissions: FilePermissions {
                    read: false,
                    write: true,
                    execute: false,
                },
            },
        );

        table.descriptors.insert(
            2,
            FileDescriptor {
                fd: 2,
                fd_type: FileDescriptorType::Stderr,
                permissions: FilePermissions {
                    read: false,
                    write: true,
                    execute: false,
                },
            },
        );

        table
    }

    /// Check if a file descriptor is valid
    pub fn is_valid_fd(&self, fd: u32) -> bool {
        self.descriptors.contains_key(&fd)
    }

    /// Get file descriptor information
    pub fn get_fd(&self, fd: u32) -> Option<&FileDescriptor> {
        self.descriptors.get(&fd)
    }

    /// Add a new file descriptor
    pub fn add_fd(&mut self, fd_type: FileDescriptorType, permissions: FilePermissions) -> u32 {
        let fd = self.next_fd;
        self.next_fd += 1;

        self.descriptors.insert(
            fd,
            FileDescriptor {
                fd,
                fd_type,
                permissions,
            },
        );

        fd
    }
}

impl ProcessInfo {
    /// Create new process information
    pub fn new() -> Self {
        Self {
            pid: std::process::id(),
            ppid: 0, // Not implemented - would be actual parent PID in full WASIX implementation
            name: "beanstalk-program".to_string(),
            args: std::env::args().collect(),
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string()),
        }
    }
}

impl WasixMemoryManager {
    /// Create a new WASIX memory manager with enhanced allocation strategies
    pub fn new() -> Self {
        let layout = WasixMemoryLayout::new();

        Self {
            current_ptr: layout.heap_start, // Start allocations in heap area
            default_alignment: 8,           // 8-byte alignment for WASIX enhanced performance
            allocated_regions: Vec::new(),
            base_address: layout.heap_start,
            layout,
            max_memory_size: 0x1000000, // 16MB default maximum
            stats: WasixMemoryStats::new(),
        }
    }

    /// Create a WASIX memory manager with custom layout
    pub fn with_layout(layout: WasixMemoryLayout) -> Result<Self, WasixError> {
        layout.validate()?;

        Ok(Self {
            current_ptr: layout.heap_start,
            default_alignment: 8,
            allocated_regions: Vec::new(),
            base_address: layout.heap_start,
            layout,
            max_memory_size: 0x1000000,
            stats: WasixMemoryStats::new(),
        })
    }

    /// Allocate aligned memory with WASIX alignment requirements
    pub fn allocate(&mut self, size: u32, alignment: u32) -> Result<u32, WasixError> {
        // Validate alignment is power of 2
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Invalid alignment {} for WASIX memory allocation. Alignment must be a power of 2.",
                alignment
            )));
        }

        // Validate size is reasonable
        if size == 0 {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(
                "Cannot allocate zero bytes".to_string(),
            ));
        }

        if size > 0x100000 {
            // 1MB limit for individual allocations
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Allocation size {} exceeds maximum allowed size of 1MB",
                size
            )));
        }

        // Calculate aligned pointer
        let mask = alignment - 1;
        let aligned_ptr = (self.current_ptr + mask) & !mask;
        let alignment_adjustment = aligned_ptr - self.current_ptr;

        // Check if allocation would exceed memory limits
        let end_ptr = aligned_ptr.checked_add(size).ok_or_else(|| {
            self.stats.record_failure();
            WasixError::MemoryOutOfBounds
        })?;

        if end_ptr > self.max_memory_size {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Allocation would exceed maximum memory size of {} bytes",
                self.max_memory_size
            )));
        }

        // Check if we're allocating in the correct area
        if aligned_ptr < self.layout.heap_start
            || end_ptr > (self.layout.heap_start + self.layout.heap_size)
        {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(
                "Allocation outside of designated heap area".to_string(),
            ));
        }

        // Perform the allocation
        self.current_ptr = end_ptr;

        // Track the allocated region
        let region = MemoryRegion::new(aligned_ptr, size);
        self.allocated_regions.push(region);

        // Update statistics
        self.stats
            .record_allocation(size, size + alignment_adjustment);
        self.stats.update_peak_usage(self.total_allocated_size());

        Ok(aligned_ptr)
    }

    /// Allocate memory with default WASIX alignment (8 bytes)
    pub fn allocate_default(&mut self, size: u32) -> Result<u32, WasixError> {
        self.allocate(size, self.default_alignment)
    }

    /// Allocate string data in the designated string area with WASIX conventions
    pub fn allocate_string(&mut self, content: &str) -> Result<(u32, u32), WasixError> {
        let bytes = content.as_bytes();
        let size = bytes.len() as u32;

        // Allocate in string data area if possible, otherwise use heap
        let ptr = if size <= self.layout.string_data_size {
            self.allocate_in_area(
                size,
                1,
                self.layout.string_data_start,
                self.layout.string_data_size,
            )?
        } else {
            self.allocate(size, 1)? // Fallback to heap for large strings
        };

        Ok((ptr, size))
    }

    /// Allocate IOVec array in the designated IOVec area with proper alignment
    pub fn allocate_iovec_array(&mut self, count: u32) -> Result<u32, WasixError> {
        let size = count * 8; // Each IOVec is 8 bytes (ptr + len)

        // Allocate in IOVec area if possible, otherwise use heap
        let ptr = if size <= self.layout.iovec_area_size {
            self.allocate_in_area(
                size,
                8,
                self.layout.iovec_area_start,
                self.layout.iovec_area_size,
            )?
        } else {
            self.allocate(size, 8)? // Fallback to heap for large IOVec arrays
        };

        Ok(ptr)
    }

    /// Allocate memory in a specific area with bounds checking
    fn allocate_in_area(
        &mut self,
        size: u32,
        alignment: u32,
        area_start: u32,
        area_size: u32,
    ) -> Result<u32, WasixError> {
        // Validate alignment is power of 2
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Invalid alignment {} for WASIX memory allocation. Alignment must be a power of 2.",
                alignment
            )));
        }

        // Validate size is reasonable
        if size == 0 {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(
                "Cannot allocate zero bytes".to_string(),
            ));
        }

        if size > area_size {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Allocation size {} exceeds area size of {} bytes",
                size, area_size
            )));
        }

        // FIXED: Track current allocation pointer within the area
        // Find the next available address in this area by checking existing allocations
        let mut current_ptr = area_start;

        // Find the highest allocated address in this area
        for region in &self.allocated_regions {
            if region.ptr >= area_start && region.ptr < (area_start + area_size) {
                let region_end = region.ptr + region.size;
                if region_end > current_ptr {
                    current_ptr = region_end;
                }
            }
        }

        // Apply alignment to the current pointer
        let mask = alignment - 1;
        let aligned_ptr = (current_ptr + mask) & !mask;

        // Check if allocation fits in the area
        let end_ptr = aligned_ptr.checked_add(size).ok_or_else(|| {
            self.stats.record_failure();
            WasixError::MemoryOutOfBounds
        })?;

        if end_ptr > (area_start + area_size) {
            self.stats.record_failure();
            return Err(WasixError::EnvironmentError(format!(
                "Allocation would exceed area bounds: trying to allocate {} bytes at 0x{:x}, but area ends at 0x{:x}",
                size,
                aligned_ptr,
                area_start + area_size
            )));
        }

        // Track the allocated region
        let region = MemoryRegion::new(aligned_ptr, size);
        self.allocated_regions.push(region);

        // Update statistics
        self.stats.record_allocation(size, size);

        Ok(aligned_ptr)
    }

    /// Get all allocated memory regions
    pub fn get_allocated_regions(&self) -> &[MemoryRegion] {
        &self.allocated_regions
    }

    /// Get total allocated memory size
    pub fn total_allocated_size(&self) -> u32 {
        self.current_ptr - self.base_address
    }

    /// Get memory layout information
    pub fn get_layout(&self) -> &WasixMemoryLayout {
        &self.layout
    }

    /// Get memory allocation statistics
    pub fn get_stats(&self) -> &WasixMemoryStats {
        &self.stats
    }

    /// Check if an address is in a valid allocated region
    pub fn is_valid_address(&self, addr: u32, size: u32) -> bool {
        let end_addr = addr.saturating_add(size);

        for region in &self.allocated_regions {
            if addr >= region.ptr && end_addr <= (region.ptr + region.size) {
                return true;
            }
        }

        false
    }

    /// Reset the allocator (for testing or reuse)
    pub fn reset(&mut self) {
        self.current_ptr = self.base_address;
        self.allocated_regions.clear();
        self.stats = WasixMemoryStats::new();
    }

    /// Set maximum memory size
    pub fn set_max_memory_size(&mut self, max_size: u32) {
        self.max_memory_size = max_size;
    }

    /// Get current memory usage as a percentage of maximum
    pub fn memory_usage_percentage(&self) -> f32 {
        if self.max_memory_size == 0 {
            return 0.0;
        }

        (self.total_allocated_size() as f32 / self.max_memory_size as f32) * 100.0
    }
}
/// Convert WasixError to CompileError with detailed error messages and context
impl From<WasixError> for CompileError {
    fn from(error: WasixError) -> Self {
        match error {
            // Basic error formats
            WasixError::InvalidFileDescriptor(fd) => CompileError::compiler_error(&format!(
                "WASIX operation failed: Invalid file descriptor {}. WASIX supports stdout (1), stderr (2), and stdin (0). This may indicate a bug in WASIX function call generation.",
                fd
            )),
            WasixError::MemoryOutOfBounds => CompileError::compiler_error(
                "WASIX operation failed: Memory access out of bounds. This indicates a bug in WASM memory management or pointer arithmetic in WASIX operations.",
            ),
            WasixError::InvalidArgumentCount => CompileError::compiler_error(
                "WASIX operation failed: Invalid argument count for WASIX function call. This indicates a bug in WASIX function signature handling.",
            ),
            WasixError::EnvironmentError(msg) => CompileError::compiler_error(&format!(
                "WASIX environment error: {}. This may indicate missing WASIX runtime support or configuration issues.",
                msg
            )),
            WasixError::NativeFunctionNotFound(name) => CompileError::compiler_error(&format!(
                "Native WASIX function '{}' not found in registry. This indicates a bug in WASIX function registration or lookup.",
                name
            )),

            // Enhanced error formats with detailed context
            WasixError::InvalidFileDescriptorWithContext {
                fd,
                function,
                context,
            } => CompileError::compiler_error(&format!(
                "WASIX function '{}' failed: Invalid file descriptor {} ({}). WASIX supports stdout (1), stderr (2), and stdin (0). Check WASIX function call generation.",
                function, fd, context
            )),
            WasixError::MemoryOutOfBoundsWithContext {
                address,
                size,
                function,
                context,
            } => CompileError::compiler_error(&format!(
                "WASIX function '{}' failed: Memory access out of bounds at address 0x{:x} (size: {} bytes). Context: {}. This indicates a bug in WASM memory layout or pointer handling.",
                function, address, size, context
            )),
            WasixError::InvalidArgumentCountWithContext {
                expected,
                actual,
                function,
            } => CompileError::compiler_error(&format!(
                "WASIX function '{}' called with wrong argument count: expected {}, got {}. This indicates a bug in WASIX function call generation.",
                function, expected, actual
            )),
            WasixError::EnvironmentErrorWithContext {
                message,
                function,
                suggestion,
            } => {
                let mut error_msg = format!("WASIX function '{}' failed: {}", function, message);
                if let Some(hint) = suggestion {
                    error_msg.push_str(&format!(" Suggestion: {}", hint));
                }
                error_msg.push_str(
                    " This may indicate missing WASIX runtime support or configuration issues.",
                );
                CompileError::compiler_error(&error_msg)
            }
            WasixError::NativeFunctionNotFoundWithContext {
                function,
                available_functions,
            } => {
                let mut error_msg = format!(
                    "Native WASIX function '{}' not found in registry.",
                    function
                );
                if available_functions.is_empty() {
                    error_msg.push_str(" No WASIX functions are currently registered.");
                } else {
                    error_msg.push_str(&format!(
                        " Available functions: {}.",
                        available_functions.join(", ")
                    ));
                }
                error_msg.push_str(" This indicates a bug in WASIX function registration.");
                CompileError::compiler_error(&error_msg)
            }

            // Import resolution and runtime errors
            WasixError::ImportResolutionError {
                module,
                function,
                reason,
                suggestion,
            } => CompileError::compiler_error(&format!(
                "Failed to resolve WASIX import '{}:{}': {}. Suggestion: {}. This indicates missing WASIX runtime support or incorrect import generation.",
                module, function, reason, suggestion
            )),
            WasixError::AllocationError {
                requested_size,
                available_size,
                function,
                suggestion,
            } => CompileError::compiler_error(&format!(
                "Memory allocation failed in WASIX function '{}': requested {} bytes, only {} bytes available. Suggestion: {}. This may indicate insufficient WASM memory configuration.",
                function, requested_size, available_size, suggestion
            )),
            WasixError::IOVecError {
                iovec_index,
                ptr,
                len,
                reason,
            } => CompileError::compiler_error(&format!(
                "IOVec validation failed at index {}: ptr=0x{:x}, len={}, reason: {}. This indicates a bug in IOVec handling or memory layout.",
                iovec_index, ptr, len, reason
            )),
            WasixError::StringEncodingError {
                position,
                encoding,
                context,
            } => CompileError::compiler_error(&format!(
                "String encoding error at position {}: invalid {} encoding. Context: {}. This indicates a bug in string handling or memory management.",
                position, encoding, context
            )),
            WasixError::ConfigurationError {
                setting,
                value,
                expected,
                suggestion,
            } => CompileError::compiler_error(&format!(
                "WASIX configuration error: setting '{}' has value '{}', expected '{}'. Suggestion: {}. This indicates incorrect WASIX runtime configuration.",
                setting, value, expected, suggestion
            )),
        }
    }
}

/// WASIX import resolution error handling and diagnostics
impl WasixFunctionRegistry {
    /// Detect and handle WASIX import resolution failures
    /// This function checks if WASIX imports can be resolved and provides helpful error messages
    pub fn validate_import_resolution(
        &self,
        available_imports: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        >,
    ) -> Result<(), WasixError> {
        for (beanstalk_name, wasix_function) in &self.functions {
            let module_name = &wasix_function.module;
            let function_name = &wasix_function.name;

            // Check if the module is available
            if !available_imports.contains_key(module_name) {
                return Err(WasixError::import_resolution_error(
                    module_name,
                    function_name,
                    &format!("WASIX module '{}' not available in runtime", module_name),
                    &self.get_module_availability_suggestion(module_name),
                ));
            }

            // Check if the function is available in the module
            let module_functions = &available_imports[module_name];
            if !module_functions.contains_key(function_name) {
                let available_functions: Vec<String> = module_functions.keys().cloned().collect();
                return Err(WasixError::import_resolution_error(
                    module_name,
                    function_name,
                    &format!(
                        "WASIX function '{}' not available in module '{}'",
                        function_name, module_name
                    ),
                    &self.get_function_availability_suggestion(function_name, &available_functions),
                ));
            }
        }

        Ok(())
    }

    /// Get suggestion for missing WASIX module
    fn get_module_availability_suggestion(&self, module_name: &str) -> String {
        match module_name {
            "wasix_32v1" => {
                "Ensure you're using a WASIX-compatible runtime like Wasmer with WASIX support enabled. Try: wasmer run --enable-wasix your_program.wasm".to_string()
            },
            "wasix_64v1" => {
                "Ensure you're using a WASIX-compatible runtime with 64-bit support. Try: wasmer run --enable-wasix --arch=x86_64 your_program.wasm".to_string()
            },
            "wasix_snapshot_preview1" => {
                "This is a preview WASIX module. Ensure your runtime supports WASIX preview features.".to_string()
            },
            "wasi_snapshot_preview1" => {
                "This program uses WASI compatibility mode. Most WASM runtimes support this. Try: wasmer run your_program.wasm".to_string()
            },
            "wasi_unstable" => {
                "This program uses legacy WASI. Consider upgrading to WASIX or use a runtime with legacy WASI support.".to_string()
            },
            _ => {
                format!("Unknown WASIX module '{}'. Check your runtime's WASIX module support.", module_name)
            }
        }
    }

    /// Get suggestion for missing WASIX function
    fn get_function_availability_suggestion(
        &self,
        function_name: &str,
        available_functions: &[String],
    ) -> String {
        // Find similar function names
        let similar_functions: Vec<&String> = available_functions
            .iter()
            .filter(|f| {
                // Simple similarity check: same prefix or contains the function name
                f.starts_with(&function_name[..function_name.len().min(3)])
                    || f.contains(function_name)
                    || function_name.contains(f.as_str())
            })
            .collect();

        if !similar_functions.is_empty() {
            format!(
                "Did you mean one of: {}? Check the WASIX specification for correct function names.",
                similar_functions
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else if available_functions.is_empty() {
            "No functions are available in this WASIX module. Check your runtime's WASIX support."
                .to_string()
        } else {
            format!(
                "Available functions in this module: {}. Check the WASIX specification for correct function names.",
                available_functions.join(", ")
            )
        }
    }

    /// Check if a specific WASIX import is available
    pub fn check_import_availability(
        &self,
        beanstalk_name: &str,
        runtime_modules: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        >,
    ) -> Result<(), WasixError> {
        let wasix_function = self.functions.get(beanstalk_name).ok_or_else(|| {
            WasixError::function_not_found_with_context(
                beanstalk_name,
                self.functions.keys().cloned().collect(),
            )
        })?;

        let module_name = &wasix_function.module;
        let function_name = &wasix_function.name;

        // Check module availability
        let module_functions = runtime_modules.get(module_name).ok_or_else(|| {
            WasixError::import_resolution_error(
                module_name,
                function_name,
                &format!("WASIX module '{}' not supported by runtime", module_name),
                &self.get_module_availability_suggestion(module_name),
            )
        })?;

        // Check function availability
        if !module_functions.contains_key(function_name) {
            let available_functions: Vec<String> = module_functions.keys().cloned().collect();
            return Err(WasixError::import_resolution_error(
                module_name,
                function_name,
                &format!(
                    "Function '{}' not available in WASIX module '{}'",
                    function_name, module_name
                ),
                &self.get_function_availability_suggestion(function_name, &available_functions),
            ));
        }

        Ok(())
    }

    /// Generate comprehensive import resolution diagnostics
    pub fn generate_import_diagnostics(
        &self,
        runtime_modules: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, String>,
        >,
    ) -> ImportResolutionDiagnostics {
        let mut diagnostics = ImportResolutionDiagnostics::new();

        for (beanstalk_name, wasix_function) in &self.functions {
            let module_name = &wasix_function.module;
            let function_name = &wasix_function.name;

            // Check module availability
            if !runtime_modules.contains_key(module_name) {
                diagnostics.add_missing_module(
                    beanstalk_name.clone(),
                    module_name.clone(),
                    self.get_module_availability_suggestion(module_name),
                );
                continue;
            }

            // Check function availability
            let module_functions = &runtime_modules[module_name];
            if !module_functions.contains_key(function_name) {
                let available_functions: Vec<String> = module_functions.keys().cloned().collect();
                diagnostics.add_missing_function(
                    beanstalk_name.clone(),
                    module_name.clone(),
                    function_name.clone(),
                    available_functions,
                    self.get_function_availability_suggestion(
                        function_name,
                        &module_functions.keys().cloned().collect::<Vec<_>>(),
                    ),
                );
            } else {
                diagnostics.add_available_function(
                    beanstalk_name.clone(),
                    module_name.clone(),
                    function_name.clone(),
                );
            }
        }

        diagnostics
    }
}

/// Diagnostics for WASIX import resolution
#[derive(Debug, Clone)]
pub struct ImportResolutionDiagnostics {
    /// Successfully resolved imports
    pub available_imports: Vec<AvailableImport>,
    /// Missing WASIX modules
    pub missing_modules: Vec<MissingModule>,
    /// Missing WASIX functions
    pub missing_functions: Vec<MissingFunction>,
}

/// Information about an available WASIX import
#[derive(Debug, Clone)]
pub struct AvailableImport {
    /// Beanstalk function name
    pub beanstalk_name: String,
    /// WASIX module name
    pub module_name: String,
    /// WASIX function name
    pub function_name: String,
}

/// Information about a missing WASIX module
#[derive(Debug, Clone)]
pub struct MissingModule {
    /// Beanstalk function name that needs this module
    pub beanstalk_name: String,
    /// Missing WASIX module name
    pub module_name: String,
    /// Suggestion for resolving the missing module
    pub suggestion: String,
}

/// Information about a missing WASIX function
#[derive(Debug, Clone)]
pub struct MissingFunction {
    /// Beanstalk function name
    pub beanstalk_name: String,
    /// WASIX module name
    pub module_name: String,
    /// Missing WASIX function name
    pub function_name: String,
    /// Available functions in the module
    pub available_functions: Vec<String>,
    /// Suggestion for resolving the missing function
    pub suggestion: String,
}

impl ImportResolutionDiagnostics {
    /// Create new import resolution diagnostics
    pub fn new() -> Self {
        Self {
            available_imports: Vec::new(),
            missing_modules: Vec::new(),
            missing_functions: Vec::new(),
        }
    }

    /// Add an available import
    pub fn add_available_function(
        &mut self,
        beanstalk_name: String,
        module_name: String,
        function_name: String,
    ) {
        self.available_imports.push(AvailableImport {
            beanstalk_name,
            module_name,
            function_name,
        });
    }

    /// Add a missing module
    pub fn add_missing_module(
        &mut self,
        beanstalk_name: String,
        module_name: String,
        suggestion: String,
    ) {
        self.missing_modules.push(MissingModule {
            beanstalk_name,
            module_name,
            suggestion,
        });
    }

    /// Add a missing function
    pub fn add_missing_function(
        &mut self,
        beanstalk_name: String,
        module_name: String,
        function_name: String,
        available_functions: Vec<String>,
        suggestion: String,
    ) {
        self.missing_functions.push(MissingFunction {
            beanstalk_name,
            module_name,
            function_name,
            available_functions,
            suggestion,
        });
    }

    /// Check if all imports are available
    pub fn all_imports_available(&self) -> bool {
        self.missing_modules.is_empty() && self.missing_functions.is_empty()
    }

    /// Get the number of missing imports
    pub fn missing_import_count(&self) -> usize {
        self.missing_modules.len() + self.missing_functions.len()
    }

    /// Generate a summary report
    pub fn generate_summary(&self) -> String {
        if self.all_imports_available() {
            format!(
                "All {} WASIX imports are available and can be resolved.",
                self.available_imports.len()
            )
        } else {
            let mut summary = format!(
                "WASIX import resolution: {} available, {} missing ({} modules, {} functions).\n",
                self.available_imports.len(),
                self.missing_import_count(),
                self.missing_modules.len(),
                self.missing_functions.len()
            );

            if !self.missing_modules.is_empty() {
                summary.push_str("\nMissing modules:\n");
                for missing in &self.missing_modules {
                    summary.push_str(&format!(
                        "  - {} needs module '{}': {}\n",
                        missing.beanstalk_name, missing.module_name, missing.suggestion
                    ));
                }
            }

            if !self.missing_functions.is_empty() {
                summary.push_str("\nMissing functions:\n");
                for missing in &self.missing_functions {
                    summary.push_str(&format!(
                        "  - {} needs '{}:{}': {}\n",
                        missing.beanstalk_name,
                        missing.module_name,
                        missing.function_name,
                        missing.suggestion
                    ));
                }
            }

            summary
        }
    }
}

impl Default for ImportResolutionDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced memory allocation error handling for WASIX operations
impl WasixMemoryManager {
    /// Allocate memory with comprehensive error handling and diagnostics
    pub fn allocate_with_error_handling(
        &mut self,
        size: u32,
        alignment: u32,
        context: &str,
    ) -> Result<u32, WasixError> {
        // Validate allocation request
        self.validate_allocation_request(size, alignment, context)?;

        // Check available memory
        let available_memory = self.get_available_memory();
        if size > available_memory {
            return Err(WasixError::allocation_error(
                size,
                available_memory,
                context,
                &self.get_memory_increase_suggestion(size, available_memory),
            ));
        }

        // Perform the allocation in the heap area
        let heap_start = self.layout.heap_start;
        let heap_size = self.layout.heap_size;
        match self.allocate_in_area(size, alignment, heap_start, heap_size) {
            Ok(ptr) => {
                // Update peak usage tracking
                let current_usage = self.total_allocated_size();
                self.stats.update_peak_usage(current_usage);
                Ok(ptr)
            }
            Err(e) => {
                self.stats.record_failure();
                Err(self.enhance_allocation_error(e, size, alignment, context))
            }
        }
    }

    /// Validate allocation request parameters
    fn validate_allocation_request(
        &self,
        size: u32,
        alignment: u32,
        context: &str,
    ) -> Result<(), WasixError> {
        // Check for zero-size allocation
        if size == 0 {
            return Err(WasixError::allocation_error(
                size,
                0,
                context,
                "Zero-size allocations are not allowed. Check your string length calculations.",
            ));
        }

        // Check for excessive size (prevent memory exhaustion)
        if size > 0x10000000 {
            // 256MB limit
            return Err(WasixError::allocation_error(
                size,
                0x10000000,
                context,
                "Allocation size exceeds 256MB limit. This may indicate a bug in size calculation or an attempt to allocate excessive memory.",
            ));
        }

        // Validate alignment
        if alignment == 0 || (alignment & (alignment - 1)) != 0 {
            return Err(WasixError::configuration_error(
                "memory_alignment",
                &alignment.to_string(),
                "power of 2 (1, 2, 4, 8, 16, etc.)",
                "Use a valid alignment value like 8 for WASIX operations.",
            ));
        }

        // Check if alignment is reasonable (not too large)
        if alignment > 4096 {
            return Err(WasixError::configuration_error(
                "memory_alignment",
                &alignment.to_string(),
                "≤ 4096 bytes",
                "Excessive alignment may waste memory. Consider using 8 or 16 byte alignment for WASIX.",
            ));
        }

        Ok(())
    }

    /// Get available memory for allocation
    fn get_available_memory(&self) -> u32 {
        if self.max_memory_size > self.total_allocated_size() {
            self.max_memory_size - self.total_allocated_size()
        } else {
            0
        }
    }

    /// Generate suggestion for increasing memory limits
    fn get_memory_increase_suggestion(&self, requested: u32, available: u32) -> String {
        let current_usage = self.total_allocated_size();
        let needed_total = current_usage + requested;
        let suggested_limit = ((needed_total as f32 * 1.5) as u32).max(0x1000000); // At least 16MB

        format!(
            "Increase WASM memory limit to at least {} bytes (currently {}). Current usage: {} bytes, requested: {} bytes, available: {} bytes. Try setting --max-memory={} when running with Wasmer.",
            suggested_limit,
            self.max_memory_size,
            current_usage,
            requested,
            available,
            suggested_limit
        )
    }

    /// Enhance allocation error with context and suggestions
    fn enhance_allocation_error(
        &self,
        original_error: WasixError,
        size: u32,
        _alignment: u32,
        context: &str,
    ) -> WasixError {
        match original_error {
            WasixError::MemoryOutOfBounds => WasixError::allocation_error(
                size,
                self.get_available_memory(),
                context,
                &format!(
                    "Memory allocation failed due to insufficient space. {}",
                    self.get_memory_increase_suggestion(size, self.get_available_memory())
                ),
            ),
            WasixError::EnvironmentError(msg) => WasixError::allocation_error(
                size,
                self.get_available_memory(),
                context,
                &format!(
                    "Memory allocation failed: {}. Check WASM memory configuration and limits.",
                    msg
                ),
            ),
            _ => original_error,
        }
    }

    /// Check memory health and generate warnings
    pub fn check_memory_health(&self) -> MemoryHealthReport {
        let mut report = MemoryHealthReport::new();
        let usage_percentage = self.memory_usage_percentage();
        let fragmentation = self.calculate_fragmentation();

        // Check memory usage levels
        if usage_percentage > 90.0 {
            report.add_warning(
                MemoryWarningLevel::Critical,
                format!(
                    "Memory usage is {}% of maximum. Consider increasing memory limits or optimizing allocations.",
                    usage_percentage as u32
                ),
            );
        } else if usage_percentage > 75.0 {
            report.add_warning(
                MemoryWarningLevel::High,
                format!(
                    "Memory usage is {}% of maximum. Monitor memory usage closely.",
                    usage_percentage as u32
                ),
            );
        } else if usage_percentage > 50.0 {
            report.add_warning(
                MemoryWarningLevel::Medium,
                format!(
                    "Memory usage is {}% of maximum. Memory usage is moderate.",
                    usage_percentage as u32
                ),
            );
        }

        // Check fragmentation
        if fragmentation > 0.3 {
            report.add_warning(
                MemoryWarningLevel::Medium,
                format!(
                    "Memory fragmentation is {:.1}%. Consider memory compaction or larger initial allocations.",
                    fragmentation * 100.0
                ),
            );
        }

        // Check allocation failure rate
        let failure_rate = if self.stats.total_allocations > 0 {
            self.stats.allocation_failures as f32 / self.stats.total_allocations as f32
        } else {
            0.0
        };

        if failure_rate > 0.1 {
            report.add_warning(
                MemoryWarningLevel::High,
                format!(
                    "Allocation failure rate is {:.1}%. Check memory limits and allocation patterns.",
                    failure_rate * 100.0
                ),
            );
        }

        report
    }

    /// Calculate memory fragmentation as a ratio
    fn calculate_fragmentation(&self) -> f32 {
        if self.allocated_regions.is_empty() {
            return 0.0;
        }

        let total_allocated: u32 = self.allocated_regions.iter().map(|r| r.size).sum();
        let total_span = self.total_allocated_size();

        if total_span == 0 {
            return 0.0;
        }

        1.0 - (total_allocated as f32 / total_span as f32)
    }

    /// Generate memory allocation diagnostics
    pub fn generate_allocation_diagnostics(&self) -> AllocationDiagnostics {
        AllocationDiagnostics {
            total_allocations: self.stats.total_allocations,
            total_bytes_allocated: self.stats.total_bytes_allocated,
            peak_memory_usage: self.stats.peak_memory_usage,
            current_memory_usage: self.total_allocated_size(),
            memory_usage_percentage: self.memory_usage_percentage(),
            fragmentation_ratio: self.calculate_fragmentation(),
            allocation_failures: self.stats.allocation_failures,
            alignment_adjustments: self.stats.alignment_adjustments,
            allocated_regions_count: self.allocated_regions.len(),
            max_memory_size: self.max_memory_size,
            available_memory: self.get_available_memory(),
        }
    }

    /// Suggest memory configuration optimizations
    pub fn suggest_memory_optimizations(&self) -> Vec<MemoryOptimizationSuggestion> {
        let mut suggestions = Vec::new();
        let diagnostics = self.generate_allocation_diagnostics();

        // Suggest memory limit increases
        if diagnostics.memory_usage_percentage > 80.0 {
            suggestions.push(MemoryOptimizationSuggestion {
                category: OptimizationCategory::MemoryLimit,
                priority: OptimizationPriority::High,
                description: format!(
                    "Increase WASM memory limit from {} to {} bytes to prevent allocation failures.",
                    self.max_memory_size,
                    (self.max_memory_size as f32 * 1.5) as u32
                ),
                implementation: "Use --max-memory flag with Wasmer or configure memory limits in your WASM runtime.".to_string(),
            });
        }

        // Suggest fragmentation reduction
        if diagnostics.fragmentation_ratio > 0.2 {
            suggestions.push(MemoryOptimizationSuggestion {
                category: OptimizationCategory::Fragmentation,
                priority: OptimizationPriority::Medium,
                description: format!(
                    "Reduce memory fragmentation (currently {:.1}%) by using larger initial allocations.",
                    diagnostics.fragmentation_ratio * 100.0
                ),
                implementation: "Allocate larger blocks initially and sub-allocate from them, or implement memory pooling.".to_string(),
            });
        }

        // Suggest alignment optimization
        if diagnostics.alignment_adjustments > diagnostics.total_allocations / 4 {
            suggestions.push(MemoryOptimizationSuggestion {
                category: OptimizationCategory::Alignment,
                priority: OptimizationPriority::Low,
                description: format!(
                    "Optimize memory alignment to reduce {} alignment adjustments.",
                    diagnostics.alignment_adjustments
                ),
                implementation:
                    "Use consistent alignment requirements (8 or 16 bytes) for all allocations."
                        .to_string(),
            });
        }

        suggestions
    }
}

/// Memory health report for diagnostics
#[derive(Debug, Clone)]
pub struct MemoryHealthReport {
    /// Memory warnings by severity
    pub warnings: Vec<MemoryWarning>,
}

/// Memory warning information
#[derive(Debug, Clone)]
pub struct MemoryWarning {
    /// Warning severity level
    pub level: MemoryWarningLevel,
    /// Warning message
    pub message: String,
}

/// Memory warning severity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryWarningLevel {
    /// Low priority warning
    Low,
    /// Medium priority warning
    Medium,
    /// High priority warning
    High,
    /// Critical warning requiring immediate attention
    Critical,
}

impl MemoryHealthReport {
    /// Create a new memory health report
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
        }
    }

    /// Add a warning to the report
    pub fn add_warning(&mut self, level: MemoryWarningLevel, message: String) {
        self.warnings.push(MemoryWarning { level, message });
    }

    /// Check if there are any critical warnings
    pub fn has_critical_warnings(&self) -> bool {
        self.warnings
            .iter()
            .any(|w| w.level == MemoryWarningLevel::Critical)
    }

    /// Get warnings by level
    pub fn get_warnings_by_level(&self, level: MemoryWarningLevel) -> Vec<&MemoryWarning> {
        self.warnings.iter().filter(|w| w.level == level).collect()
    }
}

impl Default for MemoryHealthReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Comprehensive allocation diagnostics
#[derive(Debug, Clone)]
pub struct AllocationDiagnostics {
    /// Total number of allocations performed
    pub total_allocations: u64,
    /// Total bytes allocated across all allocations
    pub total_bytes_allocated: u64,
    /// Peak memory usage reached
    pub peak_memory_usage: u32,
    /// Current memory usage
    pub current_memory_usage: u32,
    /// Memory usage as percentage of maximum
    pub memory_usage_percentage: f32,
    /// Memory fragmentation ratio (0.0 = no fragmentation, 1.0 = maximum fragmentation)
    pub fragmentation_ratio: f32,
    /// Number of allocation failures
    pub allocation_failures: u64,
    /// Number of alignment adjustments made
    pub alignment_adjustments: u64,
    /// Number of allocated regions
    pub allocated_regions_count: usize,
    /// Maximum memory size configured
    pub max_memory_size: u32,
    /// Available memory for allocation
    pub available_memory: u32,
}

/// Memory optimization suggestion
#[derive(Debug, Clone)]
pub struct MemoryOptimizationSuggestion {
    /// Category of optimization
    pub category: OptimizationCategory,
    /// Priority level
    pub priority: OptimizationPriority,
    /// Description of the optimization
    pub description: String,
    /// Implementation guidance
    pub implementation: String,
}

/// Categories of memory optimization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationCategory {
    /// Memory limit configuration
    MemoryLimit,
    /// Memory fragmentation reduction
    Fragmentation,
    /// Memory alignment optimization
    Alignment,
    /// Allocation pattern optimization
    AllocationPattern,
}

/// Priority levels for optimizations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizationPriority {
    /// Low priority optimization
    Low,
    /// Medium priority optimization
    Medium,
    /// High priority optimization
    High,
    /// Critical optimization required
    Critical,
}

// Utility functions for WASIX operations will be added here when JIT runtime integration is implemented
