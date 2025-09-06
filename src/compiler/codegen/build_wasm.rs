use crate::compiler::mir::build_mir::MIR;
use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::compiler_errors::CompileError;

use crate::return_compiler_error;

// Assumes that the MIR being passed in is an entire complete module.
// All dependencies should be declared inside this module.
pub fn new_wasm_module(mir: MIR) -> Result<Vec<u8>, CompileError> {
    // Create WasmModule from MIR with proper initialization
    let mut module = WasmModule::from_mir(&mir)?;

    // Process all functions in the MIR
    for mir_function in &mir.functions {
        let function_index = module.compile_mir_function(mir_function)?;
        
        // Export the function if it's marked for export
        if let Some(export) = mir.exports.get(&mir_function.name) {
            match export.kind {
                crate::compiler::mir::mir_nodes::ExportKind::Function => {
                    module.add_function_export(&export.name, function_index);
                }
                _ => {} // Other export kinds handled elsewhere
            }
        }
    }

    // Add exports for globals
    for (export_name, export) in &mir.exports {
        match export.kind {
            crate::compiler::mir::mir_nodes::ExportKind::Global => {
                module.add_global_export(export_name, export.index);
            }
            crate::compiler::mir::mir_nodes::ExportKind::Memory => {
                module.add_memory_export(export_name, export.index);
            }
            _ => {} // Function exports handled above
        }
    }

    // Build the final wasm module and validate it
    let compiled_wasm = module.finish();
    match wasmparser::validate(&compiled_wasm) {
        Ok(_) => Ok(compiled_wasm),
        Err(e) => return_compiler_error!(
            "Failed to validate final wasm output: {e}"
        ),
    }
}