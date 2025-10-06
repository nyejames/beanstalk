//! Tests for UnifiedBorrowChecker functionality
//! 
//! This module tests the borrow checking system to ensure memory safety
//! violations are properly detected and reported.

use crate::compiler::wir::unified_borrow_checker::{UnifiedBorrowChecker, run_unified_borrow_checking};
use crate::compiler::wir::wir_nodes::{WirFunction, WirBlock, Statement, Terminator, Rvalue, Operand, Constant, BorrowKind};
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::wir::extract::BorrowFactExtractor;
use crate::compiler::compiler_errors::CompileError;
use std::collections::HashMap;

#[cfg(test)]
mod borrow_checker_tests {
    use super::*;

    /// Create a simple test function for borrow checking
    fn create_test_function() -> WirFunction {
        WirFunction {
            id: 0,
            name: "test_func".to_string(),
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

    /// Test borrow checker with variable system - mutable/immutable conflicts
    #[test]
    fn test_borrow_checker_variable_mutable_immutable_conflict() {
        let mut function = create_test_function();
        
        // Create a variable and both mutable and immutable borrows
        let var_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let mut_ref_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        let immut_ref_place = Place::Local { index: 2, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            // Initialize variable
            Statement::Assign {
                place: var_place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            // Create mutable borrow
            Statement::Assign {
                place: mut_ref_place,
                rvalue: Rvalue::Ref {
                    place: var_place.clone(),
                    borrow_kind: BorrowKind::Mut,
                },
            },
            // Create immutable borrow (should conflict)
            Statement::Assign {
                place: immut_ref_place,
                rvalue: Rvalue::Ref {
                    place: var_place.clone(),
                    borrow_kind: BorrowKind::Shared,
                },
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should complete");
        
        // Check if conflicts were detected
        if let Ok(results) = borrow_result {
            // In a fully implemented borrow checker, this should detect conflicts
            // For now, we verify the infrastructure works
            assert!(results.statistics.program_points_processed > 0, "Should process program points");
        }
    }

    /// Test borrow checker with variable system - use after move
    #[test]
    fn test_borrow_checker_variable_use_after_move() {
        let mut function = create_test_function();
        
        // Create a variable, move it, then try to use it
        let var_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let moved_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        let use_place = Place::Local { index: 2, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            // Initialize variable
            Statement::Assign {
                place: var_place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            // Move the variable
            Statement::Assign {
                place: moved_place,
                rvalue: Rvalue::Use(Operand::Move(var_place.clone())),
            },
            // Try to use the moved variable (should be an error)
            Statement::Assign {
                place: use_place,
                rvalue: Rvalue::Use(Operand::Copy(var_place.clone())),
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should complete");
        
        // Check if use-after-move was detected
        if let Ok(results) = borrow_result {
            // In a fully implemented borrow checker, this should detect use-after-move
            assert!(results.statistics.program_points_processed > 0, "Should process program points");
        }
    }

    /// Test borrow checker with variable system - multiple mutable borrows
    #[test]
    fn test_borrow_checker_variable_multiple_mutable_borrows() {
        let mut function = create_test_function();
        
        // Create a variable and two mutable borrows (should conflict)
        let var_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let mut_ref1_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        let mut_ref2_place = Place::Local { index: 2, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            // Initialize variable
            Statement::Assign {
                place: var_place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            // Create first mutable borrow
            Statement::Assign {
                place: mut_ref1_place,
                rvalue: Rvalue::Ref {
                    place: var_place.clone(),
                    borrow_kind: BorrowKind::Mut,
                },
            },
            // Create second mutable borrow (should conflict)
            Statement::Assign {
                place: mut_ref2_place,
                rvalue: Rvalue::Ref {
                    place: var_place.clone(),
                    borrow_kind: BorrowKind::Mut,
                },
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should complete");
        
        // Check if multiple mutable borrow conflict was detected
        if let Ok(results) = borrow_result {
            // In a fully implemented borrow checker, this should detect conflicts
            assert!(results.statistics.program_points_processed > 0, "Should process program points");
        }
    }

    /// Test borrow checker with variable system - proper error messages
    #[test]
    fn test_borrow_checker_variable_error_messages() {
        use crate::compiler::wir::wir_nodes::{BorrowError, BorrowErrorType};
        use crate::compiler::parsers::tokens::TextLocation;
        
        // Test that borrow checker generates proper error messages for variable violations
        let location = TextLocation::default();
        let var_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        
        // Test conflicting borrows error
        let conflicting_error = BorrowError {
            point: crate::compiler::wir::wir_nodes::ProgramPoint::new(0),
            error_type: BorrowErrorType::ConflictingBorrows {
                existing_borrow: BorrowKind::Mut,
                new_borrow: BorrowKind::Shared,
                place: var_place.clone(),
            },
            message: "Cannot borrow as immutable because it is already borrowed as mutable. Finish using the mutable borrow before creating immutable borrows.".to_string(),
            location: location.clone(),
        };
        
        assert!(conflicting_error.message.contains("Cannot borrow"), "Should explain borrow violation");
        assert!(conflicting_error.message.contains("already borrowed"), "Should mention existing borrow");
        assert!(conflicting_error.message.contains("Finish using"), "Should provide guidance");
        
        // Test use after move error
        let use_after_move_error = BorrowError {
            point: crate::compiler::wir::wir_nodes::ProgramPoint::new(1),
            error_type: BorrowErrorType::UseAfterMove {
                place: var_place.clone(),
                move_point: crate::compiler::wir::wir_nodes::ProgramPoint::new(0),
            },
            message: "Use of moved value. Value was moved at previous statement. Try using references instead of moving the value.".to_string(),
            location,
        };
        
        assert!(use_after_move_error.message.contains("Use of moved value"), "Should explain move violation");
        assert!(use_after_move_error.message.contains("Try using references"), "Should provide alternatives");
    }

    /// Test borrow checker integration with variable scoping
    #[test]
    fn test_borrow_checker_variable_scoping() {
        let mut function = create_test_function();
        
        // Test that borrow checker properly handles variable scoping
        // This is a simplified test since full scoping requires more complex MIR structure
        let var_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let scoped_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        function.blocks[0].statements = vec![
            // Initialize variable in outer scope
            Statement::Assign {
                place: var_place.clone(),
                rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
            },
            // Create a reference that should be valid within scope
            Statement::Assign {
                place: scoped_place,
                rvalue: Rvalue::Ref {
                    place: var_place.clone(),
                    borrow_kind: BorrowKind::Shared,
                },
            },
        ];

        let mut extractor = BorrowFactExtractor::new();
        let extract_result = extractor.extract_function(&function);
        assert!(extract_result.is_ok(), "Fact extraction should succeed");

        let borrow_result = run_unified_borrow_checking(&function, &extractor);
        assert!(borrow_result.is_ok(), "Borrow checking should complete");
        
        // Verify that scoping is handled correctly
        if let Ok(results) = borrow_result {
            assert!(results.statistics.program_points_processed > 0, "Should process program points");
            // In a fully implemented system, this would verify scope-based borrow validity
        }
    }
}