# Minimal Web Build System Example

This example shows the absolute minimum implementation of a Beanstalk build system for web targets. It provides only the mandatory `io()` function.

## Build System Configuration

```rust
// src/build_system/minimal_web.rs

use crate::compiler::host_functions::registry::{
    HostFunctionRegistry, RuntimeBackend, HostFunctionDef, JsFunctionDef, BasicParameter
};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::string_interning::StringTable;
use wasm_encoder::ValType;

/// Create a minimal web build system with only the io() function
pub fn create_minimal_web_build_system(
    string_table: &mut StringTable,
) -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new_with_backend(RuntimeBackend::JavaScript);

    // Register the mandatory io() function
    let io_function = HostFunctionDef::new(
        "io",
        vec![BasicParameter {
            name: string_table.intern("content"),
            data_type: DataType::CoerceToString,
            ownership: Ownership::ImmutableReference,
        }],
        vec![], // Void return
        "beanstalk_io",
        "io",
        "Output content to console with automatic newline",
        string_table,
    );

    // Register JavaScript binding for io()
    let io_js_binding = JsFunctionDef::new(
        "beanstalk_io",
        "io",
        vec![ValType::I32, ValType::I32], // ptr, len
        vec![],
        "Output to console.log with newline",
    );

    registry.register_function_with_mappings(io_function, Some(io_js_binding), string_table)?;

    // Validate that io() is available (build system contract requirement)
    registry.validate_io_availability(string_table)?;

    Ok(registry)
}
```

## JavaScript Runtime Bindings

```javascript
// minimal-web-runtime.js

/**
 * Minimal web runtime for Beanstalk WASM modules
 * Provides only the mandatory io() function
 */
class MinimalBeanstalkRuntime {
    constructor(wasmBytes) {
        this.wasmBytes = wasmBytes;
        this.wasmInstance = null;
        this.wasmMemory = null;
    }

    /**
     * Initialize the WASM module with minimal IO bindings
     */
    async initialize() {
        const imports = {
            beanstalk_io: {
                // Mandatory io() function - outputs to console with newline
                io: (ptr, len) => {
                    const bytes = new Uint8Array(this.wasmMemory.buffer, ptr, len);
                    const text = new TextDecoder().decode(bytes);
                    console.log(text); // console.log automatically adds newline
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

        // Call the WASM module's start function (entry point)
        if (this.wasmInstance.exports._start) {
            this.wasmInstance.exports._start();
        } else {
            throw new Error('WASM module has no entry point (_start function)');
        }
    }
}

// Usage example
async function runBeanstalkProgram(wasmBytes) {
    const runtime = new MinimalBeanstalkRuntime(wasmBytes);
    await runtime.initialize();
    runtime.run();
}

// Export for use in Node.js or browser
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { MinimalBeanstalkRuntime, runBeanstalkProgram };
}
```

## HTML Integration Example

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Minimal Beanstalk Web App</title>
</head>
<body>
    <h1>Minimal Beanstalk Web App</h1>
    <p>Check the browser console for output.</p>

    <script>
        // Minimal Beanstalk runtime
        class MinimalBeanstalkRuntime {
            constructor(wasmBytes) {
                this.wasmBytes = wasmBytes;
                this.wasmInstance = null;
                this.wasmMemory = null;
            }

            async initialize() {
                const imports = {
                    beanstalk_io: {
                        io: (ptr, len) => {
                            const bytes = new Uint8Array(this.wasmMemory.buffer, ptr, len);
                            const text = new TextDecoder().decode(bytes);
                            console.log(text);
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
            }
        }

        // Load and run the Beanstalk WASM module
        async function main() {
            try {
                const response = await fetch('program.wasm');
                const wasmBytes = await response.arrayBuffer();
                
                const runtime = new MinimalBeanstalkRuntime(wasmBytes);
                await runtime.initialize();
                runtime.run();
            } catch (error) {
                console.error('Failed to run Beanstalk program:', error);
            }
        }

        // Run when page loads
        window.addEventListener('load', main);
    </script>
</body>
</html>
```

## Example Beanstalk Program

```beanstalk
-- minimal-example.bst
-- This program uses only the mandatory io() function

io("Hello from minimal web build system!")
io(42)
io([: The answer is 42])

count = 5
io([: Count: count])
```

## Compilation

```bash
# Compile with minimal web build system
beanstalk build minimal-example.bst --target web --output program.wasm
```

## Testing

```bash
# Test in Node.js
node -e "
const fs = require('fs');
const { runBeanstalkProgram } = require('./minimal-web-runtime.js');

const wasmBytes = fs.readFileSync('program.wasm');
runBeanstalkProgram(wasmBytes);
"

# Expected output:
# Hello from minimal web build system!
# 42
# The answer is 42
# Count: 5
```

## Build System Contract Compliance

This minimal build system satisfies the Beanstalk Build System IO Contract:

✅ Provides `Io` struct (conceptually)
✅ Implements mandatory `io()` function
✅ `io()` accepts `CoerceToString` parameter
✅ `io()` appends newline to output
✅ Compiler validation passes

## Limitations

This minimal build system does NOT provide:

❌ `write()` - output without newline
❌ `error()` - error stream output
❌ `read()` - file/resource reading
❌ DOM manipulation functions
❌ Environment variable access

For these features, use a full-featured web build system.

## When to Use

Use this minimal build system when:

- Building simple web applications
- Learning Beanstalk basics
- Prototyping without complex IO requirements
- Targeting environments with limited capabilities
- Minimizing JavaScript runtime size

## Next Steps

To add optional IO functions, see:
- [Full Web Build System Example](./build-system-full-web.md)
- [Build System IO Contract](../build-system-io-contract.md)
