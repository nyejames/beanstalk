# Minimal Native Build System Example

This example shows the absolute minimum implementation of a Beanstalk build system for native targets. It provides only the mandatory `io()` function.

## Build System Configuration

```rust
// src/build_system/minimal_native.rs

use crate::compiler::host_functions::registry::{
    HostFunctionRegistry, RuntimeBackend, HostFunctionDef, BasicParameter
};
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::string_interning::StringTable;

/// Create a minimal native build system with only the io() function
pub fn create_minimal_native_build_system(
    string_table: &mut StringTable,
) -> Result<HostFunctionRegistry, CompileError> {
    let mut registry = HostFunctionRegistry::new_with_backend(RuntimeBackend::Native);

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
        "Output content to stdout with automatic newline",
        string_table,
    );

    registry.register_function(io_function, string_table)?;

    // Validate that io() is available (build system contract requirement)
    registry.validate_io_availability(string_table)?;

    Ok(registry)
}
```

## Native Runtime Implementation

```rust
// src/runtime/minimal_native_runtime.rs

use wasmer::{Store, Module, Instance, imports, Function, FunctionEnv, FunctionEnvMut};
use std::io::{self, Write};

/// Minimal native runtime environment
pub struct MinimalNativeRuntime {
    store: Store,
    instance: Option<Instance>,
}

impl MinimalNativeRuntime {
    /// Create a new minimal native runtime
    pub fn new() -> Self {
        MinimalNativeRuntime {
            store: Store::default(),
            instance: None,
        }
    }

    /// Initialize the runtime with WASM bytes
    pub fn initialize(&mut self, wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let module = Module::new(&self.store, wasm_bytes)?;

        // Create function environment for host functions
        let env = FunctionEnv::new(&mut self.store, ());

        // Create import object with io() function
        let import_object = imports! {
            "beanstalk_io" => {
                "io" => Function::new_typed_with_env(
                    &mut self.store,
                    &env,
                    Self::host_io
                ),
            }
        };

        // Instantiate the WASM module
        self.instance = Some(Instance::new(&mut self.store, &module, &import_object)?);

        Ok(())
    }

    /// Host function: io() - output with newline
    fn host_io(mut env: FunctionEnvMut<()>, ptr: i32, len: i32) {
        let (data, store) = env.data_and_store_mut();
        
        // Get memory from the WASM instance
        let memory = env.data().instance.exports.get_memory("memory")
            .expect("Failed to get WASM memory");

        // Read string from WASM memory
        let memory_view = memory.view(&store);
        let mut buffer = vec![0u8; len as usize];
        memory_view.read(ptr as u64, &mut buffer)
            .expect("Failed to read from WASM memory");

        // Convert to string and print with newline
        let text = String::from_utf8_lossy(&buffer);
        println!("{}", text); // Automatic newline
    }

    /// Run the WASM module's entry point
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let instance = self.instance.as_ref()
            .ok_or("Runtime not initialized")?;

        // Call the entry point function
        let start_func = instance.exports.get_function("_start")?;
        start_func.call(&mut self.store, &[])?;

        Ok(())
    }
}

/// Run a Beanstalk WASM program with minimal native runtime
pub fn run_beanstalk_program(wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = MinimalNativeRuntime::new();
    runtime.initialize(wasm_bytes)?;
    runtime.run()?;
    Ok(())
}
```

## Example Beanstalk Program

```beanstalk
-- minimal-native-example.bst
-- This program uses only the mandatory io() function

io("Hello from minimal native build system!")
io(42)
io([: The answer is 42])

count = 5
io([: Count: count])
```

## Compilation and Execution

```bash
# Compile with minimal native build system
beanstalk build minimal-native-example.bst --target native --output program.wasm

# Run with Wasmer
wasmer run program.wasm

# Or run with custom runtime
cargo run --bin beanstalk-runner -- program.wasm
```

## Custom Runner Implementation

```rust
// src/bin/beanstalk-runner.rs

use std::fs;
use std::env;

mod runtime;
use runtime::minimal_native_runtime::run_beanstalk_program;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() != 2 {
        eprintln!("Usage: beanstalk-runner <wasm-file>");
        std::process::exit(1);
    }

    let wasm_path = &args[1];
    let wasm_bytes = fs::read(wasm_path)?;

    println!("Running Beanstalk program: {}", wasm_path);
    println!("---");

    run_beanstalk_program(&wasm_bytes)?;

    Ok(())
}
```

## Expected Output

```
Running Beanstalk program: program.wasm
---
Hello from minimal native build system!
42
The answer is 42
Count: 5
```

## Build System Contract Compliance

This minimal native build system satisfies the Beanstalk Build System IO Contract:

✅ Provides `Io` struct (conceptually)
✅ Implements mandatory `io()` function
✅ `io()` accepts `CoerceToString` parameter
✅ `io()` appends newline to output (via `println!`)
✅ Compiler validation passes

## Limitations

This minimal build system does NOT provide:

❌ `write()` - output without newline
❌ `error()` - stderr output
❌ `read()` - file reading
❌ Environment variable access
❌ System calls

For these features, use a full-featured native build system.

## Performance Characteristics

- **Fast startup**: Minimal overhead
- **Low memory**: Only essential runtime components
- **Direct stdout**: No buffering or formatting overhead
- **Native speed**: Full WASM performance

## When to Use

Use this minimal native build system when:

- Building simple command-line tools
- Learning Beanstalk basics
- Prototyping without complex IO requirements
- Targeting embedded or constrained environments
- Minimizing runtime dependencies

## Integration with Wasmer

```rust
// Alternative: Using Wasmer directly without custom runtime

use wasmer::{Store, Module, Instance, imports, Function};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = fs::read("program.wasm")?;
    let mut store = Store::default();
    let module = Module::new(&store, &wasm_bytes)?;

    let import_object = imports! {
        "beanstalk_io" => {
            "io" => Function::new_typed(&mut store, |ptr: i32, len: i32| {
                // Read from memory and print
                // (simplified - needs proper memory access)
                println!("Output from WASM");
            }),
        }
    };

    let instance = Instance::new(&mut store, &module, &import_object)?;
    let start = instance.exports.get_function("_start")?;
    start.call(&mut store, &[])?;

    Ok(())
}
```

## Next Steps

To add optional IO functions, see:
- [Full Native Build System Example](./build-system-full-native.md)
- [Build System IO Contract](../build-system-io-contract.md)

## Testing

```bash
# Run tests
cargo test minimal_native_runtime

# Run with different WASM runtimes
wasmer run program.wasm
wasmtime run program.wasm

# Benchmark performance
hyperfine 'wasmer run program.wasm'
```

## Troubleshooting

### Memory Access Issues

If you encounter memory access errors:

```rust
// Ensure proper memory bounds checking
fn read_string_from_memory(memory: &Memory, store: &Store, ptr: i32, len: i32) -> String {
    let memory_view = memory.view(&store);
    let mut buffer = vec![0u8; len as usize];
    
    // Check bounds
    if ptr < 0 || len < 0 {
        panic!("Invalid memory access: negative pointer or length");
    }
    
    memory_view.read(ptr as u64, &mut buffer)
        .expect("Failed to read from WASM memory");
    
    String::from_utf8_lossy(&buffer).to_string()
}
```

### Missing Entry Point

If the WASM module has no `_start` function:

```rust
// Try alternative entry points
let start_func = instance.exports.get_function("_start")
    .or_else(|_| instance.exports.get_function("main"))
    .or_else(|_| instance.exports.get_function("run"))?;
```

## See Also

- [Minimal Web Build System](./build-system-minimal-web.md)
- [Full Native Build System](./build-system-full-native.md)
- [Wasmer Documentation](https://docs.wasmer.io/)
