//! Tests for interface VTable generation in WASM backend
//! 
//! This module tests the interface support functionality added to WasmModule,
//! including vtable layout calculation, function table population, and 
//! call_indirect instruction generation.

use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::mir::mir_nodes::{MIR, InterfaceInfo, InterfaceDefinition, MethodSignature, VTable, Constant, Operand};
use crate::compiler::mir::place::{WasmType, Place};
use crate::compiler::compiler_errors::CompileError;
use wasm_encoder::{Function, ValType};
use std::collections::HashMap;

/// Test basic interface method type generation
#[test]
fn test_interface_method_type_generation() -> Result<(), CompileError> {
    let mut wasm_module = WasmModule::new();
    
    // Create a simple method signature
    let method = MethodSignature {
        id: 0,
        name: "test_method".to_string(),
        param_types: vec![WasmType::I32, WasmType::I32], // receiver + one parameter
        return_types: vec![WasmType::I32],
    };
    
    // Add the method signature to the type section
    let type_index = wasm_module.add_interface_method_signature(&method)?;
    
    // Verify that the type index was assigned correctly
    assert_eq!(type_index, 0); // First type should get index 0
    assert_eq!(wasm_module.get_type_count(), 1);
    
    Ok(())
}

/// Test interface method mapping creation
#[test]
fn test_interface_method_mapping() {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    let mapping = wasm_module.create_interface_method_mapping(&interface_info);
    
    // Verify that the mapping contains the expected method implementations
    assert_eq!(mapping.get_method_implementation(0, 0, 1), Some(10)); // interface 0, method 0, type 1 -> function 10
    assert_eq!(mapping.get_method_implementation(0, 0, 2), Some(20)); // interface 0, method 0, type 2 -> function 20
    
    // Verify implementers list
    let implementers = mapping.get_method_implementers(0, 0);
    assert!(implementers.is_some());
    let implementers = implementers.unwrap();
    assert_eq!(implementers.len(), 2);
    assert!(implementers.contains(&1));
    assert!(implementers.contains(&2));
}

/// Test vtable offset calculation
#[test]
fn test_vtable_offset_calculation() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Calculate vtable offset for interface 0, type 1
    let offset = wasm_module.calculate_vtable_offset(0, 1, &interface_info)?;
    assert_eq!(offset, 0); // First vtable should be at offset 0
    
    // Calculate vtable offset for interface 0, type 2
    let offset = wasm_module.calculate_vtable_offset(0, 2, &interface_info)?;
    assert_eq!(offset, 8); // Second vtable should be after first (2 methods * 4 bytes = 8)
    
    Ok(())
}

/// Test interface method type index calculation
#[test]
fn test_interface_method_type_index() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Get type index for interface 0, method 0
    let type_index = wasm_module.get_interface_method_type_index(0, 0, &interface_info)?;
    assert_eq!(type_index, 0); // Should be the first type index
    
    // Get type index for interface 0, method 1
    let type_index = wasm_module.get_interface_method_type_index(0, 1, &interface_info)?;
    assert_eq!(type_index, 1); // Should be the second type index
    
    Ok(())
}

/// Test interface call type validation
#[test]
fn test_interface_call_type_validation() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Valid call - correct receiver and argument types
    let receiver_type = WasmType::I32;
    let arg_types = vec![WasmType::I32];
    
    let result = wasm_module.validate_interface_call_types(0, 0, &receiver_type, &arg_types, &interface_info);
    assert!(result.is_ok());
    
    // Invalid call - wrong argument type
    let wrong_arg_types = vec![WasmType::F32];
    let result = wasm_module.validate_interface_call_types(0, 0, &receiver_type, &wrong_arg_types, &interface_info);
    assert!(result.is_err());
    
    // Invalid call - wrong number of arguments
    let too_many_args = vec![WasmType::I32, WasmType::I32];
    let result = wasm_module.validate_interface_call_types(0, 0, &receiver_type, &too_many_args, &interface_info);
    assert!(result.is_err());
    
    Ok(())
}

/// Test method offset calculation within vtable
#[test]
fn test_method_offset_in_vtable() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Method 0 should be at offset 0
    let offset = wasm_module.calculate_method_offset_in_vtable(0, 0, &interface_info)?;
    assert_eq!(offset, 0);
    
    // Method 1 should be at offset 4 (4 bytes per function index)
    let offset = wasm_module.calculate_method_offset_in_vtable(0, 1, &interface_info)?;
    assert_eq!(offset, 4);
    
    Ok(())
}

/// Test error handling for invalid interface/method IDs
#[test]
fn test_invalid_interface_method_errors() {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Invalid interface ID
    let result = wasm_module.get_interface_method_type_index(999, 0, &interface_info);
    assert!(result.is_err());
    
    // Invalid method ID
    let result = wasm_module.get_interface_method_type_index(0, 999, &interface_info);
    assert!(result.is_err());
    
    // Invalid vtable lookup
    let result = wasm_module.calculate_vtable_offset(999, 1, &interface_info);
    assert!(result.is_err());
}

/// Helper function to create test interface info
fn create_test_interface_info() -> InterfaceInfo {
    let mut interfaces = HashMap::new();
    let mut vtables = HashMap::new();
    
    // Create a test interface with 2 methods
    let interface_def = InterfaceDefinition {
        id: 0,
        name: "TestInterface".to_string(),
        methods: vec![
            MethodSignature {
                id: 0,
                name: "method1".to_string(),
                param_types: vec![WasmType::I32, WasmType::I32], // receiver + parameter
                return_types: vec![WasmType::I32],
            },
            MethodSignature {
                id: 1,
                name: "method2".to_string(),
                param_types: vec![WasmType::I32, WasmType::F32], // receiver + parameter
                return_types: vec![WasmType::F32],
            },
        ],
    };
    
    interfaces.insert(0, interface_def);
    
    // Create vtables for two implementing types
    let vtable1 = VTable {
        interface_id: 0,
        type_id: 1,
        method_functions: vec![10, 11], // Function indices for type 1's implementations
    };
    
    let vtable2 = VTable {
        interface_id: 0,
        type_id: 2,
        method_functions: vec![20, 21], // Function indices for type 2's implementations
    };
    
    vtables.insert(0, vtable1);
    vtables.insert(1, vtable2);
    
    InterfaceInfo {
        interfaces,
        vtables,
        function_table: vec![10, 11, 20, 21], // All interface method implementations
    }
}

/// Integration test for complete interface support initialization
#[test]
fn test_interface_support_initialization() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let mut mir = create_test_mir_with_interfaces(interface_info);
    
    // Create WasmModule from MIR with interface support
    let wasm_module = WasmModule::from_mir(&mir)?;
    
    // Verify that interface support was initialized
    assert_eq!(wasm_module.get_type_count(), 2); // Should have 2 interface method types
    
    Ok(())
}

/// Helper function to create a test MIR with interface information
fn create_test_mir_with_interfaces(interface_info: InterfaceInfo) -> MIR {
    use crate::compiler::mir::mir_nodes::{TypeInfo, MemoryInfo};
    
    MIR {
        functions: vec![],
        globals: HashMap::new(),
        exports: HashMap::new(),
        type_info: TypeInfo {
            function_types: vec![],
            global_types: vec![],
            memory_info: MemoryInfo {
                initial_pages: 1,
                max_pages: Some(10),
                static_data_size: 0,
            },
            interface_info,
        },
    }
}

/// Test complete interface call lowering to WASM instructions
#[test]
fn test_interface_call_lowering() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Create test operands
    let receiver = Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 });
    let args = vec![Operand::Constant(Constant::I32(42))];
    let destination = Some(Place::Local { index: 1, wasm_type: WasmType::I32 });
    
    // Create local mapping
    let mut local_map = HashMap::new();
    local_map.insert(Place::Local { index: 0, wasm_type: WasmType::I32 }, 0);
    local_map.insert(Place::Local { index: 1, wasm_type: WasmType::I32 }, 1);
    
    // Create a WASM function to test lowering
    let locals = vec![(2, ValType::I32)]; // 2 locals of type i32
    let mut function = Function::new(locals);
    
    // Test the interface call lowering
    let result = wasm_module.lower_interface_call(
        0, // interface_id
        0, // method_id
        &receiver,
        &args,
        &destination,
        &mut function,
        &local_map,
        &interface_info
    );
    
    // Should succeed with valid interface call
    assert!(result.is_ok(), "Interface call lowering should succeed: {:?}", result);
    
    Ok(())
}

/// Test interface call error handling with invalid interface
#[test]
fn test_interface_call_error_handling() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Create test operands
    let receiver = Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 });
    let args = vec![Operand::Constant(Constant::I32(42))];
    let destination = Some(Place::Local { index: 1, wasm_type: WasmType::I32 });
    
    // Create local mapping
    let mut local_map = HashMap::new();
    local_map.insert(Place::Local { index: 0, wasm_type: WasmType::I32 }, 0);
    local_map.insert(Place::Local { index: 1, wasm_type: WasmType::I32 }, 1);
    
    // Create a WASM function to test lowering
    let locals = vec![(2, ValType::I32)];
    let mut function = Function::new(locals);
    
    // Test with invalid interface ID
    let result = wasm_module.lower_interface_call(
        999, // invalid interface_id
        0,   // method_id
        &receiver,
        &args,
        &destination,
        &mut function,
        &local_map,
        &interface_info
    );
    
    // Should fail with interface not found error
    assert!(result.is_err(), "Interface call with invalid interface should fail");
    
    // Test with invalid method ID
    let result = wasm_module.lower_interface_call(
        0,   // interface_id
        999, // invalid method_id
        &receiver,
        &args,
        &destination,
        &mut function,
        &local_map,
        &interface_info
    );
    
    // Should fail with method not found error
    assert!(result.is_err(), "Interface call with invalid method should fail");
    
    Ok(())
}

/// Test vtable pointer loading from receiver object
#[test]
fn test_receiver_vtable_loading() -> Result<(), CompileError> {
    let wasm_module = WasmModule::new();
    
    // Create test receiver
    let receiver = Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 });
    
    // Create local mapping
    let mut local_map = HashMap::new();
    local_map.insert(Place::Local { index: 0, wasm_type: WasmType::I32 }, 0);
    
    // Create a WASM function to test vtable loading
    let locals = vec![(1, ValType::I32)];
    let mut function = Function::new(locals);
    
    // Test vtable pointer loading
    let result = wasm_module.load_receiver_vtable_pointer(&receiver, &mut function, &local_map);
    
    // Should succeed
    assert!(result.is_ok(), "Vtable pointer loading should succeed: {:?}", result);
    
    Ok(())
}

/// Test method function index loading from vtable
#[test]
fn test_method_function_index_loading() -> Result<(), CompileError> {
    let interface_info = create_test_interface_info();
    let wasm_module = WasmModule::new();
    
    // Create a WASM function to test method loading
    let locals = vec![(1, ValType::I32)];
    let mut function = Function::new(locals);
    
    // Test method function index loading
    let result = wasm_module.load_method_function_index(0, 0, &mut function, &interface_info);
    
    // Should succeed
    assert!(result.is_ok(), "Method function index loading should succeed: {:?}", result);
    
    Ok(())
}