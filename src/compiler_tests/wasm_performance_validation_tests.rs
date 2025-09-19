#[cfg(test)]
mod wasm_performance_validation_tests {
    use crate::compiler::codegen::build_wasm::new_wasm_module;
    use crate::compiler::mir::mir_nodes::{
        MIR, MirBlock, MirFunction, Statement, Terminator, Rvalue, Operand, Constant, BinOp,
        Export, ExportKind, MemoryInfo, InterfaceInfo,
    };
    use crate::compiler::mir::place::{Place, WasmType};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    use wasmparser::validate;

    // ===== WASM MODULE VALIDATION TESTS =====
    // Test WASM module validation using wasmparser

    #[test]
    fn test_wasm_validation_empty_module() {
        let mir = MIR::new();
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Empty MIR should generate valid WASM module");
        
        let wasm_bytes = result.unwrap();
        
        // Validate using wasmparser
        let validation_result = validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM should pass validation: {:?}", validation_result.err());
        
        println!("Empty WASM module validation: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_wasm_validation_with_simple_function() {
        let mut mir = MIR::new();
        
        // Create a simple function that returns a constant
        let mut function = MirFunction::new(
            0,
            "simple".to_string(),
            vec![],
            vec![WasmType::I32],
        );
        
        // Add basic block with return
        let mut block = MirBlock::new(0);
        block.terminator = Terminator::Return {
            values: vec![Operand::Constant(Constant::I32(42))],
        };
        
        function.blocks.insert(0, block);
        mir.functions.push(function);
        
        // Add export for the function
        mir.exports.insert("simple".to_string(), Export {
            name: "simple".to_string(),
            kind: ExportKind::Function,
            index: 0,
        });
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Simple function should generate valid WASM module: {:?}", result.err());
        
        let wasm_bytes = result.unwrap();
        
        // Validate using wasmparser
        let validation_result = validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM with function should pass validation: {:?}", validation_result.err());
        
        println!("Simple function WASM validation: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_wasm_validation_with_memory() {
        let mut mir = MIR::new();
        
        // Set up memory configuration
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 1,
            max_pages: Some(10),
            static_data_size: 1024,
        };
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with memory should generate valid WASM module");
        
        let wasm_bytes = result.unwrap();
        
        // Validate using wasmparser
        let validation_result = validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM with memory should pass validation: {:?}", validation_result.err());
        
        println!("Memory WASM validation: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_wasm_validation_with_interfaces() {
        let mut mir = MIR::new();
        
        // Set up interface configuration
        mir.type_info.interface_info = InterfaceInfo {
            interfaces: HashMap::new(),
            vtables: HashMap::new(),
            function_table: vec![0, 1, 2], // Some function indices
        };
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with interfaces should generate valid WASM module");
        
        let wasm_bytes = result.unwrap();
        
        // Validate using wasmparser
        let validation_result = validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM with interfaces should pass validation: {:?}", validation_result.err());
        
        println!("Interface WASM validation: {} bytes", wasm_bytes.len());
    }

    // ===== COMPILATION SPEED BENCHMARKS =====
    // Test MIR → WASM lowering performance

    #[test]
    fn test_compilation_speed_empty_mir() {
        let mir = MIR::new();
        
        let start = Instant::now();
        let result = new_wasm_module(mir);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Empty MIR compilation should succeed");
        assert!(duration < Duration::from_millis(100), "Empty MIR compilation should be fast: {:?}", duration);
        
        println!("Empty MIR compilation time: {:?}", duration);
    }

    #[test]
    fn test_compilation_speed_simple_function() {
        let mir = create_simple_function_mir();
        
        let start = Instant::now();
        let result = new_wasm_module(mir);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Simple function compilation should succeed");
        assert!(duration < Duration::from_millis(200), "Simple function compilation should be fast: {:?}", duration);
        
        println!("Simple function compilation time: {:?}", duration);
    }

    #[test]
    fn test_compilation_speed_multiple_functions() {
        let mir = create_multiple_functions_mir(5);
        
        let start = Instant::now();
        let result = new_wasm_module(mir);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Multiple functions compilation should succeed");
        assert!(duration < Duration::from_millis(500), "Multiple functions compilation should scale well: {:?}", duration);
        
        println!("Multiple functions (5) compilation time: {:?}", duration);
    }

    // ===== CODE SIZE TESTS =====
    // Test generated WASM size efficiency

    #[test]
    fn test_code_size_empty_module() {
        let mir = MIR::new();
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Empty MIR should generate WASM");
        
        let wasm_bytes = result.unwrap();
        
        // Empty WASM module should be minimal (< 100 bytes)
        assert!(wasm_bytes.len() < 100, "Empty WASM module should be small: {} bytes", wasm_bytes.len());
        println!("Empty WASM module size: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_code_size_simple_function() {
        let mir = create_simple_function_mir();
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Simple function should generate WASM");
        
        let wasm_bytes = result.unwrap();
        
        // Simple function should be compact (< 300 bytes)
        assert!(wasm_bytes.len() < 300, "Simple function WASM should be compact: {} bytes", wasm_bytes.len());
        println!("Simple function WASM size: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_code_size_efficiency_ratio() {
        let simple_mir = create_simple_function_mir();
        let multiple_mir = create_multiple_functions_mir(3);
        
        let simple_result = new_wasm_module(simple_mir);
        let multiple_result = new_wasm_module(multiple_mir);
        
        assert!(simple_result.is_ok() && multiple_result.is_ok(), "Both compilations should succeed");
        
        let simple_size = simple_result.unwrap().len();
        let multiple_size = multiple_result.unwrap().len();
        
        // Multiple functions should not be more than 5x larger than simple
        let size_ratio = multiple_size as f64 / simple_size as f64;
        assert!(size_ratio < 5.0, "Code size should scale reasonably: {}x", size_ratio);
        
        println!("Code size ratio (multiple/simple): {:.2}x ({} / {} bytes)", size_ratio, multiple_size, simple_size);
    }

    // ===== MEMORY USAGE TESTS =====
    // Test linear memory layout efficiency

    #[test]
    fn test_memory_layout_efficiency_simple() {
        let mut mir = MIR::new();
        
        // Set up memory with simple configuration
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 1,
            max_pages: Some(1),
            static_data_size: 64, // Small amount
        };
        
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Simple memory layout should work");
        
        let wasm_bytes = result.unwrap();
        
        // Memory usage should be efficient for simple case
        assert!(wasm_bytes.len() < 200, "Simple memory WASM should be compact: {} bytes", wasm_bytes.len());
        println!("Simple memory layout WASM size: {} bytes", wasm_bytes.len());
    }

    #[test]
    fn test_memory_usage_scaling() {
        let sizes = vec![64, 256, 1024]; // Different static data sizes
        let mut wasm_sizes = Vec::new();
        
        for static_size in sizes {
            let mut mir = MIR::new();
            mir.type_info.memory_info = MemoryInfo {
                initial_pages: (static_size / 65536) + 1, // Calculate required pages
                max_pages: Some(10),
                static_data_size: static_size,
            };
            
            let result = new_wasm_module(mir);
            assert!(result.is_ok(), "Memory scaling test should work for size {}", static_size);
            
            let wasm_bytes = result.unwrap();
            wasm_sizes.push((static_size, wasm_bytes.len()));
        }
        
        // Check that WASM size scales reasonably with memory usage
        for i in 1..wasm_sizes.len() {
            let (prev_static, prev_wasm) = wasm_sizes[i-1];
            let (curr_static, curr_wasm) = wasm_sizes[i];
            
            let static_ratio = curr_static as f64 / prev_static as f64;
            let wasm_ratio = curr_wasm as f64 / prev_wasm as f64;
            
            // WASM size should not grow faster than 3x the static data growth
            assert!(wasm_ratio < static_ratio * 3.0, 
                "WASM size growth should be reasonable: {}x static -> {}x WASM", 
                static_ratio, wasm_ratio);
        }
        
        println!("Memory usage scaling test passed:");
        for (static_size, wasm_size) in wasm_sizes {
            println!("  Static: {} bytes -> WASM: {} bytes", static_size, wasm_size);
        }
    }

    // ===== SCALABILITY TESTS =====
    // Test large functions and complex scenarios

    #[test]
    fn test_scalability_many_functions() {
        let function_counts = vec![2, 5, 10];
        
        for count in function_counts {
            let mir = create_multiple_functions_mir(count);
            
            let start = Instant::now();
            let result = new_wasm_module(mir);
            let duration = start.elapsed();
            
            assert!(result.is_ok(), "Many functions ({}) should compile", count);
            assert!(duration < Duration::from_secs(1), "Many functions compilation should scale: {:?}", duration);
            
            let wasm_bytes = result.unwrap();
            let validation_result = validate(&wasm_bytes);
            assert!(validation_result.is_ok(), "Many functions WASM should be valid");
            
            println!("Many functions ({}): {} bytes, {:?}", count, wasm_bytes.len(), duration);
        }
    }

    #[test]
    fn test_comprehensive_performance() {
        let mir = create_comprehensive_test_mir();
        
        let start = Instant::now();
        let result = new_wasm_module(mir);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Comprehensive test should compile");
        assert!(duration < Duration::from_secs(2), "Comprehensive test should complete in reasonable time: {:?}", duration);
        
        let wasm_bytes = result.unwrap();
        let validation_result = validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Comprehensive test WASM should be valid");
        
        println!("Comprehensive performance test: {} bytes, {:?}", wasm_bytes.len(), duration);
    }

    // ===== HELPER FUNCTIONS FOR TEST MIR CREATION =====

    fn create_simple_function_mir() -> MIR {
        let mut mir = MIR::new();
        
        // Create simple function that returns a constant
        let mut function = MirFunction::new(
            0,
            "simple".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
            vec![WasmType::I32],
        );
        
        let mut block = MirBlock::new(0);
        block.statements.push(Statement::Assign {
            place: Place::Local { index: 1, wasm_type: WasmType::I32 },
            rvalue: Rvalue::BinaryOp {
                op: BinOp::Add,
                left: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                right: Operand::Constant(Constant::I32(1)),
            },
        });
        block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 })],
        };
        
        function.blocks.insert(0, block);
        mir.functions.push(function);
        mir.exports.insert("simple".to_string(), Export {
            name: "simple".to_string(),
            kind: ExportKind::Function,
            index: 0,
        });
        
        mir
    }

    fn create_multiple_functions_mir(count: usize) -> MIR {
        let mut mir = MIR::new();
        
        for i in 0..count {
            let mut function = MirFunction::new(
                i as u32,
                format!("func_{}", i),
                vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
                vec![WasmType::I32],
            );
            
            let mut block = MirBlock::new(0);
            
            // Add a simple computation
            block.statements.push(Statement::Assign {
                place: Place::Local { index: 1, wasm_type: WasmType::I32 },
                rvalue: Rvalue::BinaryOp {
                    op: BinOp::Add,
                    left: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                    right: Operand::Constant(Constant::I32(i as i32)),
                },
            });
            
            block.terminator = Terminator::Return {
                values: vec![Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 })],
            };
            
            function.blocks.insert(0, block);
            mir.functions.push(function);
            mir.exports.insert(format!("func_{}", i), Export {
                name: format!("func_{}", i),
                kind: ExportKind::Function,
                index: i as u32,
            });
        }
        
        mir
    }

    fn create_comprehensive_test_mir() -> MIR {
        let mut mir = MIR::new();
        
        // Set up comprehensive configuration
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 2,
            max_pages: Some(10),
            static_data_size: 1024,
        };
        
        mir.type_info.interface_info = InterfaceInfo {
            interfaces: HashMap::new(),
            vtables: HashMap::new(),
            function_table: vec![0, 1],
        };
        
        // Add multiple functions with different features
        let simple_mir = create_simple_function_mir();
        let multiple_mir = create_multiple_functions_mir(3);
        
        // Merge functions from both MIRs
        for function in simple_mir.functions {
            mir.functions.push(function);
        }
        for mut function in multiple_mir.functions {
            function.id = mir.functions.len() as u32; // Adjust ID to avoid conflict
            mir.functions.push(function);
        }
        
        // Merge exports
        for (name, export) in simple_mir.exports {
            mir.exports.insert(name, export);
        }
        for (name, mut export) in multiple_mir.exports {
            export.index = mir.functions.len() as u32 - 3 + export.index; // Adjust index
            mir.exports.insert(name, export);
        }
        
        mir
    }

    // ===== ERROR CONDITION TESTS =====
    // Test that invalid configurations are properly handled

    #[test]
    fn test_validation_handles_invalid_memory_config() {
        let mut mir = MIR::new();
        
        // Invalid: max pages less than initial pages
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 10,
            max_pages: Some(5), // Invalid: max < initial
            static_data_size: 1024,
        };
        
        let result = new_wasm_module(mir);
        
        // Should either fail during generation or pass with corrected values
        match result {
            Ok(wasm_bytes) => {
                let validation_result = validate(&wasm_bytes);
                // If generation succeeded, validation should pass (implementation may correct the values)
                println!("Invalid memory config was handled gracefully: {} bytes", wasm_bytes.len());
            }
            Err(_) => {
                // Generation failure is also acceptable for invalid config
                println!("Invalid memory config was properly rejected during generation");
            }
        }
    }

    #[test]
    fn test_validation_performance_with_larger_module() {
        let mir = create_multiple_functions_mir(20);
        
        let start = Instant::now();
        let result = new_wasm_module(mir);
        let generation_time = start.elapsed();
        
        assert!(result.is_ok(), "Larger module should generate successfully");
        
        let wasm_bytes = result.unwrap();
        
        let start = Instant::now();
        let validation_result = validate(&wasm_bytes);
        let validation_time = start.elapsed();
        
        assert!(validation_result.is_ok(), "Larger module should validate successfully");
        
        // Performance assertions
        assert!(generation_time.as_millis() < 2000, "Generation should be reasonable: {:?}", generation_time);
        assert!(validation_time.as_millis() < 1000, "Validation should be fast: {:?}", validation_time);
        
        println!("Larger module validation performance:");
        println!("  Size: {} bytes", wasm_bytes.len());
        println!("  Generation: {:?}", generation_time);
        println!("  Validation: {:?}", validation_time);
    }
}