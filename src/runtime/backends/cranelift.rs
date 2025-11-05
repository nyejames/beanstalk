// Cranelift backend for fast development compilation
//
// Uses Wasmer's Cranelift backend for rapid compilation during development.
// Prioritizes compilation speed over runtime performance.

use crate::compiler::compiler_errors::CompileError;
use crate::runtime::{CraneliftOptLevel, RuntimeConfig};
use wasmer::sys::{Cranelift, CraneliftOptLevel as WasmerCraneliftOptLevel};
use wasmer::{Instance, Module, Store};

/// Execute WASM using Cranelift backend with a specified optimisation level
pub fn execute_with_cranelift(
    wasm_bytes: &[u8],
    config: &RuntimeConfig,
    opt_level: &CraneliftOptLevel,
) -> Result<(), CompileError> {
    // Create a store with Cranelift compiler configured for the specified optimisation level
    let wasmer_opt_level = match opt_level {
        CraneliftOptLevel::None => WasmerCraneliftOptLevel::None,
        CraneliftOptLevel::Speed => WasmerCraneliftOptLevel::Speed,
        CraneliftOptLevel::SpeedAndSize => WasmerCraneliftOptLevel::SpeedAndSize,
    };

    let _cranelift = Cranelift::new().opt_level(wasmer_opt_level);

    let mut store = Store::default();

    // Log optimization level for debugging
    if config
        .flags
        .iter()
        .any(|f| matches!(f, crate::runtime::RuntimeFlag::Debug))
    {
        println!("Cranelift optimization level: {:?}", opt_level);
    }

    // Compile module with Cranelift
    let module = Module::new(&store, wasm_bytes).map_err(|e| {
        CompileError::compiler_error(&format!("Cranelift compilation failed: {}", e))
    })?;

    // Set up imports based on IO backend
    let (import_object, _wasi_env_guard) = crate::runtime::jit::create_import_object(
        &mut store,
        &module,
        wasm_bytes,
        &config.io_backend,
    )?;

    // Instantiate and execute
    let instance = Instance::new(&mut store, &module, &import_object).map_err(|e| {
        CompileError::compiler_error(&format!("Failed to instantiate with Cranelift: {}", e))
    })?;

    execute_instance(&mut store, &instance)
}

/// Execute the instantiated WASM module
fn execute_instance(store: &mut Store, instance: &Instance) -> Result<(), CompileError> {
    // Look for entry points
    if let Ok(main_func) = instance.exports.get_function("main") {
        let result = main_func.call(store, &[]);

        match result {
            Ok(values) => {
                if !values.is_empty() {
                    println!("Cranelift execution result: {:?}", values);
                }
                Ok(())
            }
            Err(e) => Err(CompileError::compiler_error(&format!(
                "Cranelift runtime error: {}",
                e
            ))),
        }
    } else if let Ok(start_func) = instance.exports.get_function("_start") {
        let result = start_func.call(store, &[]);

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(CompileError::compiler_error(&format!(
                "Cranelift runtime error: {}",
                e
            ))),
        }
    } else {
        Err(CompileError::compiler_error(
            "No entry point found in WASM module",
        ))
    }
}
