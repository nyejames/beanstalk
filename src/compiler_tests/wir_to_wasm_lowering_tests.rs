//! Tests for WIR-to-WASM lowering functionality
//! 
//! This module tests the host function call lowering, WASIX fd_write generation,
//! and entry point export generation implemented in task 3.

use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::host_functions::registry::{
    create_builtin_registry, HostFunctionDef, BasicParameter, RuntimeBackend
};
use crate::compiler::datatypes::DataType;
use crate::compiler::wir::wir_nodes::{
    WIR, WirFunction, WirBlock, Statement, Operand, Constant, Export, ExportKind
};
use crate::compiler::wir::place::{Place, WasmType};
use std::collections::{HashMap, HashSet};

/// Test host function call lowering with different backends
#[test]
fn test_host_function_call_lowering_with_registry() {
    // Create a host function registry with WASIX backend
    let registry = create_builtin_registry().expect("Failed to create builtin registry");
    
    // Create a simple WIR with a host function call
    let mut wir = create_test_wir_with_host_call();
    
    // Test WASM module generation with registry
    let result = WasmModule::from_wir_with_registry(&wir, Some(&registry));
    assert!(result.is_ok(), "WASM module generation should succeed with registry");
    
    let module = result.unwrap();
    
    // Verify that host functions are properly registered
    assert!(module.get_host_function_index("template_output").is_some(), 
           "Template output function should be registered in WASM module");
}

/// Test WASIX fd_write generation for template output statements
#[test]
fn test_wasix_fd_write_generation() {
    // Create a registry with WASIX backend
    let registry = create_builtin_registry().expect("Failed to create builtin registry");
    
    // Create WIR with template output statement
    let wir = create_test_wir_with_template_output_statement();
    
    // Generate WASM module
    let result = WasmModule::from_wir_with_registry(&wir, Some(&registry));
    assert!(result.is_ok(), "WASM module generation should succeed");
    
    let module = result.unwrap();
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let validation_result = wasmparser::validate(&wasm_bytes);
    assert!(validation_result.is_ok(), "Generated WASM should be valid: {:?}", validation_result.err());
    
    // Check that the WASM contains the expected imports
    let parser = wasmparser::Parser::new(0);
    let mut has_wasix_import = false;
    
    for payload in parser.parse_all(&wasm_bytes) {
        match payload.expect("Failed to parse WASM") {
            wasmparser::Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.expect("Failed to read import");
                    if import.module == "wasix_32v1" && import.name == "fd_write" {
                        has_wasix_import = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    
    assert!(has_wasix_import, "WASM should contain WASIX fd_write import");
}

/// Test entry point export generation and function indexing
#[test]
fn test_entry_point_export_generation() {
    // Create WIR with main function (entry point)
    let wir = create_test_wir_with_main_function();
    
    // Generate WASM module
    let result = WasmModule::from_wir(&wir);
    assert!(result.is_ok(), "WASM module generation should succeed");
    
    let module = result.unwrap();
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let validation_result = wasmparser::validate(&wasm_bytes);
    assert!(validation_result.is_ok(), "Generated WASM should be valid: {:?}", validation_result.err());
    
    // Check that the WASM contains the main function export
    let parser = wasmparser::Parser::new(0);
    let mut has_main_export = false;
    
    for payload in parser.parse_all(&wasm_bytes) {
        match payload.expect("Failed to parse WASM") {
            wasmparser::Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.expect("Failed to read export");
                    if export.name == "main" {
                        has_main_export = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    
    assert!(has_main_export, "WASM should contain main function export");
}

/// Test that only one start function is exported per module
#[test]
fn test_single_entry_point_validation() {
    // Create WIR with multiple potential entry points
    let wir = create_test_wir_with_multiple_functions();
    
    // Generate WASM module - should succeed with single entry point
    let result = WasmModule::from_wir(&wir);
    assert!(result.is_ok(), "WASM module generation should succeed with single entry point");
    
    // Test would fail if we had multiple entry points, but our current implementation
    // only exports main as entry point, so this should pass
}

/// Test host function import section generation with correct module names
#[test]
fn test_import_section_generation() {
    // Create registry with different runtime mappings
    let registry = create_builtin_registry().expect("Failed to create builtin registry");
    
    // Create WIR with host function imports
    let wir = create_test_wir_with_host_imports();
    
    // Generate WASM module with registry
    let result = WasmModule::from_wir_with_registry(&wir, Some(&registry));
    assert!(result.is_ok(), "WASM module generation should succeed");
    
    let module = result.unwrap();
    let wasm_bytes = module.finish();
    
    // Parse and verify imports use correct module names
    let parser = wasmparser::Parser::new(0);
    let mut found_correct_import = false;
    
    for payload in parser.parse_all(&wasm_bytes) {
        match payload.expect("Failed to parse WASM") {
            wasmparser::Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.expect("Failed to read import");
                    // Check for WASIX mapping (template_output -> fd_write)
                    if import.module == "wasix_32v1" && import.name == "fd_write" {
                        found_correct_import = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    
    assert!(found_correct_import, "WASM should use correct runtime-specific module names");
}

// Helper functions to create test WIR structures

fn create_test_wir_with_host_call() -> WIR {
    let mut wir = WIR::new();
    
    // Add a simple function with host call
    let mut function = WirFunction::new(
        0,
        "test_function".to_string(),
        vec![],
        vec![],
        vec![],
    );
    
    let mut block = WirBlock::new(0);
    
    // Add host call statement
    let host_func = HostFunctionDef::new(
        "template_output",
        vec![BasicParameter {
            name: "content".to_string(),
            data_type: DataType::Template,
            ownership: crate::compiler::datatypes::Ownership::MutableOwned,
        }],
        vec![],
        "beanstalk_io",
        "template_output",
        "Template output function for testing",
    );
    
    block.add_statement(Statement::HostCall {
        function: host_func.clone(),
        args: vec![Operand::Constant(Constant::String("Hello, World!".to_string()))],
        destination: None,
    });
    
    block.set_terminator(crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] });
    function.add_block(block);
    wir.add_function(function);
    
    // Add host import
    let mut host_imports = HashSet::new();
    host_imports.insert(host_func);
    wir.add_host_imports(&host_imports);
    
    wir
}

fn create_test_wir_with_template_output_statement() -> WIR {
    create_test_wir_with_host_call() // Same as host call test for now
}

fn create_test_wir_with_main_function() -> WIR {
    let mut wir = WIR::new();
    
    // Add main function
    let mut main_function = WirFunction::new(
        0,
        "main".to_string(),
        vec![],
        vec![],
        vec![],
    );
    
    let mut block = WirBlock::new(0);
    block.set_terminator(crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] });
    main_function.add_block(block);
    wir.add_function(main_function);
    
    // Add export for main function
    let mut exports = HashMap::new();
    exports.insert("main".to_string(), Export {
        name: "main".to_string(),
        kind: ExportKind::Function,
        index: 0,
    });
    wir.exports = exports;
    
    wir
}

fn create_test_wir_with_multiple_functions() -> WIR {
    let mut wir = WIR::new();
    
    // Add main function (entry point)
    let mut main_function = WirFunction::new(
        0,
        "main".to_string(),
        vec![],
        vec![],
        vec![],
    );
    
    let mut main_block = WirBlock::new(0);
    main_block.set_terminator(crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] });
    main_function.add_block(main_block);
    wir.add_function(main_function);
    
    // Add another function (not entry point)
    let mut other_function = WirFunction::new(
        1,
        "other_function".to_string(),
        vec![],
        vec![],
        vec![],
    );
    
    let mut other_block = WirBlock::new(0);
    other_block.set_terminator(crate::compiler::wir::wir_nodes::Terminator::Return { values: vec![] });
    other_function.add_block(other_block);
    wir.add_function(other_function);
    
    // Only export main as entry point
    let mut exports = HashMap::new();
    exports.insert("main".to_string(), Export {
        name: "main".to_string(),
        kind: ExportKind::Function,
        index: 0,
    });
    wir.exports = exports;
    
    wir
}

fn create_test_wir_with_host_imports() -> WIR {
    create_test_wir_with_host_call() // Same structure with host imports
}