//! Tests for UnifiedBorrowChecker functionality
//! 
//! This module tests the borrow checking system to ensure memory safety
//! violations are properly detected and reported.

use crate::compiler::mir::unified_borrow_checker::{UnifiedBorrowChecker, run_unified_borrow_checking};
use crate::compiler::mir::mir_nodes::{MirFunction, MirBlock, Statement, Terminator, Rvalue, Operand, Constant, BorrowKind};
use crate::compiler::mir::place::{Place, WasmType};
use crate::compiler::mir::extract::BorrowFactExtractor;
use crate::compiler::compiler_errors::CompileError;
use std::collections::HashMap;

#[cfg(test)]
mod borrow_checker_tests {
    use super::*;

    /// Create a simple test function for borrow checking
    fn create_test_function() -> MirFunction {
        MirFunction {
            id: 0,
            name: "test_func".to_string(),
            parameters: vec![],
            return_types: vec![],
            blocks: vec![MirBlock {
                id: 0,
                statements: vec![Statement::Nop],
                terminator: Terminator::Return { values: vec![] },
            }],
            locals: HashMap::new(),
            signature: crate::compiler::mir::mir_nodes::FunctionSignature {
                params: vec![],
                returns: vec![],
            },
            events: HashMap::new(),
        }
    }

    /// Test basic borrow checker initialization
    #[test]
    fn test_borrow_checker_initialization() {
        let checker = UnifiedBorrowChecker::new();
        
        // Verify initial state
        assert_eq!(checker.loans.len(), 0, "New borrow checker should have no loans");
        assert_eq!(checker.errors.len(), 0, "New borrow checker should have no errors");
    }

    /// Test borrow fact extraction
    #[test]
    fn test_borrow_fact_extraction() {
        let function = create_test_function();
        let mut extractor = BorrowFactExtractor::new();
        
        let result = extractor.extract_function(&function);
        assert!(result.is_ok(), "Fact extraction should succeed for simple function");
    }

    /// Test simple borrow checking on valid code
    #[test]
    fn test_valid_borrow_checking() {
        let mut function = create_test_function();
        
        // Add a simple assignment that should be valid
        let place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        function.blocks[0].statements = vec![
            Statement::Assign {
                place: place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            }
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should succeed for valid code");
        
        if let Ok(results) = borrow_result {
            assert_eq!(results.errors.len(), 0, "Valid code should have no borrow errors");
        }
    }

    /// Test borrow checking with reference creation
    #[test]
    fn test_reference_creation() {
        let mut function = create_test_function();
        
        // Create a reference to a local variable
        let place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let ref_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            Statement::Assign {
                place: place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            Statement::Assign {
                place: ref_place,
                rvalue: Rvalue::Ref {
                    place: place.clone(),
                    borrow_kind: BorrowKind::Shared,
                },
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should succeed for reference creation");
    }

    /// Test borrow checking with move semantics
    #[test]
    fn test_move_semantics() {
        let mut function = create_test_function();
        
        let place_a = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_b = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            Statement::Assign {
                place: place_a.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            Statement::Assign {
                place: place_b,
                rvalue: Rvalue::Use(Operand::Move(place_a.clone())),
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should succeed for move semantics");
    }

    /// Test borrow checking with copy semantics
    #[test]
    fn test_copy_semantics() {
        let mut function = create_test_function();
        
        let place_a = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_b = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            Statement::Assign {
                place: place_a.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            Statement::Assign {
                place: place_b,
                rvalue: Rvalue::Use(Operand::Copy(place_a.clone())),
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should succeed for copy semantics");
    }

    /// Test borrow checker error detection
    #[test]
    fn test_borrow_error_detection() {
        // This test would check for actual borrow violations once the borrow checker
        // is fully implemented. For now, we test that the infrastructure works.
        
        let function = create_test_function();
        let mut extractor = BorrowFactExtractor::new();
        
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should work");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should complete");
    }

    /// Test borrow checker integration with MIR events
    #[test]
    fn test_borrow_checker_events_integration() {
        let mut function = create_test_function();
        
        // Add some statements that generate events
        let place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        function.blocks[0].statements = vec![
            Statement::Assign {
                place: place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
        ];

        // Generate events for the function
        function.generate_events_for_all_blocks();
        
        // Verify events were generated
        assert!(!function.events.is_empty(), "Function should have generated events");

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should work with events");
    }
}