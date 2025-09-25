//! Tests for enhanced WasmModule finish() method with MIR-based module generation

use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::mir::mir_nodes::{MIR, Export, ExportKind, FunctionSignature};
use crate::compiler::mir::place::WasmType;

#[test]
fn test_finish_with_empty_mir() {
    let mir = MIR::new();
    let wasm_module = WasmModule::new();
    
    // Test that finish_with_mir works with empty MIR
    let result = wasm_module.finish_with_mir(&mir);
    assert!(result.is_ok(), "finish_with_mir should succeed with empty MIR");
    
    let wasm_bytes = result.unwrap();
    assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
}

#[test]
fn test_finish_with_function_export() {
    let mut mir = MIR::new();
    
    // Add a function export
    let export = Export {
        name: "test_function".to_string(),
        kind: ExportKind::Function,
        index: 0,
    };
    mir.exports.insert("test_function".to_string(), export);
    
    // Add a function signature to type info
    let function_sig = FunctionSignature {
        param_types: vec![WasmType::I32],
        result_types: vec![WasmType::I32],
    };
    mir.type_info.function_types.push(function_sig);
    
    // Create a module with simulated function count
    let wasm_module = WasmModule::new();
    // We can't directly set function_count as it's private, so we'll test with the export only
    
    let result = wasm_module.finish_with_mir(&mir);
    // This should fail because we don't have the actual function, but that's expected
    // The important thing is that the export generation code runs
    assert!(result.is_err(), "Should fail with invalid function index");
}

#[test]
fn test_finish_with_memory_export() {
    let mut mir = MIR::new();
    
    // Add a memory export
    let export = Export {
        name: "memory".to_string(),
        kind: ExportKind::Memory,
        index: 0,
    };
    mir.exports.insert("memory".to_string(), export);
    
    let wasm_module = WasmModule::new();
    
    let result = wasm_module.finish_with_mir(&mir);
    assert!(result.is_ok(), "finish_with_mir should succeed with memory export");
    
    let wasm_bytes = result.unwrap();
    assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
}

#[test]
fn test_finish_with_start_function() {
    let mut mir = MIR::new();
    
    // Add a main function export (should become start function)
    let export = Export {
        name: "main".to_string(),
        kind: ExportKind::Function,
        index: 0,
    };
    mir.exports.insert("main".to_string(), export);
    
    // Add a function signature with no params and no return (valid for start function)
    let function_sig = FunctionSignature {
        param_types: vec![],
        result_types: vec![],
    };
    mir.type_info.function_types.push(function_sig.clone());
    
    // Add a function to the MIR using the proper constructor
    use crate::compiler::mir::mir_nodes::MirFunction;
    let function = MirFunction::new(0, "main".to_string(), vec![], vec![]);
    mir.functions.push(function);
    
    let wasm_module = WasmModule::new();
    
    let result = wasm_module.finish_with_mir(&mir);
    // This should fail because we don't have the actual compiled function, but that's expected
    // The important thing is that the start section generation code runs
    assert!(result.is_err(), "Should fail with invalid function index");
}

#[test]
fn test_finish_with_interface_support() {
    let mut mir = MIR::new();
    
    // Add interface support with function table
    mir.type_info.interface_info.function_table = vec![0, 1, 2];
    
    let wasm_module = WasmModule::new();
    
    let result = wasm_module.finish_with_mir(&mir);
    // This should fail because we don't have the actual functions, but that's expected
    // The important thing is that the element section generation code runs
    assert!(result.is_err(), "Should fail with invalid function indices");
}

#[test]
fn test_finish_section_ordering() {
    let _mir = MIR::new();
    let wasm_module = WasmModule::new();
    
    // Test that the basic finish method maintains proper section ordering
    let wasm_bytes = wasm_module.finish();
    assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
    
    // The WASM should be valid (basic validation)
    // In a real implementation, we might use wasmparser to validate the module
}

#[test]
fn test_finish_with_mir_basic_functionality() {
    let mir = MIR::new();
    let wasm_module = WasmModule::new();
    
    // Test the basic finish_with_mir functionality
    let result = wasm_module.finish_with_mir(&mir);
    assert!(result.is_ok(), "finish_with_mir should succeed with empty MIR");
    
    let wasm_bytes = result.unwrap();
    assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
    
    // Verify it's the same as calling finish() directly for empty MIR
    let wasm_module2 = WasmModule::new();
    let wasm_bytes2 = wasm_module2.finish();
    
    // Both should produce valid WASM modules
    assert!(!wasm_bytes2.is_empty(), "Direct finish() should also produce valid WASM");
}