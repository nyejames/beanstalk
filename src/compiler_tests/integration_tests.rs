//! End-to-end pipeline integration tests
//! 
//! This module tests the complete compilation pipeline components.

use crate::compiler::mir::mir_nodes::MIR;
use crate::compiler::codegen::build_wasm::new_wasm_module;

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test basic MIR to WASM pipeline
    #[test]
    fn test_basic_mir_to_wasm_pipeline() {
        let mir = MIR::new();
        
        // Generate WASM from empty MIR
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "Empty MIR should generate WASM successfully");
        
        let wasm_bytes = wasm_result.unwrap();
        assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
    }

    /// Test complete pipeline with simple variable declaration
    #[test]
    fn test_simple_variable_pipeline() {
        let source = "let x = 42;";
        
        // Parse to AST
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Simple variable declaration should parse: {:?}", ast_result.err());
        
        let ast = ast_result.unwrap();
        
        // Transform to MIR
        let mir_result = build_mir_from_ast(ast);
        assert!(mir_result.is_ok(), "Variable declaration should transform to MIR: {:?}", mir_result.err());
        
        let mir = mir_result.unwrap();
        
        // Generate WASM
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "Variable declaration MIR should generate WASM: {:?}", wasm_result.err());
    }

    /// Test complete pipeline with simple function
    #[test]
    fn test_simple_function_pipeline() {
        let source = r#"
            function test_func() -> Int:
                return 42
            ;
        "#;
        
        // Parse to AST
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Simple function should parse: {:?}", ast_result.err());
        
        let ast = ast_result.unwrap();
        
        // Transform to MIR
        let mir_result = build_mir_from_ast(ast);
        assert!(mir_result.is_ok(), "Function should transform to MIR: {:?}", mir_result.err());
        
        let mir = mir_result.unwrap();
        
        // Generate WASM
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "Function MIR should generate WASM: {:?}", wasm_result.err());
    }

    /// Test complete pipeline with arithmetic operations
    #[test]
    fn test_arithmetic_pipeline() {
        let source = r#"
            let a = 10;
            let b = 20;
            let result = a + b;
        "#;
        
        // Parse to AST
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Arithmetic operations should parse: {:?}", ast_result.err());
        
        let ast = ast_result.unwrap();
        
        // Transform to MIR
        let mir_result = build_mir_from_ast(ast);
        assert!(mir_result.is_ok(), "Arithmetic should transform to MIR: {:?}", mir_result.err());
        
        let mir = mir_result.unwrap();
        
        // Generate WASM
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "Arithmetic MIR should generate WASM: {:?}", wasm_result.err());
    }

    /// Test complete pipeline with control flow
    #[test]
    fn test_control_flow_pipeline() {
        let source = r#"
            let x = 10;
            if x > 5:
                let y = 20
            else
                let y = 30
            ;
        "#;
        
        // Parse to AST
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Control flow should parse: {:?}", ast_result.err());
        
        let ast = ast_result.unwrap();
        
        // Transform to MIR
        let mir_result = build_mir_from_ast(ast);
        assert!(mir_result.is_ok(), "Control flow should transform to MIR: {:?}", mir_result.err());
        
        let mir = mir_result.unwrap();
        
        // Generate WASM
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "Control flow MIR should generate WASM: {:?}", wasm_result.err());
    }

    /// Test pipeline error propagation
    #[test]
    fn test_error_propagation() {
        let source = "invalid syntax here !!!";
        
        // Parse to AST - should fail
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_err(), "Invalid syntax should fail to parse");
        
        // Verify error type
        if let Err(error) = ast_result {
            match error.error_type {
                crate::compiler::compiler_errors::ErrorType::Syntax => {
                    // Expected syntax error
                }
                _ => panic!("Expected syntax error, got: {:?}", error.error_type),
            }
        }
    }

    /// Test WASM validation in pipeline
    #[test]
    fn test_wasm_validation_in_pipeline() {
        let source = "let x = 42;";
        
        // Complete pipeline
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Source should parse");
        
        let mir_result = build_mir_from_ast(ast_result.unwrap());
        assert!(mir_result.is_ok(), "AST should transform to MIR");
        
        let wasm_result = new_wasm_module(mir_result.unwrap());
        assert!(wasm_result.is_ok(), "MIR should generate WASM");
        
        // Validate generated WASM
        let wasm_bytes = wasm_result.unwrap();
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM should be valid: {:?}", validation_result.err());
    }

    /// Test pipeline with memory operations
    #[test]
    fn test_memory_operations_pipeline() {
        let source = r#"
            let data = "Hello, World!";
        "#;
        
        // Parse to AST
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "String literal should parse: {:?}", ast_result.err());
        
        let ast = ast_result.unwrap();
        
        // Transform to MIR
        let mir_result = build_mir_from_ast(ast);
        assert!(mir_result.is_ok(), "String literal should transform to MIR: {:?}", mir_result.err());
        
        let mir = mir_result.unwrap();
        
        // Verify memory configuration
        assert!(mir.type_info.memory_info.initial_pages > 0, "String literal should require memory");
        
        // Generate WASM
        let wasm_result = new_wasm_module(mir);
        assert!(wasm_result.is_ok(), "String literal MIR should generate WASM: {:?}", wasm_result.err());
    }

    /// Test pipeline performance with reasonable compilation time
    #[test]
    fn test_compilation_performance() {
        let source = r#"
            function fibonacci(n Int) -> Int:
                if n <= 1:
                    return n
                else
                    return fibonacci(n - 1) + fibonacci(n - 2)
                ;
            ;
            
            let result = fibonacci(10);
        "#;
        
        let start_time = std::time::Instant::now();
        
        // Complete pipeline
        let ast_result = build_ast_from_source(source, "test.bst");
        assert!(ast_result.is_ok(), "Recursive function should parse");
        
        let mir_result = build_mir_from_ast(ast_result.unwrap());
        assert!(mir_result.is_ok(), "Recursive function should transform to MIR");
        
        let wasm_result = new_wasm_module(mir_result.unwrap());
        assert!(wasm_result.is_ok(), "Recursive function should generate WASM");
        
        let compilation_time = start_time.elapsed();
        
        // Compilation should complete in reasonable time (less than 5 seconds as per requirements)
        assert!(compilation_time.as_secs() < 5, "Compilation should complete in under 5 seconds, took: {:?}", compilation_time);
    }
}