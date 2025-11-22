# Beanstalk Build System Examples

This directory contains comprehensive examples of Beanstalk build system implementations for different target environments.

## Overview

Every Beanstalk build system must comply with the [Build System IO Contract](../build-system-io-contract.md), which requires providing at minimum the `io()` function for basic printing.

## Available Examples

### Web Build Systems

#### [Minimal Web Build System](./build-system-minimal-web.md)
- **Target**: Web browsers and Node.js
- **Features**: Only the mandatory `io()` function
- **Use Case**: Simple web applications, learning, prototyping
- **Runtime**: JavaScript with console.log
- **Size**: Minimal (~50 lines of JS)

#### [Full Web Build System](./build-system-full-web.md)
- **Target**: Web browsers and Node.js
- **Features**: All IO functions (io, write, error, read)
- **Use Case**: Complex web applications with full IO capabilities
- **Runtime**: JavaScript with fetch API, console methods
- **Size**: Full-featured (~200 lines of JS)

### Native Build Systems

#### [Minimal Native Build System](./build-system-minimal-native.md)
- **Target**: Native executables via Wasmer/Wasmtime
- **Features**: Only the mandatory `io()` function
- **Use Case**: Simple CLI tools, learning, embedded systems
- **Runtime**: Rust with stdout
- **Size**: Minimal (~100 lines of Rust)

#### [Full Native Build System](./build-system-full-native.md)
- **Target**: Native executables via Wasmer/Wasmtime
- **Features**: All IO functions (io, write, error, read)
- **Use Case**: Complex CLI applications, system utilities
- **Runtime**: Rust with full file system access
- **Size**: Full-featured (~300 lines of Rust)

## Quick Start

### Web Development

```bash
# 1. Choose your build system
# Minimal: Only io() function
# Full: All IO functions (io, write, error, read)

# 2. Write your Beanstalk program
cat > hello.bst << 'EOF'
io("Hello, World!")
io(42)
EOF

# 3. Compile
beanstalk build hello.bst --target web --output hello.wasm

# 4. Create HTML file with runtime
# See examples for complete HTML templates

# 5. Open in browser
open index.html
```

### Native Development

```bash
# 1. Choose your build system
# Minimal: Only io() function
# Full: All IO functions (io, write, error, read)

# 2. Write your Beanstalk program
cat > hello.bst << 'EOF'
io("Hello, World!")
io(42)
EOF

# 3. Compile
beanstalk build hello.bst --target native --output hello.wasm

# 4. Run with Wasmer
wasmer run hello.wasm

# Or run with custom runtime
cargo run --bin beanstalk-runner -- hello.wasm
```

## Build System Contract Compliance

All examples in this directory comply with the Beanstalk Build System IO Contract:

| Requirement | Minimal Web | Full Web | Minimal Native | Full Native |
|-------------|-------------|----------|----------------|-------------|
| Provides Io struct | ✅ | ✅ | ✅ | ✅ |
| Mandatory io() | ✅ | ✅ | ✅ | ✅ |
| io() accepts CoerceToString | ✅ | ✅ | ✅ | ✅ |
| io() appends newline | ✅ | ✅ | ✅ | ✅ |
| Compiler validation passes | ✅ | ✅ | ✅ | ✅ |
| Optional write() | ❌ | ✅ | ❌ | ✅ |
| Optional error() | ❌ | ✅ | ❌ | ✅ |
| Optional read() | ❌ | ✅ | ❌ | ✅ |

## Choosing the Right Build System

### Use Minimal Build System When:
- Building simple applications
- Learning Beanstalk basics
- Prototyping without complex IO
- Targeting constrained environments
- Minimizing runtime size

### Use Full Build System When:
- Building complex applications
- Need fine-grained output control
- Require error handling and reporting
- Need file/resource reading
- Want all available IO capabilities

## Feature Comparison

### Mandatory Features (All Build Systems)

| Feature | Description | Example |
|---------|-------------|---------|
| `io()` | Output with newline | `io("Hello")` |
| CoerceToString | Automatic type conversion | `io(42)` → `"42"` |
| Template support | String interpolation | `io([: Count: n])` |

### Optional Features (Full Build Systems Only)

| Feature | Description | Example |
|---------|-------------|---------|
| `write()` | Output without newline | `write("Loading...")` |
| `error()` | Error stream output | `error("Failed!")` |
| `read()` | File/resource reading | `read("config.txt")` |

## Implementation Patterns

### Minimal Implementation Pattern

```rust
// 1. Create registry with mandatory io() only
let mut registry = HostFunctionRegistry::new_with_backend(backend);

// 2. Register io() function
let io_function = HostFunctionDef::new(
    "io",
    vec![/* CoerceToString parameter */],
    vec![], // Void return
    "beanstalk_io",
    "io",
    "Output with newline",
    string_table,
);
registry.register_function(io_function, string_table)?;

// 3. Validate io() availability
registry.validate_io_availability(string_table)?;
```

### Full Implementation Pattern

```rust
// 1. Create registry
let mut registry = HostFunctionRegistry::new_with_backend(backend);

// 2. Register all IO functions
register_io_function(&mut registry, string_table)?;
register_write_function(&mut registry, string_table)?;
register_error_function(&mut registry, string_table)?;
register_read_function(&mut registry, string_table)?;

// 3. Validate io() availability
registry.validate_io_availability(string_table)?;
```

## Testing Your Build System

### Validation Tests

```rust
#[test]
fn test_build_system_contract_compliance() {
    let mut string_table = StringTable::new();
    let registry = create_your_build_system(&mut string_table)
        .expect("Failed to create build system");

    // Test 1: io() function is registered
    let io_name = string_table.intern("io");
    assert!(registry.has_function(&io_name));

    // Test 2: Validation passes
    assert!(registry.validate_io_availability(&string_table).is_ok());
}
```

### Integration Tests

```beanstalk
-- test-io.bst
io("Test 1: Basic output")
io(42)
io([: Test 2: Template with value 42])
```

```bash
# Run integration test
beanstalk build test-io.bst --output test.wasm
wasmer run test.wasm

# Expected output:
# Test 1: Basic output
# 42
# Test 2: Template with value 42
```

## Extending Build Systems

### Adding Custom IO Functions

```rust
// Example: Add a custom log() function
fn register_log_function(
    registry: &mut HostFunctionRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompileError> {
    let log_function = HostFunctionDef::new(
        "log",
        vec![
            BasicParameter {
                name: string_table.intern("level"),
                data_type: DataType::String,
                ownership: Ownership::ImmutableReference,
            },
            BasicParameter {
                name: string_table.intern("message"),
                data_type: DataType::CoerceToString,
                ownership: Ownership::ImmutableReference,
            },
        ],
        vec![],
        "beanstalk_io",
        "log",
        "Log message with level",
        string_table,
    );

    registry.register_function(log_function, string_table)
}
```

### Adding Environment-Specific Functions

```rust
// Web-specific: DOM manipulation
fn register_dom_functions(registry: &mut HostFunctionRegistry) {
    // register_set_inner_html()
    // register_add_event_listener()
    // register_query_selector()
}

// Native-specific: System calls
fn register_system_functions(registry: &mut HostFunctionRegistry) {
    // register_get_env()
    // register_set_env()
    // register_spawn_process()
}
```

## Performance Considerations

### Minimal Build Systems
- **Startup**: < 1ms
- **Memory**: < 100KB
- **Binary Size**: < 50KB
- **Best For**: Simple applications, embedded systems

### Full Build Systems
- **Startup**: < 5ms
- **Memory**: < 500KB
- **Binary Size**: < 200KB
- **Best For**: Complex applications, full-featured tools

## Troubleshooting

### Common Issues

#### 1. io() Function Not Found

```
Error: Build system does not provide required 'io()' function
```

**Solution**: Ensure your build system registers the io() function:
```rust
registry.register_function(io_function, string_table)?;
registry.validate_io_availability(string_table)?;
```

#### 2. Memory Access Errors

```
Error: Failed to read from WASM memory
```

**Solution**: Add bounds checking:
```rust
if ptr < 0 || len < 0 || len > MAX_STRING_LENGTH {
    return Err("Invalid memory access");
}
```

#### 3. Missing Entry Point

```
Error: WASM module has no entry point (_start function)
```

**Solution**: Ensure your WASM module exports `_start`:
```rust
let start_func = instance.exports.get_function("_start")?;
```

## Resources

- [Build System IO Contract](../build-system-io-contract.md) - Complete specification
- [Beanstalk Language Guide](../../README.md) - Language documentation
- [Wasmer Documentation](https://docs.wasmer.io/) - WASM runtime
- [WebAssembly Specification](https://webassembly.github.io/spec/) - WASM spec

## Contributing

To add a new build system example:

1. Follow the Build System IO Contract
2. Implement at minimum the io() function
3. Add validation tests
4. Document features and limitations
5. Provide usage examples
6. Submit a pull request

## License

All examples are provided under the same license as the Beanstalk compiler.
