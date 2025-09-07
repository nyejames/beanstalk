// Direct JIT execution for quick development feedback
//
// This module provides immediate WASM execution using Wasmer's JIT capabilities
// for rapid development iteration without additional compilation steps.

use crate::compiler::compiler_errors::CompileError;
use crate::runtime::{IoBackend, RuntimeConfig};
use wasmer::{Function, Instance, Module, Store, imports};

/// Execute WASM bytecode directly using JIT compilation
pub fn execute_direct_jit(wasm_bytes: &[u8], config: &RuntimeConfig) -> Result<(), CompileError> {
    // Create Wasmer store for JIT execution
    let mut store = Store::default();

    // Compile WASM module
    let module = Module::new(&store, wasm_bytes).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to compile WASM module: {}", e))
    })?;

    // Set up imports based on IO backend
    let import_object = create_import_object(&mut store, &config.io_backend)?;

    // Instantiate the module
    let instance = Instance::new(&mut store, &module, &import_object).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to instantiate WASM module: {}", e))
    })?;

    // Wasmer automatically runs the start section (if present) when instantiating the module above.
    // So if instantiation succeeded, the start section has already run.
    // Now, optionally call 'main' or '_start' if present (for non-WASI or legacy modules).
    if let Ok(main_func) = instance.exports.get_function("main") {
        match main_func.call(&mut store, &[]) {
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

/// Create import object based on the configured IO backend
pub fn create_import_object(
    store: &mut Store,
    io_backend: &IoBackend,
) -> Result<wasmer::Imports, CompileError> {
    let mut imports = imports! {};

    match io_backend {
        IoBackend::Wasi => {
            // Set up WASI imports
            setup_wasi_imports(store, &mut imports)?;
        }
        IoBackend::Custom(_config_path) => {
            // Set up custom IO hooks
            setup_custom_io_imports(store, &mut imports)?;
        }
        IoBackend::JsBindings => {
            // Set up JS/DOM bindings (for web targets)
            setup_js_imports(store, &mut imports)?;
        }
        IoBackend::Native => {
            // Set up native system call imports
            setup_native_imports(store, &mut imports)?;
        }
    }

    Ok(imports)
}

/// Set up WASI imports for standard I/O operations
fn setup_wasi_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    setup_beanstalk_io_imports(store, imports)?;
    setup_beanstalk_env_imports(store, imports)?;
    setup_beanstalk_sys_imports(store, imports)?;
    Ok(())
}

/// Set up Beanstalk IO imports
fn setup_beanstalk_io_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Print function: (ptr: i32, len: i32) -> ()
    let print_func = Function::new_typed(store, |msg_ptr: i32, msg_len: i32| {
        // TODO: Read from WASM memory when memory access is available
        println!("Beanstalk Print: ptr={}, len={}", msg_ptr, msg_len);
    });
    imports.define("beanstalk_io", "print", print_func);

    // Read input function: (buffer_ptr: i32) -> i32
    let read_input_func = Function::new_typed(store, |buffer_ptr: i32| -> i32 {
        // TODO: Implement actual input reading
        println!("Beanstalk ReadInput: buffer_ptr={}", buffer_ptr);
        0 // Return 0 bytes read for now
    });
    imports.define("beanstalk_io", "read_input", read_input_func);

    // Write file function: (path_ptr: i32, path_len: i32, content_ptr: i32, content_len: i32) -> i32
    let write_file_func = Function::new_typed(
        store,
        |path_ptr: i32, path_len: i32, content_ptr: i32, content_len: i32| -> i32 {
            // TODO: Implement actual file writing
            println!(
                "Beanstalk WriteFile: path_ptr={}, path_len={}, content_ptr={}, content_len={}",
                path_ptr, path_len, content_ptr, content_len
            );
            0 // Return success for now
        },
    );
    imports.define("beanstalk_io", "write_file", write_file_func);

    // Read file function: (path_ptr: i32, path_len: i32, buffer_ptr: i32) -> i32
    let read_file_func = Function::new_typed(
        store,
        |path_ptr: i32, path_len: i32, buffer_ptr: i32| -> i32 {
            // TODO: Implement actual file reading
            println!(
                "Beanstalk ReadFile: path_ptr={}, path_len={}, buffer_ptr={}",
                path_ptr, path_len, buffer_ptr
            );
            0 // Return 0 bytes read for now
        },
    );
    imports.define("beanstalk_io", "read_file", read_file_func);

    Ok(())
}

/// Set up Beanstalk environment imports
fn setup_beanstalk_env_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Get environment variable: (key_ptr: i32, key_len: i32, buffer_ptr: i32) -> i32
    let get_env_func = Function::new_typed(
        store,
        |key_ptr: i32, key_len: i32, buffer_ptr: i32| -> i32 {
            // TODO: Implement actual environment variable access
            println!(
                "Beanstalk GetEnv: key_ptr={}, key_len={}, buffer_ptr={}",
                key_ptr, key_len, buffer_ptr
            );
            -1 // Return -1 (not found) for now
        },
    );
    imports.define("beanstalk_env", "get_env", get_env_func);

    // Set environment variable: (key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32) -> i32
    let set_env_func = Function::new_typed(
        store,
        |key_ptr: i32, key_len: i32, value_ptr: i32, value_len: i32| -> i32 {
            // TODO: Implement actual environment variable setting
            println!(
                "Beanstalk SetEnv: key_ptr={}, key_len={}, value_ptr={}, value_len={}",
                key_ptr, key_len, value_ptr, value_len
            );
            0 // Return success for now
        },
    );
    imports.define("beanstalk_env", "set_env", set_env_func);

    Ok(())
}

/// Set up Beanstalk system imports
fn setup_beanstalk_sys_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Exit function: (exit_code: i32) -> ()
    // Note: We can't use std::process::exit directly in Wasmer functions due to the ! return type
    let exit_func = Function::new_typed(store, |exit_code: i32| {
        println!("Beanstalk Exit: exit_code={}", exit_code);
        // For now, just print the exit code. In a real implementation,
        // we'd need to handle this differently (e.g., return an error to the runtime)
    });
    imports.define("beanstalk_sys", "exit", exit_func);

    Ok(())
}

/// Set up custom IO imports for embedded scenarios
fn setup_custom_io_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Use the same Beanstalk imports but with custom implementations
    setup_beanstalk_io_imports(store, imports)?;
    setup_beanstalk_env_imports(store, imports)?;
    setup_beanstalk_sys_imports(store, imports)?;
    Ok(())
}

/// Set up JavaScript/DOM bindings for web targets
fn setup_js_imports(store: &mut Store, imports: &mut wasmer::Imports) -> Result<(), CompileError> {
    // Use Beanstalk imports but map to JS equivalents
    setup_beanstalk_io_imports(store, imports)?;
    setup_beanstalk_env_imports(store, imports)?;

    // Don't include sys imports for web (no exit)
    Ok(())
}

/// Set up native system call imports
fn setup_native_imports(
    store: &mut Store,
    imports: &mut wasmer::Imports,
) -> Result<(), CompileError> {
    // Use full Beanstalk imports for native execution
    setup_beanstalk_io_imports(store, imports)?;
    setup_beanstalk_env_imports(store, imports)?;
    setup_beanstalk_sys_imports(store, imports)?;
    Ok(())
}
