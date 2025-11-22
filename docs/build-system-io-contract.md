# Beanstalk Build System IO Contract

## Overview

Every Beanstalk build system MUST provide an `Io` struct (with capital I) that implements the IO interface for the target environment. This contract ensures consistent IO capabilities across all Beanstalk projects while allowing build systems to adapt to their specific runtime environments.

## Mandatory Requirements

### 1. Io Struct Provision

Every build system MUST provide an `Io` struct that serves as the namespace for all IO operations in the compiled module.

```beanstalk
-- Conceptual structure (implementation varies by build system)
Io = struct:
    -- Mandatory function
    io |content CoerceToString| -> Void
    
    -- Optional functions (based on target environment)
    write |content CoerceToString| -> Void
    error |content CoerceToString| -> Void
    read |path String| -> Result<String, Error>
;
```

### 2. Mandatory io() Function

The `io()` function is the ONLY mandatory function that every build system must provide. This function:

- **Signature**: `io |content CoerceToString| -> Void`
- **Behavior**: Outputs content to the default output stream with an automatic newline
- **Type Handling**: Accepts any type through `CoerceToString` automatic conversion
- **Newline**: MUST append a newline character (`\n`) to all output
- **Target Mapping**:
  - Web: `console.log(content)`
  - Native: `stdout` with newline
  - Embedded: Serial output or equivalent

**Example Usage**:
```beanstalk
io("Hello, World!")           -- Prints: Hello, World!\n
io(42)                        -- Prints: 42\n
io([: Hello name])            -- Prints: Hello Alice\n (if name = "Alice")
```

## Optional Functions

Build systems MAY provide additional IO functions based on their target environment capabilities:

### write() - Output Without Newline

```beanstalk
write |content CoerceToString| -> Void
```

- Outputs content WITHOUT automatic newline
- Useful for building output incrementally
- Not required for basic Beanstalk programs

**Example**:
```beanstalk
write("Loading")
write(".")
write(".")
write(".")
io("Done!")
-- Output: Loading...Done!\n
```

### error() - Error Output Stream

```beanstalk
error |content CoerceToString| -> Void
```

- Outputs to error stream (stderr or equivalent)
- Includes automatic newline like `io()`
- Not available in all environments (e.g., some embedded systems)

**Example**:
```beanstalk
error("Fatal error: Database connection failed")
-- Output to stderr: Fatal error: Database connection failed\n
```

### read() - File/Resource Reading

```beanstalk
read |path String| -> Result<String, Error>
```

- Reads content from file system or resource
- Returns Result type for error handling
- Not available in sandboxed environments (e.g., web without file API)

**Example**:
```beanstalk
content = read("config.txt") !err:
    error([: Failed to read config: err])
    return
;
io([: Config loaded: content])
```

## Build System Implementation Guide

### Minimal Implementation (Required)

A minimal build system MUST provide at least the `io()` function:

```rust
// Rust host function registration example
pub fn register_minimal_io(registry: &mut HostFunctionRegistry) -> Result<(), CompileError> {
    // Register io function
    let io_function = HostFunctionDef::new(
        "io",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString,
            ownership: Ownership::Borrowed,
        }],
        vec![], // Void return
        "beanstalk_io",
        "io",
        "Output content to stdout with automatic newline",
        string_table,
    );
    
    registry.register_function(io_function)?;
    
    // Register JS binding for web targets
    let io_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "io",
        vec![ValType::I32, ValType::I32], // ptr, len
        vec![],
        "Output to console.log with newline",
    );
    
    registry.register_js_mapping(string_table.intern("io"), io_js_binding)?;
    
    Ok(())
}
```

### Full Implementation (Optional)

A full-featured build system MAY provide all IO functions:

```rust
pub fn register_full_io(registry: &mut HostFunctionRegistry) -> Result<(), CompileError> {
    // Register io() - mandatory
    register_io_function(registry)?;
    
    // Register write() - optional
    register_write_function(registry)?;
    
    // Register error() - optional
    register_error_function(registry)?;
    
    // Register read() - optional
    register_read_function(registry)?;
    
    Ok(())
}
```

## Target Environment Examples

### Web Build System

```javascript
// Minimal web implementation
const beanstalk_io = {
    io: (ptr, len) => {
        const bytes = new Uint8Array(wasmMemory.buffer, ptr, len);
        const text = new TextDecoder().decode(bytes);
        console.log(text); // Automatic newline from console.log
    }
};

// Full web implementation
const beanstalk_io = {
    io: (ptr, len) => {
        const text = readString(ptr, len);
        console.log(text);
    },
    write: (ptr, len) => {
        const text = readString(ptr, len);
        process.stdout.write(text); // No newline
    },
    error: (ptr, len) => {
        const text = readString(ptr, len);
        console.error(text);
    },
    read: async (pathPtr, pathLen) => {
        const path = readString(pathPtr, pathLen);
        const response = await fetch(path);
        return await response.text();
    }
};
```

### Native Build System

```rust
// Minimal native implementation
fn register_native_io() {
    host_functions.register("io", |content: &str| {
        println!("{}", content); // Automatic newline
    });
}

// Full native implementation
fn register_native_io() {
    host_functions.register("io", |content: &str| {
        println!("{}", content);
    });
    
    host_functions.register("write", |content: &str| {
        print!("{}", content); // No newline
        std::io::stdout().flush().unwrap();
    });
    
    host_functions.register("error", |content: &str| {
        eprintln!("{}", content);
    });
    
    host_functions.register("read", |path: &str| -> Result<String, String> {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path, e))
    });
}
```

### Embedded Build System

```rust
// Minimal embedded implementation (serial output only)
fn register_embedded_io() {
    host_functions.register("io", |content: &str| {
        serial_println!("{}", content); // Output to serial with newline
    });
}

// Note: write(), error(), and read() typically not available in embedded systems
```

## Compiler Validation

The Beanstalk compiler validates that the `io()` function is available during compilation:

1. **Compilation Start**: Compiler checks host function registry for `io()` function
2. **Missing Function**: If `io()` is not registered, compilation fails with clear error
3. **Error Message**: Suggests checking build system configuration

**Error Example**:
```
Error: Build system does not provide required 'io()' function
  --> project configuration
help: Every Beanstalk build system must provide at minimum the io() function for basic printing
suggestion: Check your build system configuration and ensure the Io struct includes the io() function
```

## Portability Guarantees

### Guaranteed Across All Build Systems

- `io()` function is ALWAYS available
- `io()` ALWAYS appends newline
- `io()` ALWAYS accepts any type through CoerceToString

### Not Guaranteed (Optional)

- `write()` may not be available
- `error()` may not be available
- `read()` may not be available

### Writing Portable Code

```beanstalk
-- This code works on ALL build systems
io("Hello, World!")
io(42)
io([: Result: result])

-- This code requires checking for availability
-- (Future: conditional compilation or capability detection)
if has_capability("write"):
    write("Progress: ")
    write(percent)
    io("%")
;
```

## Build System Developer Checklist

When creating a new Beanstalk build system:

- [ ] Provide `Io` struct (capital I)
- [ ] Implement mandatory `io()` function with signature: `io |content CoerceToString| -> Void`
- [ ] Ensure `io()` appends newline to all output
- [ ] Register `io()` in host function registry
- [ ] Provide JS bindings for web targets (if applicable)
- [ ] Document which optional functions are available
- [ ] Test that `io()` works with all types (Int, Float, Bool, String, templates)
- [ ] Verify compiler validation passes with your build system

## Future Extensions

### Capability Detection (Planned)

```beanstalk
-- Future syntax for checking optional functions
if Io.has_method("write"):
    Io.write("No newline")
;

if Io.has_method("read"):
    content = Io.read("file.txt") !err:
        io([: Error: err])
    ;
;
```

### Additional Optional Methods (Under Consideration)

- `flush()` - Flush output buffers
- `write_bytes()` - Binary output
- `read_bytes()` - Binary input
- `exists()` - Check file existence
- `list_dir()` - Directory listing

## Summary

The Beanstalk Build System IO Contract ensures:

1. **Universal Compatibility**: Every Beanstalk program can use `io()` regardless of target
2. **Flexibility**: Build systems can provide additional IO functions based on capabilities
3. **Simplicity**: Minimal implementation requires only one function
4. **Extensibility**: Contract allows for future additions without breaking changes

Build system developers should prioritize implementing `io()` correctly, then add optional functions as their target environment allows.
