//! Integration tests for the complete LIR to WASM codegen pipeline
//!
//! These tests validate end-to-end transformation from LIR modules to valid WASM bytecode.
//! They test the complete pipeline including:
//! - LIR analysis
//! - WASM module building
//! - Instruction lowering
//! - Control flow management
//! - Memory management
//! - Validation
//!
//! Task 15.2: Write integration tests for complete pipeline
//! Requirements: All requirements

#[cfg(test)]
mod pipeline_integration_tests {
    use crate::compiler::codegen::wasm::encode::encode_wasm;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirModule, LirType};

    /// Helper to create a minimal LIR module with a single function
    fn create_minimal_module(name: &str, body: Vec<LirInst>, is_main: bool) -> LirModule {
        LirModule {
            functions: vec![LirFunction {
                name: name.to_string(),
                params: vec![],
                returns: vec![],
                locals: vec![],
                body,
                is_main,
            }],
            structs: vec![],
        }
    }

    /// Helper to create a function with parameters and return type
    fn create_function_with_signature(
        name: &str,
        params: Vec<LirType>,
        returns: Vec<LirType>,
        locals: Vec<LirType>,
        body: Vec<LirInst>,
        is_main: bool,
    ) -> LirFunction {
        LirFunction {
            name: name.to_string(),
            params,
            returns,
            locals,
            body,
            is_main,
        }
    }


    // =========================================================================
    // Basic Pipeline Tests - End-to-End LIR to WASM Transformation
    // =========================================================================

    /// Test: Empty module generates valid WASM
    /// Validates: Requirements 1.1, 8.1
    #[test]
    fn test_empty_main_function_generates_valid_wasm() {
        let module = create_minimal_module("main", vec![], true);
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Empty main function should generate valid WASM");
        let wasm_bytes = result.unwrap();
        assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");

        // Validate the WASM module using wasmparser
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: Simple constant function generates valid WASM
    /// Validates: Requirements 1.3, 7.1
    #[test]
    fn test_constant_function_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I32Const(42),
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Constant function should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: Function with arithmetic operations generates valid WASM
    /// Validates: Requirements 1.3, 7.1
    #[test]
    fn test_arithmetic_function_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I32Const(10),
                LirInst::I32Const(20),
                LirInst::I32Add,
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Arithmetic function should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    /// Test: Function with local variables generates valid WASM
    /// Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5
    #[test]
    fn test_local_variables_generate_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![],
                vec![LirType::I32, LirType::I64],
                vec![
                    LirInst::I32Const(42),
                    LirInst::LocalSet(0),
                    LirInst::I64Const(100),
                    LirInst::LocalSet(1),
                    LirInst::LocalGet(0),
                    LirInst::Drop,
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Function with locals should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: Function with parameters generates valid WASM
    /// Validates: Requirements 2.1, 2.5
    #[test]
    fn test_function_with_parameters_generates_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![LirType::I32, LirType::I32],
                vec![],
                vec![],
                vec![
                    LirInst::LocalGet(0),
                    LirInst::LocalGet(1),
                    LirInst::I32Add,
                    LirInst::Drop,
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Function with parameters should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    /// Test: Function with return value generates valid WASM
    /// Validates: Requirements 4.4
    #[test]
    fn test_function_with_return_generates_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![LirType::I32],
                vec![],
                vec![
                    LirInst::I32Const(42),
                    LirInst::Return,
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Function with return should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    // =========================================================================
    // Control Flow Integration Tests
    // =========================================================================

    /// Test: Simple if/else generates valid WASM
    /// Validates: Requirements 4.1, 4.5
    #[test]
    fn test_if_else_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I32Const(1), // condition
                LirInst::If {
                    then_branch: vec![
                        LirInst::I32Const(10),
                        LirInst::Drop,
                    ],
                    else_branch: Some(vec![
                        LirInst::I32Const(20),
                        LirInst::Drop,
                    ]),
                },
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "If/else should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: If without else generates valid WASM
    /// Validates: Requirements 4.1
    #[test]
    fn test_if_without_else_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I32Const(1), // condition
                LirInst::If {
                    then_branch: vec![
                        LirInst::I32Const(10),
                        LirInst::Drop,
                    ],
                    else_branch: None,
                },
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "If without else should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    /// Test: Loop with break generates valid WASM
    /// Validates: Requirements 4.2, 4.6
    #[test]
    fn test_loop_with_break_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::Block {
                    instructions: vec![
                        LirInst::Loop {
                            instructions: vec![
                                LirInst::I32Const(1),
                                LirInst::BrIf(1), // break out of block
                            ],
                        },
                    ],
                },
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Loop with break should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: Nested blocks generate valid WASM
    /// Validates: Requirements 4.5
    #[test]
    fn test_nested_blocks_generate_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::Block {
                    instructions: vec![
                        LirInst::Block {
                            instructions: vec![
                                LirInst::I32Const(1),
                                LirInst::Drop,
                            ],
                        },
                    ],
                },
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Nested blocks should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    // =========================================================================
    // Multi-Function Module Tests
    // =========================================================================

    /// Test: Module with multiple functions generates valid WASM
    /// Validates: Requirements 1.1, 4.3
    #[test]
    fn test_multiple_functions_generate_valid_wasm() {
        let module = LirModule {
            functions: vec![
                create_function_with_signature(
                    "helper",
                    vec![LirType::I32],
                    vec![LirType::I32],
                    vec![],
                    vec![
                        LirInst::LocalGet(0),
                        LirInst::I32Const(1),
                        LirInst::I32Add,
                        LirInst::Return,
                    ],
                    false,
                ),
                create_function_with_signature(
                    "main",
                    vec![],
                    vec![],
                    vec![],
                    vec![
                        // Simple main that does nothing
                        // Note: We don't call helper here because function indices
                        // need to account for internal functions (alloc/free)
                        LirInst::Nop,
                    ],
                    true,
                ),
            ],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Multiple functions should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    // =========================================================================
    // Type System Integration Tests
    // =========================================================================

    /// Test: All numeric types generate valid WASM
    /// Validates: Requirements 1.2, 7.1
    #[test]
    fn test_all_numeric_types_generate_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![],
                vec![LirType::I32, LirType::I64, LirType::F32, LirType::F64],
                vec![
                    // I32 operations
                    LirInst::I32Const(42),
                    LirInst::LocalSet(0),
                    // I64 operations
                    LirInst::I64Const(100),
                    LirInst::LocalSet(1),
                    // F32 operations
                    LirInst::F32Const(3.14),
                    LirInst::LocalSet(2),
                    // F64 operations
                    LirInst::F64Const(2.718281828),
                    LirInst::LocalSet(3),
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "All numeric types should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: I64 arithmetic generates valid WASM
    /// Validates: Requirements 1.3, 7.1
    #[test]
    fn test_i64_arithmetic_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I64Const(100),
                LirInst::I64Const(200),
                LirInst::I64Add,
                LirInst::I64Const(50),
                LirInst::I64Sub,
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "I64 arithmetic should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: F64 arithmetic generates valid WASM
    /// Validates: Requirements 1.3, 7.1
    #[test]
    fn test_f64_arithmetic_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::F64Const(3.14),
                LirInst::F64Const(2.0),
                LirInst::F64Mul,
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "F64 arithmetic should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    // =========================================================================
    // Memory Operations Integration Tests
    // =========================================================================

    /// Test: Memory load/store operations generate valid WASM
    /// Validates: Requirements 3.2, 7.3, 8.3
    #[test]
    fn test_memory_operations_generate_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![],
                vec![LirType::I32],
                vec![
                    // Store a value at address 0
                    LirInst::I32Const(0),      // address
                    LirInst::I32Const(42),     // value
                    LirInst::I32Store { offset: 0, align: 2 },
                    // Load the value back
                    LirInst::I32Const(0),      // address
                    LirInst::I32Load { offset: 0, align: 2 },
                    LirInst::LocalSet(0),
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Memory operations should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: I64 memory operations generate valid WASM
    /// Validates: Requirements 3.2
    #[test]
    fn test_i64_memory_operations_generate_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![],
                vec![LirType::I64],
                vec![
                    LirInst::I32Const(0),      // address
                    LirInst::I64Const(12345678901234),
                    LirInst::I64Store { offset: 0, align: 3 },
                    LirInst::I32Const(0),
                    LirInst::I64Load { offset: 0, align: 3 },
                    LirInst::LocalSet(0),
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "I64 memory operations should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    // =========================================================================
    // Global Variables Integration Tests
    // =========================================================================

    /// Test: Global variable access generates valid WASM
    /// Validates: Requirements 1.3
    #[test]
    fn test_global_access_generates_valid_wasm() {
        // Note: This test uses GlobalGet/GlobalSet which reference globals
        // set up by the memory manager (heap pointer global)
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::GlobalGet(0),  // Get heap pointer
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Global access should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    // =========================================================================
    // Comparison Operations Integration Tests
    // =========================================================================

    /// Test: Comparison operations generate valid WASM
    /// Validates: Requirements 1.3
    #[test]
    fn test_comparison_operations_generate_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                // I32 comparisons
                LirInst::I32Const(10),
                LirInst::I32Const(20),
                LirInst::I32LtS,
                LirInst::Drop,
                // I32 equality
                LirInst::I32Const(10),
                LirInst::I32Const(10),
                LirInst::I32Eq,
                LirInst::Drop,
                // I32 inequality
                LirInst::I32Const(10),
                LirInst::I32Const(20),
                LirInst::I32Ne,
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Comparison operations should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: F64 comparison operations generate valid WASM
    /// Validates: Requirements 1.3
    #[test]
    fn test_f64_comparison_operations_generate_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::F64Const(3.14),
                LirInst::F64Const(2.71),
                LirInst::F64Eq,
                LirInst::Drop,
                LirInst::F64Const(3.14),
                LirInst::F64Const(2.71),
                LirInst::F64Ne,
                LirInst::Drop,
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "F64 comparisons should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    // =========================================================================
    // Complex Integration Tests
    // =========================================================================

    /// Test: Complex function with multiple features generates valid WASM
    /// Validates: All requirements
    #[test]
    fn test_complex_function_generates_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![LirType::I32],
                vec![LirType::I32, LirType::I32],
                vec![
                    // Initialize locals
                    LirInst::I32Const(0),
                    LirInst::LocalSet(0),  // counter = 0
                    LirInst::I32Const(0),
                    LirInst::LocalSet(1),  // sum = 0
                    // Loop to sum numbers
                    LirInst::Block {
                        instructions: vec![
                            LirInst::Loop {
                                instructions: vec![
                                    // sum += counter
                                    LirInst::LocalGet(1),
                                    LirInst::LocalGet(0),
                                    LirInst::I32Add,
                                    LirInst::LocalSet(1),
                                    // counter++
                                    LirInst::LocalGet(0),
                                    LirInst::I32Const(1),
                                    LirInst::I32Add,
                                    LirInst::LocalSet(0),
                                    // if counter >= 10, break
                                    LirInst::LocalGet(0),
                                    LirInst::I32Const(10),
                                    LirInst::I32GtS,
                                    LirInst::BrIf(1),
                                    // continue loop
                                    LirInst::Br(0),
                                ],
                            },
                        ],
                    },
                    // Return sum
                    LirInst::LocalGet(1),
                    LirInst::Return,
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Complex function should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }

    /// Test: Nested control flow generates valid WASM
    /// Validates: Requirements 4.1, 4.2, 4.5
    #[test]
    fn test_nested_control_flow_generates_valid_wasm() {
        let module = create_minimal_module(
            "main",
            vec![
                LirInst::I32Const(1),
                LirInst::If {
                    then_branch: vec![
                        LirInst::Block {
                            instructions: vec![
                                LirInst::I32Const(1),
                                LirInst::If {
                                    then_branch: vec![
                                        LirInst::I32Const(42),
                                        LirInst::Drop,
                                    ],
                                    else_branch: None,
                                },
                            ],
                        },
                    ],
                    else_branch: Some(vec![
                        LirInst::Loop {
                            instructions: vec![
                                LirInst::Br(0), // infinite loop (but we break immediately)
                            ],
                        },
                    ]),
                },
            ],
            true,
        );
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Nested control flow should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }


    // =========================================================================
    // Error Handling Integration Tests
    // =========================================================================

    /// Test: Invalid local index produces appropriate error
    /// Validates: Requirements 6.1, 6.2
    #[test]
    fn test_invalid_local_index_produces_error() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "main",
                vec![],
                vec![],
                vec![LirType::I32], // Only 1 local (index 0)
                vec![
                    LirInst::LocalGet(100), // Invalid index
                ],
                true,
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_err(), "Invalid local index should produce error");
    }

    /// Test: Module without main function still generates valid WASM
    /// Validates: Requirements 1.1
    #[test]
    fn test_module_without_main_generates_valid_wasm() {
        let module = LirModule {
            functions: vec![create_function_with_signature(
                "helper",
                vec![LirType::I32],
                vec![LirType::I32],
                vec![],
                vec![
                    LirInst::LocalGet(0),
                    LirInst::Return,
                ],
                false, // Not main
            )],
            structs: vec![],
        };
        let result = encode_wasm(&module);

        assert!(result.is_ok(), "Module without main should generate valid WASM");
        let wasm_bytes = result.unwrap();
        let validation = wasmparser::validate(&wasm_bytes);
        assert!(validation.is_ok(), "Generated WASM should pass validation");
    }
}
