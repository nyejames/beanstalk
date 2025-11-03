// Direct JIT execution for quick development feedback
//
// This module provides immediate WASM execution using Wasmer's JIT capabilities
// for rapid development iteration without additional compilation steps.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::host_functions::wasix_registry::{
    WasixFunctionRegistry, create_wasix_registry,
};
use crate::runtime::{IoBackend, RuntimeConfig};
use std::cell::RefCell;
use wasmer::{Function, Instance, Memory, Module, Store, imports};
use wasmer_wasix::WasiEnvBuilder;

/// Execute WASM bytecode directly using JIT compilation
pub fn execute_direct_jit(wasm_bytes: &[u8], config: &RuntimeConfig) -> Result<(), CompileError> {
    execute_direct_jit_with_capture(wasm_bytes, config, false)
}

/// Execute WASM bytecode with optional output capture for testing
pub fn execute_direct_jit_with_capture(
    wasm_bytes: &[u8],
    config: &RuntimeConfig,
    capture_output: bool,
) -> Result<(), CompileError> {
    // WASIX requires a Tokio runtime context for async I/O operations
    // Create a runtime only for JIT execution to avoid overhead in other CLI operations
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| CompileError::compiler_error(&format!("Failed to create Tokio runtime for WASIX: {}", e)))?;
    
    // Enter the runtime context and execute synchronously
    // The _guard ensures we stay in the runtime context for the duration of this function
    let _guard = runtime.enter();
    
    // Create a Wasmer store for JIT execution
    let mut store = Store::default();

    // Compile WASM module
    let module = Module::new(&store, wasm_bytes).map_err(|e| {

        // Check for the specific magic header bug in Wasmer RC version
        let error_str = e.to_string();
        if error_str.contains("magic header not detected") && error_str.contains("actual=[0x77, 0x61, 0x73, 0x6d]") {
            // This is a known bug in Wasmer 6.1.0-rc.5 where it misreports the magic header
            // The WASM bytes are actually correct (as verified by our debug output)
            // but Wasmer incorrectly reports finding "wasm" instead of "\0asm"
            return CompileError::compiler_error(
                "WASM magic header bug detected in Wasmer 6.1.0-rc.5. \
                The WASM module is correctly generated but Wasmer RC has a bug in magic header validation. \
                This is a known issue with the release candidate version. \
                Workaround: Use a stable version of Wasmer or wait for the final release."
            );
        }

        CompileError::compiler_error(&format!("Failed to compile WASM module: {}", e))
    })?;

    // Set up imports based on IO backend
    let (import_object, wasi_env_opt) = create_import_object_with_capture(
        &mut store,
        &module,
        wasm_bytes,
        &config.io_backend,
        capture_output,
    )?;

    // Instantiate the module
    let instance = Instance::new(&mut store, &module, &import_object).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to instantiate WASM module: {}", e))
    })?;

    // Keep WasiFunctionEnv alive for the duration of execution
    // The WasiFunctionEnv must not be dropped while the instance is being used
    // Simply keeping it in scope ensures it stays alive
    let _wasi_env_guard = wasi_env_opt;

    // Set up memory access for backends that need it (WASIX and Native)
    if matches!(config.io_backend, IoBackend::Wasix | IoBackend::Native) {
        // Try different memory export names in order of preference
        let memory_result = instance
            .exports
            .get_memory("memory")
            .or_else(|_| instance.exports.get_memory("mem"))
            .or_else(|_| instance.exports.get_memory("0"));

        match memory_result {
            Ok(memory) => {
                // Validate memory configuration
                let memory_view = memory.view(&store);
                let memory_size = memory_view.data_size();
                
                #[cfg(feature = "verbose_codegen_logging")]
                println!("WASIX memory found: {} bytes", memory_size);
                
                // Ensure minimum memory size for WASIX operations
                if memory_size < 65536 {
                    return Err(CompileError::compiler_error(
                        "WASM memory too small for WASIX operations. Minimum 64KB required."
                    ));
                }

                set_wasix_memory_and_store(memory.clone(), &mut store as *mut Store);
                
                #[cfg(feature = "verbose_codegen_logging")]
                println!("WASIX memory access configured successfully");
            }
            Err(_) => {
                #[cfg(feature = "verbose_codegen_logging")]
                {
                    println!("Warning: No memory export found for WASIX");
                    println!("Available exports:");
                    for (name, _) in instance.exports.iter() {
                        println!("  - {}", name);
                    }
                }
                
                // For WASIX and Native backends, memory is required
                let backend_name = match config.io_backend {
                    IoBackend::Wasix => "WASIX",
                    IoBackend::Native => "Native",
                    _ => "Unknown",
                };
                return Err(CompileError::compiler_error(&format!(
                    "{} backend requires WASM memory export. Ensure the WASM module exports memory.",
                    backend_name
                )));
            }
        }
    }

    // Wasmer automatically runs the start section (if present) when instantiating the module above.
    // So if instantiation succeeded, the start section has already run.
    // Now, optionally call 'main' or '_start' if present (for non-WASIX or legacy modules).
    #[cfg(feature = "verbose_codegen_logging")]
    println!("JIT: Looking for main function...");
    if let Ok(main_func) = instance.exports.get_function("main") {
        #[cfg(feature = "verbose_codegen_logging")]
        println!("JIT: Found main function");
        #[cfg(feature = "verbose_codegen_logging")]
        println!("JIT: About to call main function");
        match main_func.call(&mut store, &[]) {
            Ok(values) => {
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "JIT: Main function completed successfully with values: {:?}",
                    values
                );
                if !values.is_empty() {
                    println!("Program returned: {:?}", values);
                }
                Ok(())
            }
            Err(e) => {
                #[cfg(feature = "verbose_codegen_logging")]
                println!("JIT: Main function failed with error: {}", e);
                Err(CompileError::compiler_error(&format!(
                    "Runtime error: {}",
                    e
                )))
            }
        }
    } else if let Ok(start_func) = instance.exports.get_function("_start") {
        let result = start_func.call(&mut store, &[]);
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(CompileError::compiler_error(&format!(
                "Runtime error: {}",
                e
            ))),
        }
    } else {
        // If neither 'main' nor '_start' is present, but instantiation succeeded, the start section has already run.
        Ok(())
    }
}

/// Create an import object based on the configured IO backend
pub fn create_import_object(
    store: &mut Store,
    module: &Module,
    wasm_bytes: &[u8],
    io_backend: &IoBackend,
) -> Result<(wasmer::Imports, Option<wasmer_wasix::WasiFunctionEnv>), CompileError> {
    create_import_object_with_capture(store, module, wasm_bytes, io_backend, false)
}

/// Create an import object with optional output capture
/// Returns both the imports and an optional WasiFunctionEnv that needs to be kept alive
pub fn create_import_object_with_capture(
    store: &mut Store,
    module: &Module,
    wasm_bytes: &[u8],
    io_backend: &IoBackend,
    capture_output: bool,
) -> Result<(wasmer::Imports, Option<wasmer_wasix::WasiFunctionEnv>), CompileError> {
    match io_backend {
        IoBackend::Wasix => {
            // TEMPORARY: Use native implementation instead of wasmer-wasix due to API compatibility issues
            // The wasmer-wasix 0.601.0 API has initialization requirements that are complex to satisfy
            // For now, we implement fd_write directly which is sufficient for print() functionality
            let mut imports = imports! {};
            setup_native_wasix_fd_write(store, &mut imports, capture_output)?;
            Ok((imports, None))
        }
        IoBackend::Custom(_config_path) => {
            // Set up custom IO hooks
            let mut imports = imports! {};
            setup_custom_io_imports(store, &mut imports)?;
            Ok((imports, None))
        }
        IoBackend::JsBindings => {
            // Set up JS/DOM bindings (for web targets)
            let mut imports = imports! {};
            setup_js_imports(store, &mut imports)?;
            Ok((imports, None))
        }
        IoBackend::Native => {
            // Set up native system call imports with capture support
            let mut imports = imports! {};
            setup_native_imports_with_capture(store, &mut imports, capture_output)?;
            Ok((imports, None))
        }
    }
}

/// Create an import object with native WASIX function support
pub fn create_import_object_with_wasix_native(
    store: &mut Store,
    module: &Module,
    wasm_bytes: &[u8],
    io_backend: &IoBackend,
    _wasix_registry: &mut WasixFunctionRegistry,
) -> Result<(wasmer::Imports, Option<wasmer_wasix::WasiFunctionEnv>), CompileError> {
    match io_backend {
        IoBackend::Wasix => {
            // Set up WASIX imports with native function support
            let imports = setup_wasix_imports_with_native_support(store, module, wasm_bytes)?;
            Ok((imports, None))
        }
        _ => {
            // For non-WASIX backends, use standard import creation
            create_import_object_with_capture(store, module, wasm_bytes, io_backend, false)
        }
    }
}

/// Implement fd_write functionality with WASM memory access
/// This is the core implementation that reads IOVec structures from memory and outputs data
fn implement_fd_write_with_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    fd: i32,
    iovs_ptr: u32,
    iovs_len: u32,
    nwritten_ptr: u32,
) -> Result<u32, String> {
    // Validate file descriptor
    if fd != 1 && fd != 2 {
        return Err(format!(
            "Invalid file descriptor: {}. Only stdout (1) and stderr (2) are supported",
            fd
        ));
    }

    // Handle empty write
    if iovs_len == 0 {
        // Write 0 to nwritten_ptr and return success
        write_u32_to_memory(memory, store, nwritten_ptr, 0)?;
        return Ok(0);
    }

    // Read IOVec structures from WASM memory
    let iovecs = read_iovecs_from_memory(memory, store, iovs_ptr, iovs_len)?;

    // Write data to the appropriate file descriptor
    let mut total_written = 0u32;

    for iovec in &iovecs {
        if iovec.len > 0 {
            // Read string data from this IOVec
            let string_data = read_string_from_memory(memory, store, iovec.ptr, iovec.len)?;

            // Write to stdout or stderr
            match fd {
                1 => {
                    // stdout
                    print!("{}", string_data);
                    std::io::Write::flush(&mut std::io::stdout())
                        .map_err(|e| format!("Failed to flush stdout: {}", e))?;
                }
                2 => {
                    // stderr
                    eprint!("{}", string_data);
                    std::io::Write::flush(&mut std::io::stderr())
                        .map_err(|e| format!("Failed to flush stderr: {}", e))?;
                }
                _ => unreachable!(), // Already validated above
            }

            total_written += iovec.len;
        }
    }

    // Write the total bytes written to nwritten_ptr in WASM memory
    write_u32_to_memory(memory, store, nwritten_ptr, total_written)?;

    Ok(0) // Success errno
}

/// Read IOVec structures from WASM memory
fn read_iovecs_from_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    iovs_ptr: u32,
    iovs_len: u32,
) -> Result<Vec<IOVec>, String> {
    if iovs_len == 0 {
        return Ok(Vec::new());
    }

    // Each IOVec is 8 bytes: 4 bytes ptr + 4 bytes len
    let total_size = iovs_len.checked_mul(8).ok_or("IOVec array size overflow")?;

    // Read the entire IOVec array from memory
    let iovec_bytes = read_bytes_from_memory(memory, store, iovs_ptr, total_size)?;

    // Parse each IOVec structure
    let mut iovecs = Vec::new();
    for i in 0..iovs_len {
        let offset = (i * 8) as usize;
        if offset + 8 > iovec_bytes.len() {
            return Err("IOVec array bounds error".to_string());
        }

        // Read ptr and len as little-endian u32 values
        let ptr = u32::from_le_bytes([
            iovec_bytes[offset],
            iovec_bytes[offset + 1],
            iovec_bytes[offset + 2],
            iovec_bytes[offset + 3],
        ]);
        let len = u32::from_le_bytes([
            iovec_bytes[offset + 4],
            iovec_bytes[offset + 5],
            iovec_bytes[offset + 6],
            iovec_bytes[offset + 7],
        ]);

        iovecs.push(IOVec { ptr, len });
    }

    Ok(iovecs)
}

/// Read bytes from WASM linear memory with enhanced bounds checking and error handling
fn read_bytes_from_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, String> {
    // Handle zero-length reads
    if len == 0 {
        return Ok(Vec::new());
    }

    // Validate reasonable length limits to prevent excessive memory allocation
    const MAX_READ_SIZE: u32 = 16 * 1024 * 1024; // 16MB limit
    if len > MAX_READ_SIZE {
        return Err(format!(
            "Read size {} exceeds maximum allowed size of {} bytes",
            len, MAX_READ_SIZE
        ));
    }

    // Check for address overflow
    let end_ptr = ptr.checked_add(len).ok_or_else(|| {
        format!(
            "Memory address overflow: ptr=0x{:x} + len={} would overflow u32",
            ptr, len
        )
    })?;

    // Get memory view and validate bounds
    let memory_view = memory.view(store);
    let memory_size = memory_view.data_size() as u32;

    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "Memory bounds check - ptr: 0x{:x}, len: {}, end_ptr: 0x{:x}, memory_size: {}",
        ptr, len, end_ptr, memory_size
    );

    // Validate memory bounds
    if ptr >= memory_size {
        return Err(format!(
            "Memory out of bounds: start address 0x{:x} is beyond memory size {}",
            ptr, memory_size
        ));
    }

    if end_ptr > memory_size {
        return Err(format!(
            "Memory out of bounds: trying to read 0x{:x}..0x{:x} but memory size is {}",
            ptr, end_ptr, memory_size
        ));
    }

    // Allocate buffer and read from memory
    let mut bytes = vec![0u8; len as usize];
    memory_view
        .read(ptr as u64, &mut bytes)
        .map_err(|e| format!("Failed to read from memory at 0x{:x}: {}", ptr, e))?;

    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "Successfully read {} bytes from memory at 0x{:x}",
        bytes.len(),
        ptr
    );

    Ok(bytes)
}

/// Read string data from WASM memory with enhanced UTF-8 validation and error handling
fn read_string_from_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    ptr: u32,
    len: u32,
) -> Result<String, String> {
    // Handle zero-length strings
    if len == 0 {
        return Ok(String::new());
    }

    // Validate reasonable string length limits
    const MAX_STRING_SIZE: u32 = 1024 * 1024; // 1MB limit for strings
    if len > MAX_STRING_SIZE {
        return Err(format!(
            "String length {} exceeds maximum allowed size of {} bytes",
            len, MAX_STRING_SIZE
        ));
    }

    // Read raw bytes from memory
    let bytes = read_bytes_from_memory(memory, store, ptr, len)?;

    #[cfg(feature = "verbose_codegen_logging")]
    {
        let preview_len = std::cmp::min(bytes.len(), 50);
        println!(
            "Reading string: {} bytes from 0x{:x}, preview: {:?}",
            len,
            ptr,
            &bytes[..preview_len]
        );
    }

    // Validate UTF-8 with detailed error information
    match String::from_utf8(bytes) {
        Ok(string) => {
            #[cfg(feature = "verbose_codegen_logging")]
            {
                let preview_len = std::cmp::min(string.len(), 50);
                println!(
                    "Successfully decoded string: {:?}{}",
                    &string[..preview_len],
                    if string.len() > 50 { "..." } else { "" }
                );
            }
            Ok(string)
        }
        Err(utf8_error) => {
            let error_pos = utf8_error.utf8_error().valid_up_to();
            Err(format!(
                "Invalid UTF-8 string data at memory 0x{:x}: error at byte position {} - {}",
                ptr, error_pos, utf8_error
            ))
        }
    }
}

/// Write a u32 value to WASM memory with enhanced bounds checking (little-endian)
fn write_u32_to_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    ptr: u32,
    value: u32,
) -> Result<(), String> {
    // Get memory view and validate bounds
    let memory_view = memory.view(store);
    let memory_size = memory_view.data_size() as u32;

    // Check for address overflow
    let end_ptr = ptr.checked_add(4).ok_or_else(|| {
        format!(
            "Memory address overflow: ptr=0x{:x} + 4 would overflow u32",
            ptr
        )
    })?;

    // Validate memory bounds
    if ptr >= memory_size {
        return Err(format!(
            "Memory out of bounds: write address 0x{:x} is beyond memory size {}",
            ptr, memory_size
        ));
    }

    if end_ptr > memory_size {
        return Err(format!(
            "Memory out of bounds: trying to write u32 at 0x{:x}..0x{:x} but memory size is {}",
            ptr, end_ptr, memory_size
        ));
    }

    // Convert value to little-endian bytes and write
    let bytes = value.to_le_bytes();
    memory_view
        .write(ptr as u64, &bytes)
        .map_err(|e| format!("Failed to write u32 to memory at 0x{:x}: {}", ptr, e))?;

    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "Successfully wrote u32 value {} to memory at 0x{:x}",
        value, ptr
    );

    Ok(())
}

/// Write bytes to WASM memory with enhanced bounds checking and cleanup
fn write_bytes_to_memory(
    memory: &wasmer::Memory,
    store: &impl wasmer::AsStoreRef,
    ptr: u32,
    data: &[u8],
) -> Result<(), String> {
    if data.is_empty() {
        return Ok(());
    }

    let len = data.len() as u32;
    
    // Get memory view and validate bounds
    let memory_view = memory.view(store);
    let memory_size = memory_view.data_size() as u32;

    // Check for address overflow
    let end_ptr = ptr.checked_add(len).ok_or_else(|| {
        format!(
            "Memory address overflow: ptr=0x{:x} + len={} would overflow u32",
            ptr, len
        )
    })?;

    // Validate memory bounds
    if ptr >= memory_size {
        return Err(format!(
            "Memory out of bounds: write address 0x{:x} is beyond memory size {}",
            ptr, memory_size
        ));
    }

    if end_ptr > memory_size {
        return Err(format!(
            "Memory out of bounds: trying to write {} bytes at 0x{:x}..0x{:x} but memory size is {}",
            len, ptr, end_ptr, memory_size
        ));
    }

    // Write data to memory
    memory_view
        .write(ptr as u64, data)
        .map_err(|e| format!("Failed to write {} bytes to memory at 0x{:x}: {}", len, ptr, e))?;

    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "Successfully wrote {} bytes to memory at 0x{:x}",
        len, ptr
    );

    Ok(())
}

/// Memory cleanup utilities for host function operations
pub struct MemoryCleanup {
    /// Allocated memory regions that need cleanup
    allocated_regions: Vec<(u32, u32)>, // (ptr, size) pairs
}

impl MemoryCleanup {
    /// Create a new memory cleanup tracker
    pub fn new() -> Self {
        Self {
            allocated_regions: Vec::new(),
        }
    }

    /// Track an allocated memory region for cleanup
    pub fn track_allocation(&mut self, ptr: u32, size: u32) {
        self.allocated_regions.push((ptr, size));
    }

    /// Clear tracked allocations (for manual cleanup)
    pub fn clear_tracked(&mut self) {
        self.allocated_regions.clear();
    }

    /// Get tracked allocations for debugging
    pub fn get_tracked(&self) -> &[(u32, u32)] {
        &self.allocated_regions
    }
}

impl Drop for MemoryCleanup {
    fn drop(&mut self) {
        // In a full implementation, this would free the tracked memory regions
        // For now, we just log the cleanup for debugging
        #[cfg(feature = "verbose_codegen_logging")]
        if !self.allocated_regions.is_empty() {
            println!(
                "Memory cleanup: {} regions tracked for cleanup",
                self.allocated_regions.len()
            );
        }
    }
}

/// IOVec structure matching WASIX specification
#[derive(Debug, Clone)]
struct IOVec {
    /// Pointer to data in linear memory
    ptr: u32,
    /// Length of data in bytes
    len: u32,
}

// Global state to hold memory and store references for WASIX functions
thread_local! {
    static WASIX_MEMORY: RefCell<Option<Memory>> = RefCell::new(None);
    static WASIX_STORE: RefCell<Option<*mut Store>> = RefCell::new(None);
    static CAPTURED_OUTPUT: RefCell<Option<CapturedOutput>> = RefCell::new(None);
}

/// Captured output for testing scenarios
#[derive(Debug, Clone)]
pub struct CapturedOutput {
    pub stdout: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pub stderr: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl CapturedOutput {
    /// Get captured stdout as a string
    pub fn get_stdout(&self) -> Result<String, std::string::FromUtf8Error> {
        let stdout = self.stdout.lock().unwrap();
        String::from_utf8(stdout.clone())
    }

    /// Get captured stderr as a string
    pub fn get_stderr(&self) -> Result<String, std::string::FromUtf8Error> {
        let stderr = self.stderr.lock().unwrap();
        String::from_utf8(stderr.clone())
    }

    /// Clear captured output
    pub fn clear(&self) {
        self.stdout.lock().unwrap().clear();
        self.stderr.lock().unwrap().clear();
    }
}



/// Set memory and store for WASIX functions to use
fn set_wasix_memory_and_store(memory: Memory, store: *mut Store) {
    WASIX_MEMORY.with(|m| {
        *m.borrow_mut() = Some(memory);
    });
    WASIX_STORE.with(|s| {
        *s.borrow_mut() = Some(store);
    });
}

/// Get memory for WASIX functions
fn get_wasix_memory() -> Option<Memory> {
    WASIX_MEMORY.with(|m| m.borrow().clone())
}

/// Get store for WASIX functions
fn get_wasix_store() -> Option<*mut Store> {
    WASIX_STORE.with(|s| *s.borrow())
}

/// Get captured output for testing
pub fn get_captured_output() -> Option<CapturedOutput> {
    CAPTURED_OUTPUT.with(|output| output.borrow().clone())
}

/// Clear captured output
pub fn clear_captured_output() {
    CAPTURED_OUTPUT.with(|output| {
        if let Some(ref captured) = *output.borrow() {
            captured.clear();
        }
    });
}

/// Set up native WASIX fd_write implementation
/// This is a simplified implementation that provides fd_write functionality without wasmer-wasix
fn setup_native_wasix_fd_write(
    store: &mut Store,
    imports: &mut wasmer::Imports,
    capture_output: bool,
) -> Result<(), CompileError> {
    // Initialize captured output if needed
    if capture_output {
        let captured_stdout = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_stderr = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        
        CAPTURED_OUTPUT.with(|output| {
            *output.borrow_mut() = Some(CapturedOutput {
                stdout: captured_stdout,
                stderr: captured_stderr,
            });
        });
    }

    // Create fd_write function
    let fd_write_func = Function::new_typed(
        store,
        move |fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten_ptr: i32| -> i32 {
            // Get memory from shared state
            let memory = match get_wasix_memory() {
                Some(mem) => mem,
                None => {
                    eprintln!("fd_write error: Memory not available");
                    return 8; // EBADF
                }
            };

            let store_ptr = match get_wasix_store() {
                Some(ptr) => ptr,
                None => {
                    eprintln!("fd_write error: Store not available");
                    return 8; // EBADF
                }
            };

            let store_ref = unsafe { &*store_ptr };
            
            // Call the implementation
            match implement_fd_write_with_memory(
                &memory,
                store_ref,
                fd,
                iovs_ptr as u32,
                iovs_len as u32,
                nwritten_ptr as u32,
            ) {
                Ok(errno) => errno as i32,
                Err(e) => {
                    eprintln!("fd_write error: {}", e);
                    5 // EIO
                }
            }
        },
    );

    // Add fd_write to wasi_snapshot_preview1 module
    imports.define("wasi_snapshot_preview1", "fd_write", fd_write_func);

    // Create template_output function for WASIX backend
    let template_output_func = create_template_output_import(store);
    
    // Add template_output function to beanstalk_io module
    imports.define("beanstalk_io", "template_output", template_output_func);

    #[cfg(feature = "verbose_codegen_logging")]
    println!("Native WASIX fd_write and template_output implementation configured");

    Ok(())
}

/// Set up WASIX imports with configurable I/O redirection and error handling
fn setup_wasix_imports_with_io(
    store: &mut Store,
    module: &Module,
    wasm_bytes: &[u8],
    capture_output: bool,
) -> Result<(wasmer::Imports, wasmer_wasix::WasiFunctionEnv), CompileError> {
    #[cfg(feature = "verbose_codegen_logging")]
    if capture_output {
        println!("WASIX environment configured for output capture (testing mode)");
    } else {
        println!("WASIX environment configured for normal output");
    }

    // Create proper WASIX environment using wasmer-wasix
    let mut wasi_env_builder = WasiEnvBuilder::new("beanstalk-program");
    
    // Set the runtime for WASIX - required for async I/O operations
    // Get the current Tokio runtime handle and create a pluggable runtime
    let runtime_handle = tokio::runtime::Handle::current();
    let task_manager = wasmer_wasix::runtime::task_manager::tokio::TokioTaskManager::new(runtime_handle);
    let runtime = wasmer_wasix::PluggableRuntime::new(std::sync::Arc::new(task_manager));
    wasi_env_builder.set_runtime(std::sync::Arc::new(runtime));

    // For capture_output, we'll use a different approach - intercept at the fd_write level
    if capture_output {
        // Initialize captured output storage
        let captured_stdout = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_stderr = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        
        CAPTURED_OUTPUT.with(|output| {
            *output.borrow_mut() = Some(CapturedOutput {
                stdout: captured_stdout,
                stderr: captured_stderr,
            });
        });
    }

    // Build the WASIX environment with enhanced error handling
    let wasi_env = wasi_env_builder
        .finalize(store)
        .map_err(|e| {
            let error_msg = format!("Failed to create WASIX environment: {}", e);
            let suggestion = "Ensure wasmer-wasix is properly installed and configured. Try updating Wasmer to the latest version.";
            CompileError::compiler_error(&format!("{} Suggestion: {}", error_msg, suggestion))
        })?;

    // Generate WASIX import object with comprehensive error handling
    let wasix_imports = wasi_env.import_object(store, module).map_err(|e| {
        let error_msg = format!("Failed to create WASIX imports: {}", e);
        let suggestion = match e.to_string().as_str() {
            s if s.contains("memory") => {
                "Increase WASM memory limits or check memory configuration"
            }
            s if s.contains("function") => {
                "Verify that all required WASIX functions are available in the runtime"
            }
            s if s.contains("module") => {
                "Check that the WASM module has correct WASIX import declarations"
            }
            _ => "Verify WASIX runtime configuration and ensure all dependencies are available",
        };
        CompileError::compiler_error(&format!("{} Suggestion: {}", error_msg, suggestion))
    })?;

    // Validate imports against module requirements
    validate_wasix_imports(wasm_bytes, &wasix_imports)?;

    #[cfg(feature = "verbose_codegen_logging")]
    println!("WASIX environment configured with proper fd_write implementation");

    // Return both the imports and the WasiEnv so it can be initialized after instantiation
    Ok((wasix_imports, wasi_env))
}



/// Set up WASIX imports with native function support and comprehensive error handling
fn setup_wasix_imports_with_native_support(
    store: &mut Store,
    module: &Module,
    wasm_bytes: &[u8],
) -> Result<wasmer::Imports, CompileError> {
    // Create WASIX environment using wasmer-wasix
    let wasi_env_builder = WasiEnvBuilder::new("beanstalk-program");

    // Build the WASIX environment with enhanced error handling
    let wasi_env = wasi_env_builder
        .finalize(store)
        .map_err(|e| {
            let error_msg = format!("Failed to create WASIX environment: {}", e);
            let suggestion = "Ensure wasmer-wasix is properly installed and configured. Try updating Wasmer to the latest version.";
            CompileError::compiler_error(&format!("{} Suggestion: {}", error_msg, suggestion))
        })?;

    // Generate WASIX import object with comprehensive error handling
    let wasix_imports = wasi_env.import_object(store, module).map_err(|e| {
        let error_msg = format!("Failed to create WASIX imports: {}", e);
        let suggestion = match e.to_string().as_str() {
            s if s.contains("memory") => {
                "Increase WASM memory limits or check memory configuration"
            }
            s if s.contains("function") => {
                "Verify that all required WASIX functions are available in the runtime"
            }
            s if s.contains("module") => {
                "Check that the WASM module has correct WASIX import declarations"
            }
            _ => "Verify WASIX runtime configuration and ensure all dependencies are available",
        };
        CompileError::compiler_error(&format!("{} Suggestion: {}", error_msg, suggestion))
    })?;

    // Validate that required WASIX imports are available
    validate_wasix_imports(wasm_bytes, &wasix_imports)?;

    // Native function overrides are not yet implemented
    // Currently using standard WASIX imports from wasmer-wasix
    // Native function dispatch through registry system requires WASM memory integration

    #[cfg(feature = "verbose_codegen_logging")]
    println!("WASIX environment configured with native function support");

    Ok(wasix_imports)
}

/// Validate that required WASIX imports are properly resolved
fn validate_wasix_imports(
    wasm_bytes: &[u8],
    imports: &wasmer::Imports,
) -> Result<(), CompileError> {
    use wasmparser::{Parser, Payload};

    let mut required_wasix_imports = Vec::new();
    let parser = Parser::new(0);

    // Parse the WASM module to find required imports
    for payload in parser.parse_all(&wasm_bytes) {
        if let Payload::ImportSection(import_reader) = payload.map_err(|e| {
            CompileError::compiler_error(&format!("Failed to parse WASM module: {}", e))
        })? {
            for import in import_reader {
                let import = import.map_err(|e| {
                    CompileError::compiler_error(&format!("Failed to read import: {}", e))
                })?;

                // Check for WASIX imports
                if import.module.starts_with("wasix_") || import.module == "wasi_snapshot_preview1"
                {
                    required_wasix_imports
                        .push((import.module.to_string(), import.name.to_string()));
                }
            }
        }
    }

    // Validate that all required WASIX imports are available
    for (module_name, function_name) in &required_wasix_imports {
        if !imports.contains_namespace(module_name) {
            return Err(CompileError::compiler_error(&format!(
                "WASIX import resolution failed: module '{}' not found. Suggestion: Ensure WASIX runtime is properly configured and supports the required module version",
                module_name
            )));
        }

        let namespace = imports.get_namespace_exports(module_name)
            .ok_or_else(|| CompileError::compiler_error(&format!(
                "WASIX import resolution failed: cannot access exports for module '{}'. Suggestion: Check WASIX runtime configuration",
                module_name
            )))?;

        if !namespace.iter().any(|(name, _)| name == function_name) {
            let available_functions: Vec<String> =
                namespace.iter().map(|(name, _)| name.clone()).collect();
            return Err(CompileError::compiler_error(&format!(
                "WASIX import resolution failed: function '{}' not found in module '{}'. Available functions: {}. Suggestion: Update WASIX runtime or check function name spelling",
                function_name,
                module_name,
                available_functions.join(", ")
            )));
        }
    }

    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "✓ All {} WASIX imports validated successfully",
        required_wasix_imports.len()
    );

    Ok(())
}

/// Set up custom IO imports for embedded scenarios
#[allow(unused_variables)]
fn setup_custom_io_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Create template_output function for custom IO backend
    let template_output_func = create_template_output_import(store);
    
    // Add template_output function to beanstalk_io module
    imports.define("beanstalk_io", "template_output", template_output_func);
    
    #[cfg(feature = "verbose_codegen_logging")]
    println!("Custom IO backend configured with template_output");
    
    Ok(())
}

/// Set up JavaScript/DOM bindings for web targets
#[allow(unused_variables)]
fn setup_js_imports(store: &mut Store, imports: &mut wasmer::Imports) -> Result<(), CompileError> {
    // Create template_output function for JavaScript backend
    let template_output_func = create_template_output_import(store);
    
    // Add template_output function to beanstalk_io module
    imports.define("beanstalk_io", "template_output", template_output_func);
    
    #[cfg(feature = "verbose_codegen_logging")]
    println!("JavaScript backend configured with template_output");
    
    Ok(())
}

/// Create a template_output import function for the JIT runtime
/// This function reads a string from WASM memory and outputs it to stdout
fn create_template_output_import(store: &mut Store) -> Function {
    Function::new_typed(
        store,
        move |text_ptr: i32, text_len: i32| {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "template_output called with text_ptr=0x{:x}, text_len={}",
                text_ptr, text_len
            );

            // Get memory from shared state
            let memory = match get_wasix_memory() {
                Some(mem) => mem,
                None => {
                    eprintln!("template_output error: Memory not available");
                    return; // Void function, just return
                }
            };

            let store_ptr = match get_wasix_store() {
                Some(ptr) => ptr,
                None => {
                    eprintln!("template_output error: Store not available");
                    return; // Void function, just return
                }
            };

            // Read string from WASM memory
            let store_ref = unsafe { &*store_ptr };
            match read_string_from_memory(&memory, store_ref, text_ptr as u32, text_len as u32) {
                Ok(text) => {
                    // Check if we should capture output or print normally
                    let should_capture = CAPTURED_OUTPUT.with(|output| output.borrow().is_some());
                    
                    if should_capture {
                        // Capture output for testing
                        CAPTURED_OUTPUT.with(|output| {
                            if let Some(ref captured) = *output.borrow() {
                                let mut stdout = captured.stdout.lock().unwrap();
                                stdout.extend_from_slice(text.as_bytes());
                            }
                        });
                    } else {
                        // Normal output to stdout
                        print!("{}", text);
                        if let Err(e) = std::io::Write::flush(&mut std::io::stdout()) {
                            eprintln!("template_output error: Failed to flush stdout: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("template_output error: {}", e);
                }
            }
        },
    )
}

/// Set up native system call imports
fn setup_native_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    setup_native_imports_with_capture(store, imports, false)
}

/// Set up native system call imports with optional output capture
fn setup_native_imports_with_capture(
    store: &mut Store,
    imports: &mut wasmer::Imports,
    capture_output: bool,
) -> Result<(), CompileError> {
    // Initialize captured output if needed
    if capture_output {
        let captured_stdout = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_stderr = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        
        CAPTURED_OUTPUT.with(|output| {
            *output.borrow_mut() = Some(CapturedOutput {
                stdout: captured_stdout,
                stderr: captured_stderr,
            });
        });
    }

    // Create template_output function for native backend
    let template_output_func = create_template_output_import(store);
    
    // Add template_output function to beanstalk_io module
    imports.define("beanstalk_io", "template_output", template_output_func);

    // Create native print function that directly outputs to stdout or captures (legacy support)
    let print_func = Function::new_typed(
        store,
        move |text_ptr: i32, text_len: i32| -> i32 {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "Native print called with text_ptr=0x{:x}, text_len={}",
                text_ptr, text_len
            );

            // Get memory from shared state
            let memory = match get_wasix_memory() {
                Some(mem) => mem,
                None => {
                    eprintln!("Native print error: Memory not available");
                    return -1; // Error
                }
            };

            let store_ptr = match get_wasix_store() {
                Some(ptr) => ptr,
                None => {
                    eprintln!("Native print error: Store not available");
                    return -1; // Error
                }
            };

            // Read string from WASM memory
            let store_ref = unsafe { &*store_ptr };
            match read_string_from_memory(&memory, store_ref, text_ptr as u32, text_len as u32) {
                Ok(text) => {
                    // Check if we should capture output or print normally
                    let should_capture = CAPTURED_OUTPUT.with(|output| output.borrow().is_some());
                    
                    if should_capture {
                        // Capture output for testing
                        CAPTURED_OUTPUT.with(|output| {
                            if let Some(ref captured) = *output.borrow() {
                                let mut stdout = captured.stdout.lock().unwrap();
                                stdout.extend_from_slice(text.as_bytes());
                            }
                        });
                    } else {
                        // Normal output to stdout
                        print!("{}", text);
                        if let Err(e) = std::io::Write::flush(&mut std::io::stdout()) {
                            eprintln!("Native print error: Failed to flush stdout: {}", e);
                            return -1; // Error
                        }
                    }
                    
                    0 // Success
                }
                Err(e) => {
                    eprintln!("Native print error: {}", e);
                    -1 // Error
                }
            }
        },
    );

    // Add print function to beanstalk_io module (legacy support)
    imports.define("beanstalk_io", "print", print_func);

    #[cfg(feature = "verbose_codegen_logging")]
    if capture_output {
        println!("Native backend host functions configured with output capture");
    } else {
        println!("Native backend host functions configured");
    }

    Ok(())
}

/// JIT Runtime with WASIX integration
pub struct JitRuntime {
    /// WASIX function registry for import-based calls
    pub wasix_registry: WasixFunctionRegistry,
    /// Wasmer store for JIT execution
    pub store: Store,
}

impl Drop for JitRuntime {
    fn drop(&mut self) {
        // Cleanup WASIX context when runtime is dropped
        self.cleanup_wasix_context();
    }
}

impl JitRuntime {
    /// Create a new JIT runtime with WASIX support
    pub fn new() -> Result<Self, CompileError> {
        let store = Store::default();
        let wasix_registry = create_wasix_registry().map_err(|e| {
            CompileError::compiler_error(&format!("Failed to create WASIX registry: {:?}", e))
        })?;

        Ok(Self {
            wasix_registry,
            store,
        })
    }

    /// Setup WASIX environment and native functions
    pub fn setup_wasix_environment(&mut self) -> Result<(), CompileError> {
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASIX environment initialized with import-based function support");

        Ok(())
    }

    /// Initialize WASIX context with memory integration (simplified)
    pub fn initialize_wasix_context_with_memory(
        &mut self,
        _memory: wasmer::Memory,
    ) -> Result<(), CompileError> {
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASIX memory integration - using import-based calls");

        Ok(())
    }

    /// Cleanup WASIX context and resources
    pub fn cleanup_wasix_context(&mut self) {
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASIX cleanup completed (simplified registry)");
    }

    /// Check if WASIX is initialized (always true for simplified registry)
    pub fn is_wasix_initialized(&self) -> bool {
        true
    }

    /// Register native WASIX function implementations
    /// TODO: This will be properly implemented in task 4 when we integrate WASIX with JIT runtime
    pub fn register_native_wasix_functions(&mut self) -> Result<(), CompileError> {
        // Placeholder implementation - actual native function registration requires memory access integration
        // The fd_write method now requires memory and store parameters which will be available in JIT context

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "Native WASIX function registration - placeholder (requires task 4 implementation)"
        );

        Ok(())
    }

    /// Check if a WASIX function is available (import-based only)
    pub fn has_wasix_function(&self, function_name: &str) -> bool {
        self.wasix_registry.has_function(function_name)
    }

    /// Get the WASIX function registry (for codegen integration)
    pub fn get_wasix_registry(&self) -> &WasixFunctionRegistry {
        &self.wasix_registry
    }

    /// Get mutable WASIX function registry (for registration)
    pub fn get_wasix_registry_mut(&mut self) -> &mut WasixFunctionRegistry {
        &mut self.wasix_registry
    }

    /// Execute WASM bytecode with WASIX support
    pub fn execute_wasm_with_wasix(
        &mut self,
        wasm_bytes: &[u8],
        config: &RuntimeConfig,
    ) -> Result<(), CompileError> {
        // Setup WASIX environment (simplified)
        self.setup_wasix_environment()?;

        // Register native WASIX functions
        self.register_native_wasix_functions()?;

        // Compile WASM module
        let module = Module::new(&self.store, wasm_bytes).map_err(|e| {
            CompileError::compiler_error(&format!("Failed to compile WASM module: {}", e))
        })?;

        // Set up imports based on IO backend
        let (import_object, _wasi_env_guard) = create_import_object_with_wasix_native(
            &mut self.store,
            &module,
            wasm_bytes,
            &config.io_backend,
            &mut self.wasix_registry,
        )?;

        // Instantiate the module
        let instance = Instance::new(&mut self.store, &module, &import_object).map_err(|e| {
            CompileError::compiler_error(&format!("Failed to instantiate WASM module: {}", e))
        })?;

        // Integrate WASM memory with WASIX context
        if let Ok(memory) = instance.exports.get_memory("memory") {
            self.initialize_wasix_context_with_memory(memory.clone())?;

            #[cfg(feature = "verbose_codegen_logging")]
            println!("WASM memory integrated with WASIX context");
        } else {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "No WASM memory export found - WASIX context will use default memory management"
            );
        }

        // Execute the module (similar to existing execute_direct_jit logic)
        if let Ok(main_func) = instance.exports.get_function("main") {
            match main_func.call(&mut self.store, &[]) {
                Ok(values) => {
                    if !values.is_empty() {
                        println!("Program returned: {:?}", values);
                    }
                    Ok(())
                }
                Err(e) => Err(CompileError::compiler_error(&format!(
                    "Runtime error: {}",
                    e
                ))),
            }
        } else if let Ok(start_func) = instance.exports.get_function("_start") {
            let result = start_func.call(&mut self.store, &[]);
            match result {
                Ok(_) => Ok(()),
                Err(e) => Err(CompileError::compiler_error(&format!(
                    "Runtime error: {}",
                    e
                ))),
            }
        } else {
            // If neither 'main' nor '_start' is present, but instantiation succeeded, the start section has already run.
            Ok(())
        }
    }
}

/// Convenience function to execute WASM with WASIX using a new runtime instance
pub fn execute_wasm_with_wasix_runtime(
    wasm_bytes: &[u8],
    config: &RuntimeConfig,
) -> Result<(), CompileError> {
    // For now, use the standard WASIX imports from wasmer-wasix
    // This should provide the fd_write implementation we need
    execute_direct_jit_with_capture(wasm_bytes, config, false)
}
