#[cfg(test)]
mod wasm_codegen_tests {
    use crate::compiler::codegen::wasm_encoding::WasmModule;
    use crate::compiler::codegen::build_wasm::new_wasm_module;
    use crate::compiler::wir::wir_nodes::{
        WIR, WirBlock, WirFunction, Statement, Terminator, Rvalue, Operand, Constant, BinOp,
        UnOp, Export, ExportKind, MemoryInfo,
    };
    use crate::compiler::wir::place::{Place, WasmType};
    use std::collections::HashMap;
    use wasm_encoder::Function;

    // ===== BASIC STATEMENT LOWERING TESTS =====
    // Test core WIR statement types for proper WASM instruction generation

    #[test]
    fn test_statement_lowering_assign_constant() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        // Test: place = constant
        let target_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let assign_stmt = Statement::Assign {
            place: target_place,
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign constant statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_assign_binary_op() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        // Set up local mapping
        let place_a = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_b = Place::Local { index: 1, wasm_type: WasmType::I32 };
        let place_result = Place::Local { index: 2, wasm_type: WasmType::I32 };
        local_map.insert(place_a.clone(), 0);
        local_map.insert(place_b.clone(), 1);
        local_map.insert(place_result.clone(), 2);

        // Test: result = a + b
        let assign_stmt = Statement::Assign {
            place: place_result,
            rvalue: Rvalue::BinaryOp(BinOp::Add, Operand::Copy(place_a), Operand::Copy(place_b)),
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign binary operation should lower successfully");
    }

    #[test]
    fn test_statement_lowering_assign_unary_op() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let place_input = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_result = Place::Local { index: 1, wasm_type: WasmType::I32 };
        local_map.insert(place_input.clone(), 0);
        local_map.insert(place_result.clone(), 1);

        // Test: result = -input
        let assign_stmt = Statement::Assign {
            place: place_result,
            rvalue: Rvalue::UnaryOp(UnOp::Neg, Operand::Copy(place_input)),
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign unary operation should lower successfully");
    }

    #[test]
    fn test_statement_lowering_call() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let result_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(result_place.clone(), 0);

        // Test: result = call func(arg1, arg2)
        let call_stmt = Statement::Call {
            func: Operand::Constant(Constant::Function(1)),
            args: vec![
                Operand::Constant(Constant::I32(10)),
                Operand::Constant(Constant::I32(20)),
            ],
            destination: Some(result_place),
        };

        let result = wasm_module.lower_statement(&call_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Call statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_nop() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let nop_stmt = Statement::Nop;
        let result = wasm_module.lower_statement(&nop_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Nop statement should lower successfully");
    }

    // ===== BASIC PLACE RESOLUTION TESTS =====

    #[test]
    fn test_place_resolution_local() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let local_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(local_place.clone(), 0);

        let result = wasm_module.resolve_place_load(&local_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Local place resolution should work");
    }

    #[test]
    fn test_place_resolution_global() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let global_place = Place::Global { index: 0, wasm_type: WasmType::I32 };

        let result = wasm_module.resolve_place_load(&global_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Global place resolution should work");
    }

    // ===== BASIC TERMINATOR LOWERING TESTS =====

    #[test]
    fn test_terminator_lowering_goto() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let goto_term = Terminator::Goto { target: 1 };
        let result = wasm_module.lower_terminator(&goto_term, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Goto terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_if() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let condition_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(condition_place.clone(), 0);

        let if_term = Terminator::If {
            condition: Operand::Copy(condition_place),
            then_block: 1,
            else_block: 2,
        };

        let result = wasm_module.lower_terminator(&if_term, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "If terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_return() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let return_term = Terminator::Return {
            values: vec![Operand::Constant(Constant::I32(42))],
        };

        let result = wasm_module.lower_terminator(&return_term, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Return terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_unreachable() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let unreachable_term = Terminator::Unreachable;
        let result = wasm_module.lower_terminator(&unreachable_term, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Unreachable terminator should lower successfully");
    }

    // ===== COMPLETE WIR TO WASM TESTS =====

    #[test]
    fn test_complete_wir_to_wasm_empty_module() {
        let wir = WIR::new();

        let result = new_wasm_module(wir);
        assert!(result.is_ok(), "Empty WIR should generate valid WASM");

        let wasm_bytes = result.unwrap();
        assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
    }

    #[test]
    fn test_complete_wir_to_wasm_simple_function() {
        let mut wir = WIR::new();

        // Create a simple function that returns a constant
        let mut function = WirFunction {
            id: 0,
            name: "test_func".to_string(),
            parameters: vec![],
            return_types: vec![WasmType::I32],
            blocks: vec![WirBlock {
                id: 0,
                statements: vec![],
                terminator: Terminator::Return {
                    values: vec![Operand::Constant(Constant::I32(42))],
                },
            }],
            locals: HashMap::new(),
            signature: crate::compiler::wir::wir_nodes::FunctionSignature {
                params: vec![],
                returns: vec![WasmType::I32],
            },
            events: HashMap::new(),
        };

        wir.add_function(function);

        let result = new_wasm_module(wir);
        assert!(result.is_ok(), "Simple function WIR should generate valid WASM");
    }

    #[test]
    fn test_complete_wir_to_wasm_with_memory() {
        let mut wir = WIR::new();

        // Set up memory configuration
        wir.type_info.memory_info.initial_pages = 1;
        wir.type_info.memory_info.static_data_size = 1024;

        let result = new_wasm_module(wir);
        assert!(result.is_ok(), "WIR with memory should generate valid WASM");
    }

    #[test]
    fn test_wasm_module_validation_integration() {
        let mut wir = WIR::new();

        // Create a function with basic operations
        let function = WirFunction {
            id: 0,
            name: "main".to_string(),
            parameters: vec![],
            return_types: vec![],
            blocks: vec![WirBlock {
                id: 0,
                statements: vec![Statement::Nop],
                terminator: Terminator::Return { values: vec![] },
            }],
            locals: HashMap::new(),
            signature: crate::compiler::wir::wir_nodes::FunctionSignature {
                params: vec![],
                returns: vec![],
            },
            events: HashMap::new(),
        };

        wir.add_function(function);

        let result = new_wasm_module(wir);
        assert!(result.is_ok(), "Basic WIR should pass WASM validation");

        // Verify the generated WASM is valid
        let wasm_bytes = result.unwrap();
        assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
        
        // Basic validation - should be parseable as WASM
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM should pass wasmparser validation");
    }
}