use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::mir::build_mir::MIR;

/// Test basic WASM validation with empty MIR
#[test]
fn test_basic_wasm_validation_empty_mir() {
    let mir = MIR::new();
    
    // Test WASM module generation with validation on empty MIR
    let result = new_wasm_module(mir);
    
    // Should succeed for empty MIR (creates minimal WASM module)
    assert!(result.is_ok(), "WASM validation should pass for empty MIR: {:?}", result.err());
    
    let wasm_bytes = result.unwrap();
    assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
}

/// Test WASM validation with invalid memory configuration
#[test]
fn test_wasm_validation_with_invalid_memory_config() {
    let mut mir = MIR::new();
    
    // Set up invalid memory configuration - static data but no memory pages
    mir.type_info.memory_info.initial_pages = 0;
    mir.type_info.memory_info.static_data_size = 1024; // 1KB of static data but no pages
    
    // Test WASM module generation - should fail validation
    let result = new_wasm_module(mir);
    
    // Should fail due to invalid memory configuration
    assert!(result.is_err(), "WASM validation should fail for invalid memory configuration");
    
    let error = result.unwrap_err();
    assert!(
        error.msg.contains("static data") || error.msg.contains("memory"),
        "Error should mention memory issue: {}",
        error.msg
    );
}

/// Test WASM validation with valid memory configuration
#[test]
fn test_wasm_validation_with_valid_memory_config() {
    let mut mir = MIR::new();
    
    // Set up valid memory configuration
    mir.type_info.memory_info.initial_pages = 1;
    mir.type_info.memory_info.static_data_size = 1024; // 1KB of static data
    
    // Test WASM module generation with memory
    let result = new_wasm_module(mir);
    
    // Should succeed with proper memory configuration
    assert!(result.is_ok(), "WASM validation should pass with proper memory configuration: {:?}", result.err());
}

/// Test WASM validation with invalid memory limits
#[test]
fn test_wasm_validation_with_invalid_memory_limits() {
    let mut mir = MIR::new();
    
    // Set up invalid memory limits - max less than initial
    mir.type_info.memory_info.initial_pages = 5;
    mir.type_info.memory_info.max_pages = Some(2); // Max less than initial - invalid!
    mir.type_info.memory_info.static_data_size = 1024;
    
    // Test WASM module generation - should fail validation
    let result = new_wasm_module(mir);
    
    // Should fail due to invalid memory limits
    assert!(result.is_err(), "WASM validation should fail for invalid memory limits");
    
    let error = result.unwrap_err();
    assert!(
        error.msg.contains("max pages") || error.msg.contains("initial pages"),
        "Error should mention memory limit issue: {}",
        error.msg
    );
}

/// Test WASM validation context tracking
#[test]
fn test_wasm_validation_context_tracking() {
    let mir = MIR::new();
    
    // Test that validation context is properly built and used
    let result = new_wasm_module(mir);
    
    // Should succeed and not panic during context building
    assert!(result.is_ok(), "WASM validation context should be built correctly: {:?}", result.err());
}