use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::compiler_errors::{CompileError, ErrorLocation};
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::wir::build_wir::WIR;
use crate::compiler::wir::wir_nodes::ExportKind;
use crate::{return_compiler_error, return_wasm_generation_error};
use crate::compiler::string_interning::StringTable;

/// Basic WASM validation using wasmparser
fn validate_wasm_module(wasm_bytes: &[u8]) -> Result<(), CompileError> {
    match wasmparser::validate(wasm_bytes) {
        Ok(_) => Ok(()),
        Err(e) => {
            let error_msg = e.to_string();
            let error_msg_static: &'static str = Box::leak(error_msg.into_boxed_str());
            return_compiler_error!(
                "Generated WASM module is invalid. This indicates a bug in the WASM backend" ; {
                    CompilationStage => "WASM Validation",
                    PrimarySuggestion => error_msg_static,
                }
            );
        }
    }
}

/// Simplified WIR-to-WASM compilation entry point
///
/// This function provides direct WIR → WASM lowering with minimal overhead.
/// Complex validation and performance tracking have been removed to focus
/// on core functionality until borrow checking is complete.
pub fn new_wasm_module(wir: WIR, string_table: &mut crate::compiler::string_interning::StringTable) -> Result<Vec<u8>, CompileError> {
    new_wasm_module_with_registry(wir, None, string_table)
}

/// WIR-to-WASM compilation with host function registry support
///
/// This function provides direct WIR → WASM lowering with access to the host function registry
/// for proper runtime-specific function mapping during codegen.
pub fn new_wasm_module_with_registry(
    wir: WIR, 
    registry: Option<&HostFunctionRegistry>,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<u8>, CompileError> {
    // Basic WIR validation
    validate_wir_for_wasm_compilation(&wir, string_table)?;

    // Create WASM module from WIR with registry access
    let mut module = WasmModule::from_wir_with_registry(&wir, registry, string_table)?;

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
                let export_name = string_table.resolve(export.name);
                let _ = module.add_function_export(export_name, function_index);
            }
        }
    }

    // Handle other exports
    for (export_name_id, export) in &wir.exports {
        let export_name = string_table.resolve(*export_name_id);
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
fn validate_wir_for_wasm_compilation(wir: &WIR, string_table: &StringTable) -> Result<(), CompileError> {
    // Empty WIR is allowed - it creates a minimal WASM module
    // This is useful for testing and incremental compilation

    // Validate function names are unique (if any functions exist)
    if !wir.functions.is_empty() {
        let mut function_names = std::collections::HashSet::new();
        for function in &wir.functions {
            let function_name = string_table.resolve(function.name);
            if !function_names.insert(function_name) {
                let function_name_static: &'static str = Box::leak(function_name.to_string().into_boxed_str());
                return_wasm_generation_error!(
                    format!("Duplicate function name '{}' in WIR. Function names must be unique for WASM generation.", function_name),
                    ErrorLocation::default(), {
                        CompilationStage => "WASM Generation",
                        VariableName => function_name_static,
                        PrimarySuggestion => "Rename one of the duplicate functions to have a unique name",
                    }
                );
            }
        }
    }

    // Validate memory configuration is reasonable
    let memory_info = &wir.type_info.memory_info;
    if memory_info.initial_pages > 65536 {
        return_wasm_generation_error!(
            format!("WIR specifies {} initial pages, but WASM maximum is 65536 pages (4GB).", memory_info.initial_pages),
            ErrorLocation::default(),
            {
                CompilationStage => "WASM Generation",
                PrimarySuggestion => "Reduce the initial memory pages to 65536 or less",
            }
        );
    }

    if let Some(max_pages) = memory_info.max_pages {
        if max_pages > 65536 {
            return_wasm_generation_error!(
                format!("WIR specifies {} max pages, but WASM maximum is 65536 pages (4GB).", max_pages),
                ErrorLocation::default(),
                {
                    CompilationStage => "WASM Generation",
                    PrimarySuggestion => "Reduce the maximum memory pages to 65536 or less",
                }
            );
        }

        // Check that max pages is not less than initial pages
        if max_pages < memory_info.initial_pages {
            return_wasm_generation_error!(
                format!("WIR memory max pages ({}) is less than initial pages ({}). Maximum memory pages must be greater than or equal to initial pages.", max_pages, memory_info.initial_pages),
                ErrorLocation::default(),
                {
                    CompilationStage => "WASM Generation",
                    PrimarySuggestion => "Increase max pages to be at least equal to initial pages",
                }
            );
        }
    }

    // Validate interface consistency
    if !wir.type_info.interface_info.interfaces.is_empty() {
        for (interface_id, interface_def) in &wir.type_info.interface_info.interfaces {
            if interface_def.methods.is_empty() {
                return_wasm_generation_error!(
                    format!("Interface {} has no methods. Empty interfaces cannot be used for dynamic dispatch.", interface_id),
                    ErrorLocation::default(),
                    {
                        CompilationStage => "WASM Generation",
                        PrimarySuggestion => "Add at least one method to the interface or remove it",
                    }
                );
            }
        }
    }

    Ok(())
}
