use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::wir::build_wir::WIR;
use crate::compiler::wir::wir_nodes::ExportKind;
use crate::return_compiler_error;

/// Basic WASM validation using wasmparser
fn validate_wasm_module(wasm_bytes: &[u8]) -> Result<(), CompileError> {
    match wasmparser::validate(wasm_bytes) {
        Ok(_) => Ok(()),
        Err(e) => {
            return_compiler_error!(
                "Generated WASM module is invalid: {}. This indicates a bug in the WASM backend.",
                e
            );
        }
    }
}

/// Simplified WIR-to-WASM compilation entry point
///
/// This function provides direct WIR → WASM lowering with minimal overhead.
/// Complex validation and performance tracking have been removed to focus
/// on core functionality until borrow checking is complete.
pub fn new_wasm_module(wir: WIR) -> Result<Vec<u8>, CompileError> {
    // Basic WIR validation
    validate_wir_for_wasm_compilation(&wir)?;

    // Create WASM module from WIR (this already compiles all functions)
    let mut module = WasmModule::from_wir(&wir)?;

    // Handle exports (functions are already compiled in from_wir)
    for wir_function in &wir.functions {
        // Export the function if it's marked for export
        if let Some(export) = wir.exports.get(&wir_function.name) {
            if export.kind == ExportKind::Function {
                // Function index is the same as the order in wir.functions since from_wir processes them in order
                let function_index = wir
                    .functions
                    .iter()
                    .position(|f| f.name == wir_function.name)
                    .unwrap() as u32;
                let _ = module.add_function_export(&export.name, function_index);
            }
        }
    }

    // Handle other exports
    for (export_name, export) in &wir.exports {
        match export.kind {
            ExportKind::Global => {
                let _ = module.add_global_export(export_name, export.index);
            }
            ExportKind::Memory => {
                module.add_memory_export(export_name)?;
            }
            ExportKind::Table => {
                // Table exports handled by interface support
            }
            _ => {} // Function exports handled above
        }
    }

    // Generate final WASM bytecode
    let compiled_wasm = module.finish();

    // Basic WASM validation
    validate_wasm_module(&compiled_wasm)?;

    Ok(compiled_wasm)
}

/// Validate WIR structure before WASM compilation
fn validate_wir_for_wasm_compilation(wir: &WIR) -> Result<(), CompileError> {
    // Empty WIR is allowed - it creates a minimal WASM module
    // This is useful for testing and incremental compilation

    // Validate function names are unique (if any functions exist)
    if !wir.functions.is_empty() {
        let mut function_names = std::collections::HashSet::new();
        for function in &wir.functions {
            if !function_names.insert(&function.name) {
                return_compiler_error!(
                    "Duplicate function name '{}' in WIR. Function names must be unique for WASM generation.",
                    function.name
                );
            }
        }
    }

    // Validate memory configuration is reasonable
    let memory_info = &wir.type_info.memory_info;
    if memory_info.initial_pages > 65536 {
        return_compiler_error!(
            "WIR specifies {} initial pages, but WASM maximum is 65536 pages (4GB).",
            memory_info.initial_pages
        );
    }

    if let Some(max_pages) = memory_info.max_pages {
        if max_pages > 65536 {
            return_compiler_error!(
                "WIR specifies {} max pages, but WASM maximum is 65536 pages (4GB).",
                max_pages
            );
        }

        // Check that max pages is not less than initial pages
        if max_pages < memory_info.initial_pages {
            return_compiler_error!(
                "WIR memory max pages ({}) is less than initial pages ({}). \
                Maximum memory pages must be greater than or equal to initial pages.",
                max_pages,
                memory_info.initial_pages
            );
        }
    }

    // Validate interface consistency
    if !wir.type_info.interface_info.interfaces.is_empty() {
        for (interface_id, interface_def) in &wir.type_info.interface_info.interfaces {
            if interface_def.methods.is_empty() {
                return_compiler_error!(
                    "Interface {} has no methods. Empty interfaces cannot be used for dynamic dispatch.",
                    interface_id
                );
            }
        }
    }

    Ok(())
}
