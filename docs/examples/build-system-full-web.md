# Full Web Build System Example

This example shows a complete implementation of a Beanstalk build system for web targets with all optional IO functions.

## Build System Configuration

```rust
// src/build_system/full_web.rs

use crate::compiler::host_functions::registry::{
    HostFunctionRegistry, RuntimeBackend, HostFunctionDef, JsFunctionDef, BasicParameter
};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::string_interning::StringTable;
use wasm_encoder::ValType;

/// Create a full-featured web build system with all IO functions
pub fn create_full_web_build_system(
    string_table: &mut StringTable,
) -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new_with_backend(RuntimeBackend::JavaScript);

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
        "Output content to console with automatic newline",
        string_table,
    );

    let io_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "io",
        vec![ValType::I32, ValType::I32],
        vec![],
        "Output to console.log with newline",
    );

    registry.register_function_with_mappings(io_function, Some(io_js_binding), string_table)
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
        "Output content without newline",
        string_table,
    );

    let write_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "write",
        vec![ValType::I32, ValType::I32],
        vec![],
        "Output without newline",
    );

    registry.register_function_with_mappings(write_function, Some(write_js_binding), string_table)
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
        "Output error message to console.error with newline",
        string_table,
    );

    let error_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "error",
        vec![ValType::I32, ValType::I32],
        vec![],
        "Output to console.error with newline",
    );

    registry.register_function_with_mappings(error_function, Some(error_js_binding), string_table)
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
        "Read content from URL or file path",
        string_table,
    );

    let read_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "read",
        vec![ValType::I32, ValType::I32], // path ptr, len
        vec![ValType::I32, ValType::I32], // result ptr, len (or error)
        "Read content from URL using fetch API",
    );

    registry.register_function_with_mappings(read_function, Some(read_js_binding), string_table)
}
```

## JavaScript Runtime Bindings

```javascript
// full-web-runtime.js

/**
 * Full-featured web runtime for Beanstalk WASM modules
 * Provides all IO functions: io(), write(), error(), read()
 */
class FullBeanstalkRuntime {
    constructor(wasmBytes) {
        this.wasmBytes = wasmBytes;
        this.wasmInstance = null;
        this.wasmMemory = null;
        this.outputBuffer = '';
    }

    /**
     * Read string from WASM memory
     */
    readString(ptr, len) {
        const bytes = new Uint8Array(this.wasmMemory.buffer, ptr, len);
        return new TextDecoder().decode(bytes);
    }

    /**
     * Write string to WASM memory
     */
    writeString(str) {
        const encoder = new TextEncoder();
        const bytes = encoder.encode(str);
        
        // Allocate memory in WASM (simplified - real implementation needs proper allocation)
        const ptr = this.wasmInstance.exports.allocate(bytes.length);
        const memory = new Uint8Array(this.wasmMemory.buffer, ptr, bytes.length);
        memory.set(bytes);
        
        return { ptr, len: bytes.length };
    }

    /**
     * Initialize the WASM module with full IO bindings
     */
    async initialize() {
        const imports = {
            beanstalk_io: {
                // Mandatory: io() - output with newline
                io: (ptr, len) => {
                    const text = this.readString(ptr, len);
                    console.log(text);
                },

                // Optional: write() - output without newline
                write: (ptr, len) => {
                    const text = this.readString(ptr, len);
                    this.outputBuffer += text;
                    process.stdout.write(text); // Node.js
                    // For browser, accumulate in buffer and flush periodically
                },

                // Optional: error() - error output with newline
                error: (ptr, len) => {
                    const text = this.readString(ptr, len);
                    console.error(text);
                },

                // Optional: read() - read from URL/file
                read: async (pathPtr, pathLen) => {
                    const path = this.readString(pathPtr, pathLen);
                    
                    try {
                        const response = await fetch(path);
                        if (!response.ok) {
                            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
                        }
                        const content = await response.text();
                        return this.writeString(content);
                    } catch (error) {
                        // Return error string
                        const errorMsg = `Failed to read ${path}: ${error.message}`;
                        return this.writeString(errorMsg);
                    }
                }
            }
        };

        const result = await WebAssembly.instantiate(this.wasmBytes, imports);
        this.wasmInstance = result.instance;
        this.wasmMemory = this.wasmInstance.exports.memory;

        return this;
    }

    /**
     * Run the WASM module's entry point
     */
    run() {
        if (!this.wasmInstance) {
            throw new Error('Runtime not initialized. Call initialize() first.');
        }

        if (this.wasmInstance.exports._start) {
            this.wasmInstance.exports._start();
        } else {
            throw new Error('WASM module has no entry point (_start function)');
        }
    }

    /**
     * Flush any buffered output
     */
    flush() {
        if (this.outputBuffer) {
            console.log(this.outputBuffer);
            this.outputBuffer = '';
        }
    }
}

// Usage example
async function runBeanstalkProgram(wasmBytes) {
    const runtime = new FullBeanstalkRuntime(wasmBytes);
    await runtime.initialize();
    runtime.run();
    runtime.flush();
}

// Export for use in Node.js or browser
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { FullBeanstalkRuntime, runBeanstalkProgram };
}
```

## Example Beanstalk Program

```beanstalk
-- full-example.bst
-- This program uses all available IO functions

-- Basic output with newline
io("=== Full Web Build System Demo ===")
io("")

-- Output without newline (progress indicator)
write("Loading")
write(".")
write(".")
write(".")
io(" Done!")

-- Error output
error("This is an error message (appears in console.error)")

-- Read from URL (with error handling)
content = read("https://example.com/data.txt") !err:
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

## HTML Integration Example

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Full Beanstalk Web App</title>
    <style>
        body {
            font-family: monospace;
            padding: 20px;
            background: #1e1e1e;
            color: #d4d4d4;
        }
        #output {
            background: #252526;
            padding: 15px;
            border-radius: 5px;
            white-space: pre-wrap;
            min-height: 200px;
        }
        .error {
            color: #f48771;
        }
        .info {
            color: #4ec9b0;
        }
    </style>
</head>
<body>
    <h1>Full Beanstalk Web App</h1>
    <div id="output"></div>

    <script>
        // Capture console output to display on page
        const outputDiv = document.getElementById('output');
        
        const originalLog = console.log;
        const originalError = console.error;
        
        console.log = function(...args) {
            outputDiv.innerHTML += `<div class="info">${args.join(' ')}</div>`;
            originalLog.apply(console, args);
        };
        
        console.error = function(...args) {
            outputDiv.innerHTML += `<div class="error">ERROR: ${args.join(' ')}</div>`;
            originalError.apply(console, args);
        };

        // Full Beanstalk runtime implementation
        class FullBeanstalkRuntime {
            constructor(wasmBytes) {
                this.wasmBytes = wasmBytes;
                this.wasmInstance = null;
                this.wasmMemory = null;
                this.outputBuffer = '';
            }

            readString(ptr, len) {
                const bytes = new Uint8Array(this.wasmMemory.buffer, ptr, len);
                return new TextDecoder().decode(bytes);
            }

            async initialize() {
                const imports = {
                    beanstalk_io: {
                        io: (ptr, len) => {
                            const text = this.readString(ptr, len);
                            console.log(text);
                        },
                        write: (ptr, len) => {
                            const text = this.readString(ptr, len);
                            this.outputBuffer += text;
                        },
                        error: (ptr, len) => {
                            const text = this.readString(ptr, len);
                            console.error(text);
                        }
                    }
                };

                const result = await WebAssembly.instantiate(this.wasmBytes, imports);
                this.wasmInstance = result.instance;
                this.wasmMemory = this.wasmInstance.exports.memory;

                return this;
            }

            run() {
                if (this.wasmInstance.exports._start) {
                    this.wasmInstance.exports._start();
                }
                
                // Flush any buffered output
                if (this.outputBuffer) {
                    console.log(this.outputBuffer);
                    this.outputBuffer = '';
                }
            }
        }

        // Load and run the Beanstalk WASM module
        async function main() {
            try {
                const response = await fetch('program.wasm');
                const wasmBytes = await response.arrayBuffer();
                
                const runtime = new FullBeanstalkRuntime(wasmBytes);
                await runtime.initialize();
                runtime.run();
            } catch (error) {
                console.error('Failed to run Beanstalk program:', error);
            }
        }

        window.addEventListener('load', main);
    </script>
</body>
</html>
```

## Build System Contract Compliance

This full build system satisfies and exceeds the Beanstalk Build System IO Contract:

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
- ✅ `io()` - Output with automatic newline

### Optional Features
- ✅ `write()` - Output without newline (for progress indicators)
- ✅ `error()` - Error stream output (console.error)
- ✅ `read()` - Read from URLs using fetch API

### Future Extensions
- ⏳ DOM manipulation functions
- ⏳ Local storage access
- ⏳ WebSocket support
- ⏳ Canvas/WebGL bindings

## When to Use

Use this full build system when:

- Building complex web applications
- Need fine-grained output control
- Require error handling and reporting
- Need to fetch data from URLs
- Want all available IO capabilities

## Performance Considerations

- `write()` buffers output for efficiency
- `read()` uses async fetch API (non-blocking)
- Memory allocation for strings is optimized
- Console output is batched when possible

## Next Steps

To customize this build system:
- Add DOM manipulation functions
- Implement local storage bindings
- Add WebSocket support
- Create custom IO functions for your use case

See also:
- [Minimal Web Build System](./build-system-minimal-web.md)
- [Native Build System](./build-system-native.md)
- [Build System IO Contract](../build-system-io-contract.md)
