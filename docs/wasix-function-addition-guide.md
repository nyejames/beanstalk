# WASIX Function Addition Guide

This guide explains how to add new WASIX functions to the Beanstalk compiler, including native implementations, memory layout requirements, and code generation templates.

## Overview

Adding a new WASIX function involves several steps across the compiler pipeline:

1. **Registry Registration**: Define function signature and native implementation
2. **Memory Layout**: Design memory structures for function parameters
3. **Native Implementation**: Implement the function logic with proper error handling
4. **Code Generation**: Add WASM codegen support for the function
5. **Language Integration**: Connect to Beanstalk language constructs
6. **Testing**: Comprehensive testing of the new functionality

## Step-by-Step Process

### Step 1: Function Registry Registration

**Location**: `src/compiler/host_functions/wasix_registry.rs`

Add your function to the WASIX registry:

```rust
impl WasixFunctionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
            native_functions: HashMap::new(),
        };
        
        // Existing functions...
        
        // Add your new function
        registry.register_function("your_function", WasixFunctionDef {
            module: "wasix_32v1".to_string(),
            name: "your_wasix_function".to_string(),
            parameters: vec![ValType::I32, ValType::I32], // Define parameter types
            returns: vec![ValType::I32], // Define return types
            native_impl: Some(native_your_function), // Link to native implementation
            func_index: None,
        });
        
        registry
    }
}
```

**Function Signature Guidelines**:
- Use WASM value types: `ValType::I32`, `ValType::I64`, `ValType::F32`, `ValType::F64`
- Follow WASIX naming conventions (snake_case)
- Match WASIX specification signatures exactly
- Include proper error return codes (typically `i32` errno)

### Step 2: Memory Layout Design

**Location**: `src/runtime/wasix_memory.rs`

Design memory structures for your function parameters:

```rust
// Example: File read operation
#[repr(C)]
pub struct FileReadParams {
    /// File descriptor
    pub fd: u32,
    /// Buffer pointer in linear memory
    pub buf_ptr: u32,
    /// Buffer size
    pub buf_len: u32,
    /// Result pointer for bytes read
    pub nread_ptr: u32,
}

impl WasixMemoryManager {
    /// Allocate memory for file read parameters
    pub fn allocate_file_read_params(&mut self, fd: u32, buf_size: u32) -> (u32, u32, u32) {
        // Allocate buffer with proper alignment
        let buf_ptr = self.allocate(buf_size, 1); // 1-byte aligned for data
        
        // Allocate result storage
        let nread_ptr = self.allocate(4, 4); // 4-byte aligned for u32
        
        (buf_ptr, buf_size, nread_ptr)
    }
}
```

**Memory Layout Requirements**:

| Data Type | Alignment | Size | Notes |
|-----------|-----------|------|-------|
| `u8` data | 1 byte | Variable | String data, byte arrays |
| `u32` integers | 4 bytes | 4 bytes | File descriptors, lengths |
| `u64` integers | 8 bytes | 8 bytes | Timestamps, large values |
| IOVec structures | 8 bytes | 8 bytes | I/O vector arrays |
| Pointers | 4 bytes | 4 bytes | Memory addresses (32-bit) |

### Step 3: Native Implementation

**Location**: `src/runtime/wasix_native_functions.rs`

Implement the native function with proper error handling:

```rust
/// Native implementation of file read operation
pub fn native_file_read(context: &mut WasixContext, args: &[Value]) -> Result<Vec<Value>, WasixError> {
    // 1. Validate argument count
    if args.len() != 4 {
        return Err(WasixError::InvalidArgumentCount);
    }
    
    // 2. Extract parameters
    let fd = args[0].unwrap_i32() as u32;
    let buf_ptr = args[1].unwrap_i32() as u32;
    let buf_len = args[2].unwrap_i32() as u32;
    let nread_ptr = args[3].unwrap_i32() as u32;
    
    // 3. Validate file descriptor
    let file = context.fd_table.get(fd)
        .ok_or(WasixError::InvalidFileDescriptor(fd))?;
    
    // 4. Get memory access
    let memory = context.get_memory();
    
    // 5. Validate memory bounds
    let buf_end = buf_ptr.checked_add(buf_len)
        .ok_or(WasixError::MemoryOutOfBounds)?;
    if buf_end > memory.size().bytes().0 as u32 {
        return Err(WasixError::MemoryOutOfBounds);
    }
    
    // 6. Perform the operation
    let mut buffer = vec![0u8; buf_len as usize];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n as u32,
        Err(e) => return Ok(vec![Value::I32(wasix_errno_from_io_error(e))]),
    };
    
    // 7. Write data to WASM memory
    let memory_view = memory.view::<u8>();
    for (i, &byte) in buffer[..bytes_read as usize].iter().enumerate() {
        memory_view.get(buf_ptr as usize + i)
            .ok_or(WasixError::MemoryOutOfBounds)?
            .store(byte);
    }
    
    // 8. Write result to nread_ptr
    memory.view::<u32>()
        .get(nread_ptr as usize)
        .ok_or(WasixError::MemoryOutOfBounds)?
        .store(bytes_read);
    
    // 9. Return success
    Ok(vec![Value::I32(0)]) // WASIX_ESUCCESS
}

/// Convert I/O error to WASIX errno
fn wasix_errno_from_io_error(error: std::io::Error) -> i32 {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => 2,        // WASIX_ENOENT
        ErrorKind::PermissionDenied => 13, // WASIX_EACCES
        ErrorKind::InvalidInput => 22,   // WASIX_EINVAL
        ErrorKind::UnexpectedEof => 5,   // WASIX_EIO
        _ => 5,                          // WASIX_EIO (generic I/O error)
    }
}
```

**Native Implementation Guidelines**:
- Always validate argument count first
- Check memory bounds before accessing WASM memory
- Use proper WASIX error codes (see WASIX specification)
- Handle all possible error conditions gracefully
- Write results back to WASM memory using provided pointers
- Return `Vec<Value>` with appropriate WASIX errno codes

### Step 4: Code Generation Support

**Location**: `src/compiler/codegen/wasix_codegen.rs`

Add WASM code generation for your function:

```rust
impl WasmModule {
    /// Add WASIX imports for your function
    pub fn add_wasix_imports(&mut self) -> Result<(), CompileError> {
        // Existing imports...
        
        // Add your function import
        let file_read_type = self.module.type_section().function(
            [ValType::I32, ValType::I32, ValType::I32, ValType::I32], // fd, buf_ptr, buf_len, nread_ptr
            [ValType::I32] // errno result
        );
        
        self.module.import_section().import(
            "wasix_32v1",
            "fd_read", // WASIX function name
            EntityType::Function(file_read_type)
        );
        
        Ok(())
    }
    
    /// Lower host call for your function
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
            "file_read" => self.lower_wasix_file_read(args, function, local_map), // Add your function
            _ => return_compiler_error!("Unsupported WASIX function: {}", function_name),
        }
    }
    
    /// Lower file read operation to WASM
    pub fn lower_wasix_file_read(
        &mut self,
        args: &[Operand],
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // 1. Validate arguments
        if args.len() != 3 {
            return_compiler_error!("file_read expects 3 arguments: fd, buffer_size, result_var");
        }
        
        // 2. Get file descriptor
        let fd_operand = &args[0];
        self.load_operand(fd_operand, function, local_map)?;
        
        // 3. Allocate buffer memory
        let buffer_size = match &args[1] {
            Operand::Constant(size) => *size as u32,
            _ => return_compiler_error!("file_read buffer size must be constant"),
        };
        
        let (buf_ptr, buf_len, nread_ptr) = self.allocate_file_read_memory(buffer_size, function)?;
        
        // 4. Generate WASIX call
        function.instruction(&Instruction::LocalGet(buf_ptr));
        function.instruction(&Instruction::I32Const(buf_len as i32));
        function.instruction(&Instruction::LocalGet(nread_ptr));
        function.instruction(&Instruction::Call(self.wasix_file_read_func_index));
        
        // 5. Handle result
        if let Some(dest) = destination {
            let dest_local = local_map.get_local(dest)?;
            function.instruction(&Instruction::LocalSet(dest_local));
        } else {
            function.instruction(&Instruction::Drop);
        }
        
        Ok(())
    }
    
    /// Allocate memory for file read operation
    fn allocate_file_read_memory(&mut self, buffer_size: u32, function: &mut Function) -> Result<(u32, u32, u32), CompileError> {
        // Allocate buffer
        let buf_ptr = self.memory_manager.allocate(buffer_size, 1);
        
        // Allocate result storage
        let nread_ptr = self.memory_manager.allocate(4, 4);
        
        Ok((buf_ptr, buffer_size, nread_ptr))
    }
}
```

### Step 5: Language Integration

**Location**: `src/compiler/parsers/statements/` or appropriate parser module

Connect your function to Beanstalk language constructs:

```rust
// Example: Adding file read as a built-in function
impl Parser {
    pub fn parse_builtin_function_call(&mut self, name: &str, args: Vec<AstNode>) -> Result<AstNode, CompileError> {
        match name {
            "print" => self.parse_print_call(args),
            "file_read" => self.parse_file_read_call(args), // Add your function
            _ => return_rule_error!(self.current_location(), "Unknown built-in function: {}", name),
        }
    }
    
    fn parse_file_read_call(&mut self, args: Vec<AstNode>) -> Result<AstNode, CompileError> {
        // Validate argument count
        if args.len() != 2 {
            return_syntax_error!(self.current_location(), "file_read expects 2 arguments: file_descriptor, buffer_size");
        }
        
        // Validate argument types
        let fd_arg = &args[0];
        let size_arg = &args[1];
        
        // Create host call AST node
        Ok(AstNode {
            location: self.current_location(),
            node_type: AstNodeType::HostCall {
                function: "file_read".to_string(),
                args: args,
                return_type: DataType::Int, // Returns bytes read
            },
        })
    }
}
```

### Step 6: Testing

**Location**: `src/compiler_tests/wasix_function_tests.rs`

Create comprehensive tests for your function:

```rust
#[cfg(test)]
mod wasix_file_read_tests {
    use super::*;
    
    #[test]
    fn test_file_read_native_implementation() {
        let mut context = WasixContext::new().unwrap();
        
        // Setup test file
        let test_data = b"Hello, WASIX!";
        let fd = context.create_test_file(test_data).unwrap();
        
        // Allocate memory
        let buf_ptr = 0x1000;
        let buf_len = 32;
        let nread_ptr = 0x2000;
        
        // Call native function
        let args = vec![
            Value::I32(fd as i32),
            Value::I32(buf_ptr as i32),
            Value::I32(buf_len as i32),
            Value::I32(nread_ptr as i32),
        ];
        
        let result = native_file_read(&mut context, &args).unwrap();
        
        // Verify success
        assert_eq!(result, vec![Value::I32(0)]); // WASIX_ESUCCESS
        
        // Verify data was read
        let memory = context.get_memory();
        let nread = memory.view::<u32>().get(nread_ptr as usize).unwrap().load();
        assert_eq!(nread, test_data.len() as u32);
        
        // Verify buffer contents
        let buffer: Vec<u8> = (0..nread)
            .map(|i| memory.view::<u8>().get(buf_ptr as usize + i as usize).unwrap().load())
            .collect();
        assert_eq!(&buffer, test_data);
    }
    
    #[test]
    fn test_file_read_invalid_fd() {
        let mut context = WasixContext::new().unwrap();
        
        let args = vec![
            Value::I32(999), // Invalid FD
            Value::I32(0x1000),
            Value::I32(32),
            Value::I32(0x2000),
        ];
        
        let result = native_file_read(&mut context, &args);
        assert!(matches!(result, Err(WasixError::InvalidFileDescriptor(999))));
    }
    
    #[test]
    fn test_file_read_memory_bounds() {
        let mut context = WasixContext::new().unwrap();
        let fd = context.create_test_file(b"test").unwrap();
        
        let args = vec![
            Value::I32(fd as i32),
            Value::I32(0xFFFFFF00), // Out of bounds pointer
            Value::I32(32),
            Value::I32(0x2000),
        ];
        
        let result = native_file_read(&mut context, &args);
        assert!(matches!(result, Err(WasixError::MemoryOutOfBounds)));
    }
    
    #[test]
    fn test_file_read_codegen() {
        let source = r#"
            fd = open_file("test.txt")
            bytes_read = file_read(fd, 1024)
            print("Read {} bytes", bytes_read)
        "#;
        
        let compiled = compile_beanstalk_source(source).unwrap();
        
        // Verify WASIX imports are generated
        assert!(compiled.has_wasix_import("fd_read"));
        
        // Verify proper memory allocation
        assert!(compiled.has_memory_allocation_for("file_read_buffer"));
        
        // Verify function call generation
        assert!(compiled.has_wasix_function_call("fd_read"));
    }
}
```

## Function Templates

### Template 1: Simple I/O Function

For functions that read/write data with file descriptors:

```rust
// Registry registration
registry.register_function("your_io_function", WasixFunctionDef {
    module: "wasix_32v1".to_string(),
    name: "wasix_io_function".to_string(),
    parameters: vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
    returns: vec![ValType::I32],
    native_impl: Some(native_your_io_function),
    func_index: None,
});

// Native implementation template
pub fn native_your_io_function(context: &mut WasixContext, args: &[Value]) -> Result<Vec<Value>, WasixError> {
    // 1. Validate arguments
    if args.len() != 4 {
        return Err(WasixError::InvalidArgumentCount);
    }
    
    // 2. Extract parameters
    let fd = args[0].unwrap_i32() as u32;
    let data_ptr = args[1].unwrap_i32() as u32;
    let data_len = args[2].unwrap_i32() as u32;
    let result_ptr = args[3].unwrap_i32() as u32;
    
    // 3. Validate file descriptor
    let file = context.fd_table.get(fd)
        .ok_or(WasixError::InvalidFileDescriptor(fd))?;
    
    // 4. Perform operation with memory bounds checking
    let memory = context.get_memory();
    // ... implementation details ...
    
    // 5. Return result
    Ok(vec![Value::I32(0)]) // Success
}
```

### Template 2: System Information Function

For functions that return system information:

```rust
// Registry registration
registry.register_function("get_system_info", WasixFunctionDef {
    module: "wasix_32v1".to_string(),
    name: "clock_time_get".to_string(),
    parameters: vec![ValType::I32, ValType::I64, ValType::I32],
    returns: vec![ValType::I32],
    native_impl: Some(native_get_system_info),
    func_index: None,
});

// Native implementation template
pub fn native_get_system_info(context: &mut WasixContext, args: &[Value]) -> Result<Vec<Value>, WasixError> {
    // 1. Validate arguments
    if args.len() != 3 {
        return Err(WasixError::InvalidArgumentCount);
    }
    
    // 2. Extract parameters
    let clock_id = args[0].unwrap_i32() as u32;
    let precision = args[1].unwrap_i64() as u64;
    let time_ptr = args[2].unwrap_i32() as u32;
    
    // 3. Get system information
    let current_time = match clock_id {
        0 => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
        _ => return Ok(vec![Value::I32(22)]), // WASIX_EINVAL
    };
    
    // 4. Write result to memory
    let memory = context.get_memory();
    memory.view::<u64>()
        .get(time_ptr as usize)
        .ok_or(WasixError::MemoryOutOfBounds)?
        .store(current_time);
    
    // 5. Return success
    Ok(vec![Value::I32(0)])
}
```

### Template 3: Network Function

For functions that handle networking operations:

```rust
// Registry registration
registry.register_function("socket_create", WasixFunctionDef {
    module: "wasix_32v1".to_string(),
    name: "sock_open".to_string(),
    parameters: vec![ValType::I32, ValType::I32, ValType::I32],
    returns: vec![ValType::I32],
    native_impl: Some(native_socket_create),
    func_index: None,
});

// Native implementation template
pub fn native_socket_create(context: &mut WasixContext, args: &[Value]) -> Result<Vec<Value>, WasixError> {
    // 1. Validate arguments
    if args.len() != 3 {
        return Err(WasixError::InvalidArgumentCount);
    }
    
    // 2. Extract parameters
    let address_family = args[0].unwrap_i32() as u32;
    let socket_type = args[1].unwrap_i32() as u32;
    let fd_ptr = args[2].unwrap_i32() as u32;
    
    // 3. Create socket
    let socket_fd = match context.create_socket(address_family, socket_type) {
        Ok(fd) => fd,
        Err(e) => return Ok(vec![Value::I32(wasix_errno_from_socket_error(e))]),
    };
    
    // 4. Write file descriptor to memory
    let memory = context.get_memory();
    memory.view::<u32>()
        .get(fd_ptr as usize)
        .ok_or(WasixError::MemoryOutOfBounds)?
        .store(socket_fd);
    
    // 5. Return success
    Ok(vec![Value::I32(0)])
}
```

## Common Patterns and Best Practices

### 1. Error Handling

Always use proper WASIX error codes:

```rust
// Common WASIX error codes
const WASIX_ESUCCESS: i32 = 0;   // Success
const WASIX_EBADF: i32 = 9;      // Bad file descriptor
const WASIX_EINVAL: i32 = 22;    // Invalid argument
const WASIX_ENOSYS: i32 = 38;    // Function not implemented
const WASIX_ENOENT: i32 = 2;     // No such file or directory
const WASIX_EACCES: i32 = 13;    // Permission denied
```

### 2. Memory Safety

Always validate memory access:

```rust
// Check bounds before accessing memory
let end_ptr = ptr.checked_add(len)
    .ok_or(WasixError::MemoryOutOfBounds)?;
if end_ptr > memory.size().bytes().0 as u32 {
    return Err(WasixError::MemoryOutOfBounds);
}
```

### 3. Resource Management

Properly manage system resources:

```rust
// Track file descriptors
context.fd_table.insert(fd, file_handle);

// Clean up on errors
if let Err(e) = operation() {
    context.cleanup_resources();
    return Err(e);
}
```

### 4. Performance Optimization

Optimize for common cases:

```rust
// Use stack allocation for small buffers
let mut small_buffer = [0u8; 256];
let buffer = if len <= 256 {
    &mut small_buffer[..len]
} else {
    // Heap allocation for large buffers
    &mut vec![0u8; len]
};
```

## Testing Guidelines

### 1. Unit Tests

Test each function in isolation:
- Valid parameter combinations
- Invalid parameters and error conditions
- Memory boundary conditions
- Resource cleanup

### 2. Integration Tests

Test function integration with the compiler:
- Code generation correctness
- Memory layout validation
- Runtime execution
- Error propagation

### 3. Performance Tests

Measure function performance:
- Native vs import comparison
- Memory allocation overhead
- System call frequency
- Scalability with data size

This guide provides a comprehensive framework for adding new WASIX functions to the Beanstalk compiler while maintaining consistency, performance, and reliability.