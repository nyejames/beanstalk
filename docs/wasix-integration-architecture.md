# WASIX Integration Architecture

This document explains the Beanstalk compiler's WASIX (WebAssembly System Interface eXtended) integration architecture, including the native function system, memory management patterns, and WASM codegen changes.

## Overview

Beanstalk's WASIX integration provides enhanced I/O capabilities, networking, and threading support through Wasmer's WASIX implementation. The architecture supports both native function implementations for optimal performance and standard WASIX imports for compatibility.

## Architecture Components

### 1. WASIX Function Registry

**Location**: `src/compiler/host_functions/wasix_registry.rs`

The WASIX function registry manages function definitions and native implementations:

```rust
pub struct WasixFunctionRegistry {
    functions: HashMap<String, WasixFunctionDef>,
    native_functions: HashMap<String, WasixNativeFunction>,
}

#[derive(Debug, Clone)]
pub struct WasixFunctionDef {
    /// WASIX module name (e.g., "wasix_32v1")
    pub module: String,
    /// Function name (e.g., "fd_write")
    pub name: String,
    /// Parameter types
    pub parameters: Vec<ValType>,
    /// Return types
    pub returns: Vec<ValType>,
    /// Native implementation if available
    pub native_impl: Option<WasixNativeFunction>,
    /// WASM function index after import
    pub func_index: Option<u32>,
}
```

**Key Features**:
- Dual support for native implementations and WASIX imports
- Function signature validation against WASIX specification
- Runtime function index management for WASM modules
- Extensible registry for adding new WASIX functions

### 2. WASIX Context and Environment

**Location**: `src/runtime/wasix_context.rs`

The WASIX context manages runtime environment and state:

```rust
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
```

**Responsibilities**:
- WASIX environment initialization and management
- Memory access for WASIX operations
- File descriptor management (stdout, stderr, files)
- Process state tracking for threading and networking

### 3. Memory Management System

**Location**: `src/runtime/wasix_memory.rs`

WASIX-optimized memory management with WASM linear memory integration:

```rust
pub struct WasixMemoryManager {
    /// Current allocation pointer
    current_ptr: u32,
    /// Memory alignment requirements
    alignment: u32,
    /// Allocated regions for cleanup
    allocated_regions: Vec<MemoryRegion>,
    /// WASIX memory layout information
    layout_info: WasixMemoryLayout,
}
```

**Memory Layout**:
- **Reserved Area** (0x0000 - 0x10000): WASIX system use
- **Heap Area** (0x10000+): Dynamic allocations with 8-byte alignment
- **Stack Area**: Function call stack management
- **String Data**: UTF-8 string storage with proper alignment

### 4. JIT Runtime Integration

**Location**: `src/runtime/jit.rs`

Integration with Wasmer JIT runtime for native function execution:

```rust
impl JitRuntime {
    pub fn setup_wasix_environment(&mut self, store: &mut Store) -> Result<(), CompileError>;
    pub fn register_native_wasix_functions(&mut self) -> Result<(), CompileError>;
    pub fn call_native_wasix_function(&mut self, function_name: &str, args: &[Value]) -> Result<Vec<Value>, CompileError>;
}
```

**Features**:
- Native WASIX function registration and execution
- WASIX environment setup with Wasmer integration
- Fallback to standard WASIX imports when native unavailable
- Error handling and diagnostics for WASIX operations

## WASM Code Generation Changes

### 1. WASIX Import Generation

**Location**: `src/compiler/codegen/wasix_codegen.rs`

WASM modules now generate proper WASIX imports:

```rust
impl WasmModule {
    pub fn add_wasix_imports(&mut self) -> Result<(), CompileError> {
        // Add WASIX fd_write import
        let fd_write_type = self.module.type_section().function(
            [ValType::I32, ValType::I32, ValType::I32, ValType::I32], // fd, iovs, iovs_len, nwritten
            [ValType::I32] // errno result
        );
        
        self.module.import_section().import(
            "wasix_32v1", // WASIX module name
            "fd_write", 
            EntityType::Function(fd_write_type)
        );
        
        Ok(())
    }
}
```

### 2. Host Call Lowering

WASIX host calls are lowered to appropriate WASM instructions:

```rust
pub fn lower_wasix_host_call(
    &mut self,
    function_name: &str,
    args: &[Operand],
    destination: &Option<Place>,
    function: &mut Function,
    local_map: &LocalMap,
) -> Result<(), CompileError> {
    match function_name {
        "print" => self.lower_wasix_print(args, function, local_map),
        _ => return_compiler_error!("Unsupported WASIX function: {}", function_name),
    }
}
```

### 3. Memory Layout Management

WASIX operations require specific memory layouts:

```rust
pub fn lower_wasix_print(
    &mut self,
    args: &[Operand],
    function: &mut Function,
    local_map: &LocalMap,
) -> Result<(), CompileError> {
    // 1. Allocate string in linear memory with WASIX alignment
    let (string_ptr, string_len) = self.allocate_wasix_string(args[0], function, local_map)?;
    
    // 2. Create IOVec structure in memory
    let iovec_ptr = self.create_wasix_iovec(string_ptr, string_len, function)?;
    
    // 3. Allocate space for nwritten result
    let nwritten_ptr = self.allocate_wasix_result_memory(function)?;
    
    // 4. Call WASIX fd_write
    function.instruction(&Instruction::I32Const(1)); // stdout fd
    function.instruction(&Instruction::LocalGet(iovec_ptr));
    function.instruction(&Instruction::I32Const(1)); // iovec count
    function.instruction(&Instruction::LocalGet(nwritten_ptr));
    function.instruction(&Instruction::Call(self.wasix_fd_write_func_index));
    
    Ok(())
}
```

## Memory Management Patterns

### 1. WASIX Memory Alignment

WASIX requires specific memory alignment for optimal performance:

- **8-byte alignment** for IOVec structures
- **4-byte alignment** for integer parameters
- **1-byte alignment** for string data
- **Page alignment** (4KB) for large allocations

### 2. IOVec Management

IOVec (I/O Vector) structures are fundamental to WASIX I/O operations:

```rust
#[repr(C)]
pub struct IOVec {
    /// Pointer to data in linear memory
    pub ptr: u32,
    /// Length of data in bytes
    pub len: u32,
}
```

**Usage Pattern**:
1. Allocate string data in linear memory
2. Create IOVec structure pointing to string data
3. Pass IOVec array to WASIX function
4. Handle result and cleanup memory

### 3. Memory Region Tracking

All WASIX allocations are tracked for proper cleanup:

```rust
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start address in linear memory
    pub ptr: u32,
    /// Size in bytes
    pub size: u32,
    /// Allocation purpose for debugging
    pub purpose: String,
}
```

### 4. Error Handling Patterns

WASIX operations use comprehensive error handling:

```rust
#[derive(Debug, Clone)]
pub enum WasixError {
    /// Invalid file descriptor
    InvalidFileDescriptor(u32),
    /// Memory out of bounds access
    MemoryOutOfBounds,
    /// Invalid argument count for function
    InvalidArgumentCount,
    /// WASIX environment setup error
    EnvironmentError(String),
    /// Native function not found
    NativeFunctionNotFound(String),
}
```

## Native Function Implementation

### 1. Native fd_write Implementation

The core WASIX function for output operations:

```rust
pub fn native_fd_write(context: &mut WasixContext, args: &[Value]) -> Result<Vec<Value>, WasixError> {
    let fd = args[0].unwrap_i32() as u32;
    let iovs_ptr = args[1].unwrap_i32() as u32;
    let iovs_len = args[2].unwrap_i32() as u32;
    let nwritten_ptr = args[3].unwrap_i32() as u32;
    
    let memory = context.get_memory();
    
    // Read IOVec structures from memory
    let iovs = context.read_iovecs(memory, iovs_ptr, iovs_len)?;
    
    // Write data to file descriptor
    let mut total_written = 0u32;
    for iov in iovs {
        let data = memory.view::<u8>()
            .get(iov.ptr as usize..(iov.ptr + iov.len) as usize)
            .ok_or(WasixError::MemoryOutOfBounds)?;
        
        match fd {
            1 => { // stdout
                print!("{}", String::from_utf8_lossy(&data));
                total_written += iov.len;
            },
            2 => { // stderr
                eprint!("{}", String::from_utf8_lossy(&data));
                total_written += iov.len;
            },
            _ => return Err(WasixError::InvalidFileDescriptor(fd)),
        }
    }
    
    // Write result back to memory
    memory.view::<u32>()
        .get(nwritten_ptr as usize)
        .ok_or(WasixError::MemoryOutOfBounds)?
        .store(total_written);
        
    Ok(vec![Value::I32(0)]) // Success errno
}
```

### 2. Performance Considerations

Native implementations provide significant performance benefits:

- **Direct memory access** without WASM import overhead
- **Optimized system calls** using native OS interfaces
- **Reduced context switching** between WASM and host
- **Better error handling** with detailed diagnostics

### 3. Compatibility Layer

Native functions maintain compatibility with standard WASIX:

- **Same function signatures** as WASIX specification
- **Identical error codes** and return values
- **Compatible memory layouts** for IOVec and other structures
- **Graceful fallback** to standard imports when needed

## Integration with Beanstalk Language

### 1. AST to WIR Lowering

Print statements in Beanstalk are lowered to WASIX host calls:

```beanstalk
print("Hello, WASIX!")
```

Becomes:

```rust
// WIR representation
Statement::HostCall {
    function: "wasix_fd_write".to_string(),
    args: vec![
        Operand::Constant(1), // stdout fd
        Operand::Local(iovec_ptr),
        Operand::Constant(1), // iovec count
        Operand::Local(nwritten_ptr),
    ],
    destination: None,
}
```

### 2. Type System Integration

WASIX functions integrate with Beanstalk's type system:

- **String types** automatically converted to IOVec structures
- **Integer types** mapped to appropriate WASM value types
- **Error handling** integrated with Beanstalk's error system
- **Memory safety** ensured through borrow checking

### 3. Compilation Pipeline

WASIX integration spans the entire compilation pipeline:

1. **AST Stage**: Print nodes identified and validated
2. **WIR Stage**: Host calls generated with proper memory management
3. **Codegen Stage**: WASM imports and native calls generated
4. **Runtime Stage**: Native functions executed or imports resolved

## Debugging and Diagnostics

### 1. Error Messages

WASIX errors provide detailed context:

```
error: WASIX fd_write failed with invalid file descriptor
  --> example.bs:5:1
5 | print("hello")
  |       ^^^^^^^ attempted to write to file descriptor 3
note: WASIX supports stdout (1) and stderr (2) for print operations
help: ensure print() statements target valid output streams
```

### 2. Memory Debugging

Memory allocation tracking helps debug WASIX operations:

- **Allocation tracking** for all WASIX memory regions
- **Bounds checking** for memory access operations
- **Leak detection** for unreleased memory regions
- **Alignment validation** for WASIX data structures

### 3. Performance Profiling

WASIX operations can be profiled for performance analysis:

- **Native vs import timing** comparisons
- **Memory allocation overhead** measurement
- **System call frequency** tracking
- **Error rate monitoring** for diagnostics

This architecture provides a robust foundation for WASIX integration while maintaining Beanstalk's goals of simplicity and performance.