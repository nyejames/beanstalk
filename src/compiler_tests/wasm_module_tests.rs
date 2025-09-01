#[cfg(test)]
mod tests {
    use crate::compiler::codegen::wasm_encoding::WasmModule;
    use crate::compiler::mir::mir_nodes::{MIR, MirFunction, MirBlock};
    use crate::compiler::mir::place::{Place, WasmType};

    #[test]
    fn test_wasm_module_from_mir_basic() {
        // Create a basic MIR structure
        let mir = MIR::new();
        
        // Create WasmModule from MIR
        let result = WasmModule::from_mir(&mir);
        assert!(result.is_ok(), "WasmModule::from_mir should succeed");
        
        let wasm_module = result.unwrap();
        
        // Verify that the module was initialized correctly
        assert_eq!(wasm_module.get_function_count(), 0); // No functions compiled yet
        assert_eq!(wasm_module.get_type_count(), 0); // No function types in basic MIR
        assert_eq!(wasm_module.get_global_count(), 0); // No globals
    }

    #[test]
    fn test_wasm_module_compile_function() {
        // Create a basic MIR structure with a function
        let mut mir = MIR::new();
        
        // Add a simple function with at least one block
        let mut function = MirFunction::new(
            0,
            "test_function".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
            vec![WasmType::I32],
        );
        
        // Add a basic block with a simple return
        use crate::compiler::mir::mir_nodes::{MirBlock, Terminator};
        let block = MirBlock::new(0);
        function.blocks.push(block);
        
        mir.add_function(function);
        
        // Create WasmModule from MIR
        let mut wasm_module = WasmModule::from_mir(&mir).unwrap();
        
        // Compile the function
        let result = wasm_module.compile_mir_function(&mir.functions[0]);
        if let Err(ref e) = result {
            println!("Function compilation error: {:?}", e);
        }
        assert!(result.is_ok(), "Function compilation should succeed");
        
        let function_index = result.unwrap();
        assert_eq!(function_index, 0); // First function should have index 0
        assert_eq!(wasm_module.get_function_count(), 1); // One function compiled
        assert_eq!(wasm_module.get_type_count(), 1); // One function type created during compilation
    }

    #[test]
    fn test_place_resolution_local_index_mapping() {
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        // Create a MIR function with parameters and locals
        let mut function = MirFunction::new(
            0,
            "test_function".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::F64 },
            ],
            vec![WasmType::I32],
        );
        
        // Add some local variables
        function.locals.insert(
            "local_var1".to_string(),
            Place::Local { index: 2, wasm_type: WasmType::I32 }
        );
        function.locals.insert(
            "local_var2".to_string(),
            Place::Local { index: 3, wasm_type: WasmType::F32 }
        );
        
        // Create WasmModule and test local index mapping
        let mut wasm_module = WasmModule::new();
        let local_map = wasm_module.build_local_index_mapping(&function).unwrap();
        
        // Verify parameter mapping (should be indices 0, 1)
        assert_eq!(local_map.get(&function.parameters[0]), Some(&0));
        assert_eq!(local_map.get(&function.parameters[1]), Some(&1));
        
        // Verify local variable mapping (should be indices 2, 3 but order may vary)
        let local_var1 = Place::Local { index: 2, wasm_type: WasmType::I32 };
        let local_var2 = Place::Local { index: 3, wasm_type: WasmType::F32 };
        
        // Both local variables should be mapped to indices >= 2 (after parameters)
        let local1_index = local_map.get(&local_var1).copied().unwrap();
        let local2_index = local_map.get(&local_var2).copied().unwrap();
        
        assert!(local1_index >= 2, "Local variable 1 should have index >= 2, got {}", local1_index);
        assert!(local2_index >= 2, "Local variable 2 should have index >= 2, got {}", local2_index);
        assert_ne!(local1_index, local2_index, "Local variables should have different indices");
        
        // Verify local_count was updated
        assert_eq!(wasm_module.get_local_count(), 4);
    }

    #[test]
    fn test_place_resolution_global_mapping() {
        // Create a global place
        let global_place = Place::Global { index: 0, wasm_type: WasmType::I64 };
        
        // Create WasmModule and test global mapping
        let mut wasm_module = WasmModule::new();
        let result = wasm_module.add_global_index_mapping(0, &global_place);
        
        assert!(result.is_ok(), "Global mapping should succeed");
        assert_eq!(result.unwrap(), 0); // Should return the global index
        assert_eq!(wasm_module.get_global_count(), 1); // Global count should be updated
    }

    #[test]
    fn test_place_resolution_load_store_instructions() {
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        // Create a simple function with locals
        let function = MirFunction::new(
            0,
            "test_function".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
            vec![WasmType::I32],
        );
        
        // Create WasmModule and build proper local mapping
        let mut wasm_module = WasmModule::new();
        let local_map = wasm_module.build_local_index_mapping(&function).unwrap();
        
        // Test place load resolution
        let mut wasm_function = Function::new(vec![]);
        let local_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let result = wasm_module.resolve_place_load(&local_place, &mut wasm_function, &local_map);
        if let Err(ref e) = result {
            println!("Place load resolution error: {:?}", e);
        }
        assert!(result.is_ok(), "Place load resolution should succeed");
        
        // Test place store resolution
        let mut wasm_function = Function::new(vec![]);
        let result = wasm_module.resolve_place_store(&local_place, &mut wasm_function, &local_map);
        if let Err(ref e) = result {
            println!("Place store resolution error: {:?}", e);
        }
        assert!(result.is_ok(), "Place store resolution should succeed");
    }

    #[test]
    fn test_wasm_module_with_globals() {
        // Create MIR with globals
        let mut mir = MIR::new();
        
        // Add a global variable
        let global_place = Place::Global { index: 0, wasm_type: WasmType::I32 };
        mir.globals.insert(0, global_place);
        
        // Create WasmModule from MIR
        let result = WasmModule::from_mir(&mir);
        assert!(result.is_ok(), "WasmModule::from_mir with globals should succeed");
        
        let wasm_module = result.unwrap();
        assert_eq!(wasm_module.get_global_count(), 1); // One global added
    }

    #[test]
    fn test_wasm_module_finish() {
        // Create a basic MIR structure
        let mir = MIR::new();
        
        // Create WasmModule from MIR
        let wasm_module = WasmModule::from_mir(&mir).unwrap();
        
        // Finish the module to get WASM bytes
        let wasm_bytes = wasm_module.finish();
        
        // Verify that we got some WASM bytes
        assert!(!wasm_bytes.is_empty(), "WASM module should produce non-empty bytes");
        
        // Verify WASM magic number (0x00 0x61 0x73 0x6D)
        assert_eq!(&wasm_bytes[0..4], &[0x00, 0x61, 0x73, 0x6D], "Should start with WASM magic number");
        
        // Verify WASM version (0x01 0x00 0x00 0x00)
        assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00], "Should have correct WASM version");
    }

    #[test]
    fn test_wasm_module_string_constants() {
        // Create MIR with string constants (this will be tested more thoroughly in later tasks)
        let mir = MIR::new();
        
        // Create WasmModule from MIR
        let wasm_module = WasmModule::from_mir(&mir).unwrap();
        
        // Verify string constants are empty for basic MIR
        assert!(wasm_module.get_string_constants().is_empty(), "Basic MIR should have no string constants");
        assert!(wasm_module.get_string_constant_map().is_empty(), "Basic MIR should have empty string constant map");
    }

    #[test]
    fn test_wasm_module_exports() {
        // Create a basic MIR structure
        let mut mir = MIR::new();
        
        // Add a function
        let mut function = MirFunction::new(
            0,
            "exported_function".to_string(),
            vec![],
            vec![WasmType::I32],
        );
        
        // Add a basic block to the function
        let block = MirBlock::new(0);
        function.blocks.push(block);
        
        mir.add_function(function);
        
        // Create WasmModule from MIR
        let mut wasm_module = WasmModule::from_mir(&mir).unwrap();
        
        // Compile the function
        let function_index = wasm_module.compile_mir_function(&mir.functions[0]).unwrap();
        
        // Add function export
        wasm_module.add_function_export("exported_function", function_index);
        
        // Finish the module
        let wasm_bytes = wasm_module.finish();
        
        // Verify that we got valid WASM bytes
        assert!(!wasm_bytes.is_empty(), "WASM module with exports should produce non-empty bytes");
    }

    #[test]
    fn test_wasm_type_mapping() {
        use wasm_encoder::ValType;
        
        let wasm_module = WasmModule::new();
        
        // Test basic type mappings
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::I32), ValType::I32);
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::I64), ValType::I64);
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::F32), ValType::F32);
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::F64), ValType::F64);
        
        // Test pointer types map to i32 in linear memory model
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::ExternRef), ValType::I32);
        
        // Test function reference mapping
        assert_eq!(wasm_module.wasm_type_to_val_type(&WasmType::FuncRef), ValType::Ref(wasm_encoder::RefType::FUNCREF));
    }

    #[test]
    fn test_function_signature_generation() {
        let mut wasm_module = WasmModule::new();
        
        // Create a function with multiple parameter and return types
        let function = MirFunction::new(
            0,
            "complex_function".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::F64 },
            ],
            vec![WasmType::I64, WasmType::F32],
        );
        
        // Generate function signature
        let result = wasm_module.add_function_signature_from_mir(&function);
        assert!(result.is_ok(), "Function signature generation should succeed");
        
        let type_index = result.unwrap();
        assert_eq!(type_index, 0); // First type should have index 0
        assert_eq!(wasm_module.get_type_count(), 1); // One type added
    }

    #[test]
    fn test_struct_layout_calculation() {
        let wasm_module = WasmModule::new();
        
        // Test struct with mixed field types
        let field_types = vec![
            WasmType::I32,    // 4 bytes, 4-byte aligned
            WasmType::I64,    // 8 bytes, 8-byte aligned
            WasmType::F32,    // 4 bytes, 4-byte aligned
        ];
        
        let layout = wasm_module.calculate_struct_layout(&field_types);
        
        // Verify field offsets are properly aligned
        assert_eq!(layout.get_field_offset(0), Some(0));  // First field at offset 0
        assert_eq!(layout.get_field_offset(1), Some(8));  // Second field aligned to 8 bytes
        assert_eq!(layout.get_field_offset(2), Some(16)); // Third field after 8-byte field
        
        // Verify field sizes
        assert_eq!(layout.get_field_size(0), Some(4));    // I32 is 4 bytes
        assert_eq!(layout.get_field_size(1), Some(8));    // I64 is 8 bytes
        assert_eq!(layout.get_field_size(2), Some(4));    // F32 is 4 bytes
        
        // Verify total size is aligned to largest alignment (8 bytes)
        assert_eq!(layout.total_size, 24); // 20 bytes rounded up to 8-byte boundary
        assert_eq!(layout.alignment, 8);   // Largest alignment requirement
    }

    #[test]
    fn test_type_compatibility_validation() {
        use wasm_encoder::ValType;
        
        let wasm_module = WasmModule::new();
        
        // Test compatible types
        let result = wasm_module.validate_type_compatibility(&WasmType::I32, ValType::I32);
        assert!(result.is_ok(), "I32 should be compatible with I32");
        
        let result = wasm_module.validate_type_compatibility(&WasmType::F64, ValType::F64);
        assert!(result.is_ok(), "F64 should be compatible with F64");
        
        // Test pointer type compatibility (ExternRef maps to I32)
        let result = wasm_module.validate_type_compatibility(&WasmType::ExternRef, ValType::I32);
        assert!(result.is_ok(), "ExternRef should be compatible with I32 in linear memory model");
        
        // Test incompatible types
        let result = wasm_module.validate_type_compatibility(&WasmType::I32, ValType::F32);
        assert!(result.is_err(), "I32 should not be compatible with F32");
    }

    #[test]
    fn test_mir_types_validation() {
        let mut mir = MIR::new();
        
        // Add function with various types
        let function = MirFunction::new(
            0,
            "test_function".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::F64 },
                Place::Local { index: 2, wasm_type: WasmType::ExternRef }, // Pointer type
            ],
            vec![WasmType::I64],
        );
        mir.add_function(function);
        
        // Add global with valid type
        let global_place = Place::Global { index: 0, wasm_type: WasmType::F32 };
        mir.globals.insert(0, global_place);
        
        let wasm_module = WasmModule::new();
        
        // Validate all types in MIR
        let result = wasm_module.validate_mir_types(&mir);
        assert!(result.is_ok(), "All MIR types should be valid for WASM");
    }

    // ===== STATEMENT LOWERING TESTS =====

    #[test]
    fn test_lower_assign_statement_constant() {
        use crate::compiler::mir::mir_nodes::{Statement, Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Create assign statement: local_0 = 42
        let target_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let constant_rvalue = Rvalue::Use(Operand::Constant(Constant::I32(42)));
        let assign_stmt = Statement::Assign { 
            place: target_place, 
            rvalue: constant_rvalue 
        };
        
        // Lower the statement
        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign statement lowering should succeed");
    }

    #[test]
    fn test_lower_call_statement() {
        use crate::compiler::mir::mir_nodes::{Statement, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Create call statement: result = call func(arg1, arg2)
        let func_operand = Operand::Constant(Constant::Function(0));
        let args = vec![
            Operand::Constant(Constant::I32(10)),
            Operand::Constant(Constant::I32(20)),
        ];
        let destination = Some(Place::Local { index: 0, wasm_type: WasmType::I32 });
        
        let call_stmt = Statement::Call { 
            func: func_operand, 
            args, 
            destination 
        };
        
        // Lower the statement
        let result = wasm_module.lower_statement(&call_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Call statement lowering should succeed");
    }

    #[test]
    fn test_lower_drop_statement() {
        use crate::compiler::mir::mir_nodes::Statement;
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Create drop statement: drop local_0
        let target_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let drop_stmt = Statement::Drop { place: target_place };
        
        // Lower the statement
        let result = wasm_module.lower_statement(&drop_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Drop statement lowering should succeed");
    }

    #[test]
    fn test_lower_nop_statement() {
        use crate::compiler::mir::mir_nodes::Statement;
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Create nop statement
        let nop_stmt = Statement::Nop;
        
        // Lower the statement
        let result = wasm_module.lower_statement(&nop_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Nop statement lowering should succeed");
    }

    #[test]
    fn test_lower_operand_constant() {
        use crate::compiler::mir::mir_nodes::{Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test different constant types
        let constants = vec![
            Operand::Constant(Constant::I32(42)),
            Operand::Constant(Constant::I64(123456789)),
            Operand::Constant(Constant::F32(3.14)),
            Operand::Constant(Constant::F64(2.718281828)),
            Operand::Constant(Constant::Bool(true)),
            Operand::Constant(Constant::Bool(false)),
            Operand::Constant(Constant::Null),
        ];
        
        for constant in constants {
            let result = wasm_module.lower_operand(&constant, &mut wasm_function, &local_map);
            assert!(result.is_ok(), "Constant operand lowering should succeed for {:?}", constant);
        }
    }

    #[test]
    fn test_lower_operand_copy_move() {
        use crate::compiler::mir::mir_nodes::Operand;
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();
        
        // Set up local mapping
        let local_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(local_place.clone(), 0);
        
        // Test copy operand
        let copy_operand = Operand::Copy(local_place.clone());
        let result = wasm_module.lower_operand(&copy_operand, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Copy operand lowering should succeed");
        
        // Test move operand
        let move_operand = Operand::Move(local_place);
        let result = wasm_module.lower_operand(&move_operand, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Move operand lowering should succeed");
    }

    #[test]
    fn test_lower_operand_function_global_ref() {
        use crate::compiler::mir::mir_nodes::Operand;
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test function reference
        let func_ref = Operand::FunctionRef(5);
        let result = wasm_module.lower_operand(&func_ref, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Function reference operand lowering should succeed");
        
        // Test global reference
        let global_ref = Operand::GlobalRef(3);
        let result = wasm_module.lower_operand(&global_ref, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Global reference operand lowering should succeed");
    }

    #[test]
    fn test_lower_constant_string() {
        use crate::compiler::mir::mir_nodes::Constant;
        use wasm_encoder::Function;
        
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        
        // Add a string constant to the module's string constant map
        let test_string = "Hello, WASM!".to_string();
        wasm_module.add_string_constant_for_test(test_string.clone(), 1024);
        
        // Test string constant lowering
        let string_constant = Constant::String(test_string);
        let result = wasm_module.lower_constant(&string_constant, &mut wasm_function);
        assert!(result.is_ok(), "String constant lowering should succeed");
    }

    #[test]
    fn test_lower_constant_memory_types() {
        use crate::compiler::mir::mir_nodes::Constant;
        use wasm_encoder::Function;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        
        // Test memory-related constants
        let memory_offset = Constant::MemoryOffset(512);
        let result = wasm_module.lower_constant(&memory_offset, &mut wasm_function);
        assert!(result.is_ok(), "MemoryOffset constant lowering should succeed");
        
        let type_size = Constant::TypeSize(16);
        let result = wasm_module.lower_constant(&type_size, &mut wasm_function);
        assert!(result.is_ok(), "TypeSize constant lowering should succeed");
    }

    #[test]
    fn test_unsupported_statement_types() {
        use crate::compiler::mir::mir_nodes::{Statement, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test unsupported statement types return appropriate errors
        let interface_call = Statement::InterfaceCall {
            interface_id: 0,
            method_id: 1,
            receiver: Operand::Constant(Constant::I32(0)),
            args: vec![],
            destination: None,
        };
        
        let result = wasm_module.lower_statement(&interface_call, &mut wasm_function, &local_map);
        assert!(result.is_err(), "InterfaceCall should return error (not yet implemented)");
        
        let alloc_stmt = Statement::Alloc {
            place: Place::Local { index: 0, wasm_type: WasmType::I32 },
            size: Operand::Constant(Constant::I32(64)),
            align: 4,
        };
        
        let result = wasm_module.lower_statement(&alloc_stmt, &mut wasm_function, &local_map);
        assert!(result.is_err(), "Alloc should return error (not yet implemented)");
    }

    // ===== RVALUE LOWERING TESTS (Task 5) =====

    #[test]
    fn test_lower_rvalue_use_operand() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test Rvalue::Use with constant
        let use_rvalue = Rvalue::Use(Operand::Constant(Constant::I32(42)));
        let result = wasm_module.lower_rvalue(&use_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Rvalue::Use should lower successfully");
    }

    #[test]
    fn test_lower_binary_op_arithmetic() {
        use crate::compiler::mir::mir_nodes::{Rvalue, BinOp, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test arithmetic binary operations
        let arithmetic_ops = vec![
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::Rem,
        ];
        
        for op in arithmetic_ops {
            let binary_op = Rvalue::BinaryOp {
                op: op.clone(),
                left: Operand::Constant(Constant::I32(10)),
                right: Operand::Constant(Constant::I32(5)),
            };
            
            let result = wasm_module.lower_rvalue(&binary_op, &mut wasm_function, &local_map);
            assert!(result.is_ok(), "Arithmetic BinaryOp {:?} should lower successfully", op);
        }
    }

    #[test]
    fn test_lower_binary_op_bitwise() {
        use crate::compiler::mir::mir_nodes::{Rvalue, BinOp, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test bitwise binary operations
        let bitwise_ops = vec![
            BinOp::BitAnd,
            BinOp::BitOr,
            BinOp::BitXor,
            BinOp::Shl,
            BinOp::Shr,
        ];
        
        for op in bitwise_ops {
            let binary_op = Rvalue::BinaryOp {
                op: op.clone(),
                left: Operand::Constant(Constant::I32(0xFF)),
                right: Operand::Constant(Constant::I32(0x0F)),
            };
            
            let result = wasm_module.lower_rvalue(&binary_op, &mut wasm_function, &local_map);
            assert!(result.is_ok(), "Bitwise BinaryOp {:?} should lower successfully", op);
        }
    }

    #[test]
    fn test_lower_binary_op_comparison() {
        use crate::compiler::mir::mir_nodes::{Rvalue, BinOp, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test comparison binary operations
        let comparison_ops = vec![
            BinOp::Eq,
            BinOp::Ne,
            BinOp::Lt,
            BinOp::Le,
            BinOp::Gt,
            BinOp::Ge,
        ];
        
        for op in comparison_ops {
            let binary_op = Rvalue::BinaryOp {
                op: op.clone(),
                left: Operand::Constant(Constant::I32(10)),
                right: Operand::Constant(Constant::I32(20)),
            };
            
            let result = wasm_module.lower_rvalue(&binary_op, &mut wasm_function, &local_map);
            assert!(result.is_ok(), "Comparison BinaryOp {:?} should lower successfully", op);
        }
    }

    #[test]
    fn test_lower_binary_op_logical_error() {
        use crate::compiler::mir::mir_nodes::{Rvalue, BinOp, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test logical operations should return error (should be control flow)
        let logical_ops = vec![BinOp::And, BinOp::Or];
        
        for op in logical_ops {
            let binary_op = Rvalue::BinaryOp {
                op: op.clone(),
                left: Operand::Constant(Constant::Bool(true)),
                right: Operand::Constant(Constant::Bool(false)),
            };
            
            let result = wasm_module.lower_rvalue(&binary_op, &mut wasm_function, &local_map);
            assert!(result.is_err(), "Logical BinaryOp {:?} should return error (should be control flow)", op);
        }
    }

    #[test]
    fn test_lower_unary_op() {
        use crate::compiler::mir::mir_nodes::{Rvalue, UnOp, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test negation
        let neg_op = Rvalue::UnaryOp {
            op: UnOp::Neg,
            operand: Operand::Constant(Constant::I32(42)),
        };
        let result = wasm_module.lower_rvalue(&neg_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "UnaryOp Neg should lower successfully");
        
        // Test bitwise NOT
        let not_op = Rvalue::UnaryOp {
            op: UnOp::Not,
            operand: Operand::Constant(Constant::I32(0xFF)),
        };
        let result = wasm_module.lower_rvalue(&not_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "UnaryOp Not should lower successfully");
    }

    #[test]
    fn test_lower_cast_operations() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test integer conversions
        let i32_to_i64 = Rvalue::Cast {
            source: Operand::Constant(Constant::I32(42)),
            target_type: WasmType::I64,
        };
        let result = wasm_module.lower_rvalue(&i32_to_i64, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Cast I32 to I64 should lower successfully");
        
        let i64_to_i32 = Rvalue::Cast {
            source: Operand::Constant(Constant::I64(123456)),
            target_type: WasmType::I32,
        };
        let result = wasm_module.lower_rvalue(&i64_to_i32, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Cast I64 to I32 should lower successfully");
        
        // Test float conversions
        let f32_to_f64 = Rvalue::Cast {
            source: Operand::Constant(Constant::F32(3.14)),
            target_type: WasmType::F64,
        };
        let result = wasm_module.lower_rvalue(&f32_to_f64, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Cast F32 to F64 should lower successfully");
        
        // Test integer to float conversion
        let i32_to_f32 = Rvalue::Cast {
            source: Operand::Constant(Constant::I32(100)),
            target_type: WasmType::F32,
        };
        let result = wasm_module.lower_rvalue(&i32_to_f32, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Cast I32 to F32 should lower successfully");
        
        // Test float to integer conversion
        let f64_to_i64 = Rvalue::Cast {
            source: Operand::Constant(Constant::F64(42.7)),
            target_type: WasmType::I64,
        };
        let result = wasm_module.lower_rvalue(&f64_to_i64, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Cast F64 to I64 should lower successfully");
    }

    #[test]
    fn test_lower_array_creation() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test array creation with i32 elements
        let array_rvalue = Rvalue::Array {
            elements: vec![
                Operand::Constant(Constant::I32(1)),
                Operand::Constant(Constant::I32(2)),
                Operand::Constant(Constant::I32(3)),
            ],
            element_type: WasmType::I32,
        };
        
        let result = wasm_module.lower_rvalue(&array_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Array creation should lower successfully");
    }

    #[test]
    fn test_lower_struct_creation() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test struct creation with multiple fields
        let struct_rvalue = Rvalue::Struct {
            fields: vec![
                (0, Operand::Constant(Constant::I32(42))),
                (1, Operand::Constant(Constant::F32(3.14))),
                (2, Operand::Constant(Constant::Bool(true))),
            ],
            struct_type: 0,
        };
        
        let result = wasm_module.lower_rvalue(&struct_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Struct creation should lower successfully");
    }

    #[test]
    fn test_lower_memory_operations() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test memory.size
        let memory_size = Rvalue::MemorySize;
        let result = wasm_module.lower_rvalue(&memory_size, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "MemorySize should lower successfully");
        
        // Test memory.grow
        let memory_grow = Rvalue::MemoryGrow {
            pages: Operand::Constant(Constant::I32(1)),
        };
        let result = wasm_module.lower_rvalue(&memory_grow, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "MemoryGrow should lower successfully");
    }

    #[test]
    fn test_infer_operand_type() {
        use crate::compiler::mir::mir_nodes::{Operand, Constant};
        
        let wasm_module = WasmModule::new();
        
        // Test constant type inference
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::I32(42))).unwrap(), WasmType::I32);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::I64(123))).unwrap(), WasmType::I64);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::F32(3.14))).unwrap(), WasmType::F32);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::F64(2.718))).unwrap(), WasmType::F64);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::Bool(true))).unwrap(), WasmType::I32);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::String("test".to_string()))).unwrap(), WasmType::I32);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::Function(0))).unwrap(), WasmType::FuncRef);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Constant(Constant::Null)).unwrap(), WasmType::I32);
        
        // Test place type inference
        let local_place = Place::Local { index: 0, wasm_type: WasmType::F64 };
        assert_eq!(wasm_module.infer_operand_type(&Operand::Copy(local_place.clone())).unwrap(), WasmType::F64);
        assert_eq!(wasm_module.infer_operand_type(&Operand::Move(local_place)).unwrap(), WasmType::F64);
        
        // Test reference type inference
        assert_eq!(wasm_module.infer_operand_type(&Operand::FunctionRef(5)).unwrap(), WasmType::FuncRef);
        assert_eq!(wasm_module.infer_operand_type(&Operand::GlobalRef(3)).unwrap(), WasmType::I32);
    }

    // ===== CONSTANT FOLDING TESTS =====

    #[test]
    fn test_try_fold_rvalue_constants() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        
        let wasm_module = WasmModule::new();
        
        // Test folding Use rvalue
        let use_rvalue = Rvalue::Use(Operand::Constant(Constant::I32(42)));
        let folded = wasm_module.try_fold_rvalue(&use_rvalue);
        assert_eq!(folded, Some(Constant::I32(42)));
        
        // Test non-foldable rvalue
        let local_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let use_place = Rvalue::Use(Operand::Copy(local_place));
        let folded = wasm_module.try_fold_rvalue(&use_place);
        assert_eq!(folded, None);
    }

    #[test]
    fn test_fold_binary_op_constants() {
        use crate::compiler::mir::mir_nodes::{Rvalue, BinOp, Operand, Constant};
        
        let wasm_module = WasmModule::new();
        
        // Test arithmetic folding
        let add_rvalue = Rvalue::BinaryOp {
            op: BinOp::Add,
            left: Operand::Constant(Constant::I32(10)),
            right: Operand::Constant(Constant::I32(5)),
        };
        let folded = wasm_module.try_fold_rvalue(&add_rvalue);
        assert_eq!(folded, Some(Constant::I32(15)));
        
        let mul_rvalue = Rvalue::BinaryOp {
            op: BinOp::Mul,
            left: Operand::Constant(Constant::I32(6)),
            right: Operand::Constant(Constant::I32(7)),
        };
        let folded = wasm_module.try_fold_rvalue(&mul_rvalue);
        assert_eq!(folded, Some(Constant::I32(42)));
        
        // Test comparison folding
        let eq_rvalue = Rvalue::BinaryOp {
            op: BinOp::Eq,
            left: Operand::Constant(Constant::I32(5)),
            right: Operand::Constant(Constant::I32(5)),
        };
        let folded = wasm_module.try_fold_rvalue(&eq_rvalue);
        assert_eq!(folded, Some(Constant::Bool(true)));
        
        let lt_rvalue = Rvalue::BinaryOp {
            op: BinOp::Lt,
            left: Operand::Constant(Constant::I32(3)),
            right: Operand::Constant(Constant::I32(7)),
        };
        let folded = wasm_module.try_fold_rvalue(&lt_rvalue);
        assert_eq!(folded, Some(Constant::Bool(true)));
    }

    #[test]
    fn test_fold_unary_op_constants() {
        use crate::compiler::mir::mir_nodes::{Rvalue, UnOp, Operand, Constant};
        
        let wasm_module = WasmModule::new();
        
        // Test negation folding
        let neg_rvalue = Rvalue::UnaryOp {
            op: UnOp::Neg,
            operand: Operand::Constant(Constant::I32(42)),
        };
        let folded = wasm_module.try_fold_rvalue(&neg_rvalue);
        assert_eq!(folded, Some(Constant::I32(-42)));
        
        // Test bitwise NOT folding
        let not_rvalue = Rvalue::UnaryOp {
            op: UnOp::Not,
            operand: Operand::Constant(Constant::I32(0xFF)),
        };
        let folded = wasm_module.try_fold_rvalue(&not_rvalue);
        assert_eq!(folded, Some(Constant::I32(!0xFF)));
        
        // Test boolean NOT folding
        let bool_not_rvalue = Rvalue::UnaryOp {
            op: UnOp::Not,
            operand: Operand::Constant(Constant::Bool(true)),
        };
        let folded = wasm_module.try_fold_rvalue(&bool_not_rvalue);
        assert_eq!(folded, Some(Constant::Bool(false)));
    }

    #[test]
    fn test_fold_cast_constants() {
        use crate::compiler::mir::mir_nodes::{Rvalue, Operand, Constant};
        
        let wasm_module = WasmModule::new();
        
        // Test integer cast folding
        let i32_to_i64_cast = Rvalue::Cast {
            source: Operand::Constant(Constant::I32(42)),
            target_type: WasmType::I64,
        };
        let folded = wasm_module.try_fold_rvalue(&i32_to_i64_cast);
        assert_eq!(folded, Some(Constant::I64(42)));
        
        // Test float cast folding
        let f32_to_f64_cast = Rvalue::Cast {
            source: Operand::Constant(Constant::F32(3.14)),
            target_type: WasmType::F64,
        };
        let folded = wasm_module.try_fold_rvalue(&f32_to_f64_cast);
        // Note: F32 to F64 conversion may have precision differences
        if let Some(Constant::F64(value)) = folded {
            assert!((value - 3.14).abs() < 0.001, "F32 to F64 cast should be approximately 3.14, got {}", value);
        } else {
            panic!("Expected F64 constant from F32 to F64 cast");
        }
        
        // Test boolean to integer cast folding
        let bool_to_i32_cast = Rvalue::Cast {
            source: Operand::Constant(Constant::Bool(true)),
            target_type: WasmType::I32,
        };
        let folded = wasm_module.try_fold_rvalue(&bool_to_i32_cast);
        assert_eq!(folded, Some(Constant::I32(1)));
    }

    #[test]
    fn test_unsupported_rvalue_types() {
        use crate::compiler::mir::mir_nodes::Rvalue;
        use wasm_encoder::Function;
        use std::collections::HashMap;
        
        let wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();
        
        // Test unsupported rvalue types return appropriate errors
        let ref_rvalue = Rvalue::Ref {
            place: Place::Local { index: 0, wasm_type: WasmType::I32 },
            borrow_kind: crate::compiler::mir::mir_nodes::BorrowKind::Shared,
        };
        let result = wasm_module.lower_rvalue(&ref_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_err(), "Ref rvalue should return error (not yet implemented)");
        
        let deref_rvalue = Rvalue::Deref {
            place: Place::Local { index: 0, wasm_type: WasmType::I32 },
        };
        let result = wasm_module.lower_rvalue(&deref_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_err(), "Deref rvalue should return error (not yet implemented)");
        
        let load_rvalue = Rvalue::Load {
            place: Place::Local { index: 0, wasm_type: WasmType::I32 },
            alignment: 4,
            offset: 0,
        };
        let result = wasm_module.lower_rvalue(&load_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_err(), "Load rvalue should return error (not yet implemented)");
    }
}