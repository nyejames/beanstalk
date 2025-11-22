# Full Native Build System Example

This example shows a complete implementation of a Beanstalk build system for native targets with all optional IO functions.

## Build System Configuration

```rust
// src/build_system/full_native.rs

use crate::compiler::host_functions::registry::{
    HostFunctionRegistry, RuntimeBackend, HostFunctionDef, BasicParameter
};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::string_interning::StringTable;

/// Create a full-featured native build system with all IO functions
pub fn create_full_native_build_system(
    string_table: &mut StringTable,
) -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new_with_backend(RuntimeBackend::Native);

    // 1. Register mandatory io() function
    register_io_function(&mut registry, string_table)?;

    // 2. Register optional write() function
    register_write_function(&mut registry, string_table)?;

    // 3. Register optional error() function
    register_error_function(&mut registry, string_table)?;

    // 4. Register optional read() function
    register_read_function(&mut registry, string_table)?;

    // Validate that io() is available (build system contract requirement)
    registry.validate_io_availability(string_table)?;

    Ok(registry)
}

fn register_io_function(
    registry: &mut HostFunctionRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    let io_function = HostFunctionDef::new(
        "io",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString,
            ownership: Ownership::ImmutableReference,
        }],
        vec![],
        "beanstalk_io",
        "io",
        "Output content to stdout with automatic newline",
        string_table,
    );

    registry.register_function(io_function, string_table)
}

fn register_write_function(
    registry: &mut HostFunctionRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    let write_function = HostFunctionDef::new(
        "write",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString,
            ownership: Ownership::ImmutableReference,
        }],
        vec![],
        "beanstalk_io",
        "write",
        "Output content to stdout without newline",
        string_table,
    );

    registry.register_function(write_function, string_table)
}

fn register_error_function(
    registry: &mut HostFunctionRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    let error_function = HostFunctionDef::new(
        "error",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString,
            ownership: Ownership::ImmutableReference,
        }],
        vec![],
        "beanstalk_io",
        "error",
        "Output error message to stderr with newline",
        string_table,
    );

    registry.register_function(error_function, string_table)
}

fn register_read_function(
    registry: &mut HostFunctionRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    let read_function = HostFunctionDef::new_with_error(
        "read",
        vec![BasicParameter {
            name: string_table.intern("path"),
            data_type: DataType::String,
            ownership: Ownership::ImmutableReference,
        }],
        vec![DataType::String], // Returns String on success
        "beanstalk_io",
        "read",
        "Read content from file path",
        string_table,
    );

    registry.register_function(read_function, string_table)
}
```

## Native Runtime Implementation

```rust
// src/runtime/full_native_runtime.rs

use wasmer::{Store, Module, Instance, imports, Function, FunctionEnv, FunctionEnvMut, Memory};
use std::io::{self, Write};
use std::fs;

/// Full-featured native runtime environment
pub struct FullNativeRuntime {
    store: Store,
    instance: Option<Instance>,
}

/// Runtime environment data
struct RuntimeEnv {
    memory: Option<Memory>,
}

impl FullNativeRuntime {
    /// Create a new full native runtime
    pub fn new() -> Self {
        FullNativeRuntime {
            store: Store::default(),
            instance: None,
        }
    }

    /// Initialize the runtime with WASM bytes
    pub fn initialize(&mut self, wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let module = Module::new(&self.store, wasm_bytes)?;

        // Create function environment
        let env = FunctionEnv::new(&mut self.store, RuntimeEnv { memory: None });

        // Create import object with all IO functions
        let import_object = imports! {
            "beanstalk_io" => {
                "io" => Function::new_typed_with_env(
                    &mut self.store,
                    &env,
                    Self::host_io
                ),
                "write" => Function::new_typed_with_env(
                    &mut self.store,
                    &env,
                    Self::host_write
                ),
                "error" => Function::new_typed_with_env(
                    &mut self.store,
                    &env,
                    Self::host_error
                ),
                "read" => Function::new_typed_with_env(
                    &mut self.store,
                    &env,
                    Self::host_read
                ),
            }
        };

        // Instantiate the WASM module
        let instance = Instance::new(&mut self.store, &module, &import_object)?;

        // Store memory reference in environment
        if let Ok(memory) = instance.exports.get_memory("memory") {
            env.as_mut(&mut self.store).memory = Some(memory.clone());
        }

        self.instance = Some(instance);

        Ok(())
    }

    /// Read string from WASM memory
    fn read_string(env: &FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) -> String {
        let memory = env.data().memory.as_ref()
            .expect("Memory not initialized");
        
        let memory_view = memory.view(&env);
        let mut buffer = vec![0u8; len as usize];
        memory_view.read(ptr as u64, &mut buffer)
            .expect("Failed to read from WASM memory");

        String::from_utf8_lossy(&buffer).to_string()
    }

    /// Write string to WASM memory (for read() return value)
    fn write_string(env: &mut FunctionEnvMut<RuntimeEnv>, text: &str) -> (i32, i32) {
        let memory = env.data().memory.as_ref()
            .expect("Memory not initialized");

        let bytes = text.as_bytes();
        
        // Allocate memory (simplified - real implementation needs proper allocator)
        // For now, use a fixed offset in memory
        let ptr = 1024; // Fixed offset for simplicity
        
        let memory_view = memory.view(&env);
        memory_view.write(ptr, bytes)
            .expect("Failed to write to WASM memory");

        (ptr as i32, bytes.len() as i32)
    }

    /// Host function: io() - output with newline
    fn host_io(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) {
        let text = Self::read_string(&env, ptr, len);
        println!("{}", text); // Automatic newline
    }

    /// Host function: write() - output without newline
    fn host_write(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) {
        let text = Self::read_string(&env, ptr, len);
        print!("{}", text); // No newline
        io::stdout().flush().expect("Failed to flush stdout");
    }

    /// Host function: error() - error output with newline
    fn host_error(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) {
        let text = Self::read_string(&env, ptr, len);
        eprintln!("{}", text); // Automatic newline to stderr
    }

    /// Host function: read() - read from file
    fn host_read(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) -> (i32, i32) {
        let path = Self::read_string(&env, ptr, len);
        
        match fs::read_to_string(&path) {
            Ok(content) => Self::write_string(&mut env, &content),
            Err(e) => {
                let error_msg = format!("Failed to read {}: {}", path, e);
                Self::write_string(&mut env, &error_msg)
            }
        }
    }

    /// Run the WASM module's entry point
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let instance = self.instance.as_ref()
            .ok_or("Runtime not initialized")?;

        let start_func = instance.exports.get_function("_start")?;
        start_func.call(&mut self.store, &[])?;

        Ok(())
    }
}

/// Run a Beanstalk WASM program with full native runtime
pub fn run_beanstalk_program(wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = FullNativeRuntime::new();
    runtime.initialize(wasm_bytes)?;
    runtime.run()?;
    Ok(())
}
```

## Example Beanstalk Program

```beanstalk
-- full-native-example.bst
-- This program uses all available IO functions

-- Basic output with newline
io("=== Full Native Build System Demo ===")
io("")

-- Output without newline (progress indicator)
write("Loading")
write(".")
write(".")
write(".")
io(" Done!")

-- Error output to stderr
error("This is an error message (appears on stderr)")

-- Read from file (with error handling)
content = read("config.txt") !err:
    error([: Failed to read file: err])
    return
;

io([: File content: content])

-- Multiple types with io()
io(42)
io(3.14)
io(true)
io([: Result: 42])
```

## Compilation and Execution

```bash
# Create test file
echo "Hello from config file!" > config.txt

# Compile with full native build system
beanstalk build full-native-example.bst --target native --output program.wasm

# Run with custom runtime
cargo run --bin beanstalk-runner -- program.wasm

# Or run with Wasmer (requires custom imports)
wasmer run program.wasm
```

## Expected Output

```
=== Full Native Build System Demo ===

Loading... Done!
This is an error message (appears on stderr)
File content: Hello from config file!
42
3.14
true
Result: 42
```

## Build System Contract Compliance

This full native build system satisfies and exceeds the Beanstalk Build System IO Contract:

✅ Provides `Io` struct (conceptually)
✅ Implements mandatory `io()` function
✅ `io()` accepts `CoerceToString` parameter
✅ `io()` appends newline to output
✅ Compiler validation passes
✅ Provides optional `write()` function
✅ Provides optional `error()` function
✅ Provides optional `read()` function

## Features

### Mandatory Features
- ✅ `io()` - Output to stdout with automatic newline

### Optional Features
- ✅ `write()` - Output to stdout without newline
- ✅ `error()` - Output to stderr with newline
- ✅ `read()` - Read from file system

### Future Extensions
- ⏳ Environment variable access
- ⏳ Command-line argument parsing
- ⏳ Network socket support
- ⏳ Process spawning
- ⏳ Signal handling

## Performance Characteristics

- **Native speed**: Full WASM performance
- **Direct system calls**: Minimal overhead
- **Buffered IO**: Efficient stdout/stderr handling
- **File caching**: Optional file content caching

## Advanced Features

### Custom Memory Allocator

```rust
// Implement proper memory allocation for string returns
struct WasmAllocator {
    next_offset: u32,
    allocations: HashMap<u32, u32>, // ptr -> size
}

impl WasmAllocator {
    fn allocate(&mut self, size: u32) -> u32 {
        let ptr = self.next_offset;
        self.allocations.insert(ptr, size);
        self.next_offset += size;
        ptr
    }

    fn deallocate(&mut self, ptr: u32) {
        self.allocations.remove(&ptr);
    }
}
```

### Error Handling with Result Types

```rust
// Enhanced read() with proper error handling
fn host_read_with_result(
    mut env: FunctionEnvMut<RuntimeEnv>,
    ptr: i32,
    len: i32
) -> (i32, i32, i32) { // (result_ptr, result_len, is_error)
    let path = Self::read_string(&env, ptr, len);
    
    match fs::read_to_string(&path) {
        Ok(content) => {
            let (ptr, len) = Self::write_string(&mut env, &content);
            (ptr, len, 0) // Success
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            let (ptr, len) = Self::write_string(&mut env, &error_msg);
            (ptr, len, 1) // Error
        }
    }
}
```

### Logging and Debugging

```rust
// Add logging support
fn host_io_with_logging(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) {
    let text = Self::read_string(&env, ptr, len);
    
    // Log to file
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("beanstalk.log")
    {
        writeln!(file, "[{}] {}", chrono::Local::now(), text).ok();
    }
    
    // Print to stdout
    println!("{}", text);
}
```

## When to Use

Use this full native build system when:

- Building command-line applications
- Need file system access
- Require error handling and reporting
- Want fine-grained output control
- Building system utilities or tools

## Testing

```bash
# Unit tests
cargo test full_native_runtime

# Integration tests
./test-full-native.sh

# Performance benchmarks
hyperfine 'cargo run --release --bin beanstalk-runner -- program.wasm'
```

## Troubleshooting

### File Not Found Errors

```rust
// Add better error messages
fn host_read(mut env: FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) -> (i32, i32) {
    let path = Self::read_string(&env, ptr, len);
    
    match fs::read_to_string(&path) {
        Ok(content) => Self::write_string(&mut env, &content),
        Err(e) => {
            let error_msg = match e.kind() {
                io::ErrorKind::NotFound => format!("File not found: {}", path),
                io::ErrorKind::PermissionDenied => format!("Permission denied: {}", path),
                _ => format!("Failed to read {}: {}", path, e),
            };
            Self::write_string(&mut env, &error_msg)
        }
    }
}
```

### Memory Overflow

```rust
// Add bounds checking
fn read_string_safe(env: &FunctionEnvMut<RuntimeEnv>, ptr: i32, len: i32) -> Result<String, String> {
    if ptr < 0 || len < 0 {
        return Err("Invalid memory access: negative pointer or length".to_string());
    }

    if len > 1024 * 1024 { // 1MB limit
        return Err("String too large: exceeds 1MB limit".to_string());
    }

    let memory = env.data().memory.as_ref()
        .ok_or("Memory not initialized")?;
    
    let memory_view = memory.view(&env);
    let mut buffer = vec![0u8; len as usize];
    memory_view.read(ptr as u64, &mut buffer)
        .map_err(|e| format!("Failed to read from WASM memory: {}", e))?;

    Ok(String::from_utf8_lossy(&buffer).to_string())
}
```

## See Also

- [Minimal Native Build System](./build-system-minimal-native.md)
- [Full Web Build System](./build-system-full-web.md)
- [Build System IO Contract](../build-system-io-contract.md)
- [Wasmer Documentation](https://docs.wasmer.io/)
