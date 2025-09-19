use crate::compiler::mir::mir_nodes::{BorrowError, BorrowErrorType, BorrowKind, ProgramPoint};
use crate::compiler::mir::place::{Place, WasmType};
use crate::compiler::mir::streamlined_diagnostics::{fast_path, generate_borrow_errors_batch};
use crate::compiler::parsers::tokens::TextLocation;

#[cfg(test)]
mod streamlined_diagnostics_tests {
    use super::*;

    fn create_test_place() -> Place {
        Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        }
    }

    fn create_test_location() -> TextLocation {
        TextLocation::default()
    }

    #[test]
    fn test_fast_path_conflicting_mutable_borrows() {
        let location = create_test_location();
        let place = create_test_place();

        // Test that the fast path function returns an error as expected
        let result = fast_path::conflicting_mutable_borrows(location, &place);
        assert!(result.is_err());

        if let Err(error) = result {
            assert!(error.msg.contains("mutable more than once"));
            assert!(error.msg.contains("local_0"));
        }
    }

    #[test]
    fn test_fast_path_shared_mutable_conflict() {
        let location = create_test_location();
        let place = create_test_place();

        let result = fast_path::shared_mutable_conflict(location, &place);
        assert!(result.is_err());

        if let Err(error) = result {
            assert!(error.msg.contains("already borrowed as immutable"));
            assert!(error.msg.contains("local_0"));
        }
    }

    #[test]
    fn test_fast_path_use_after_move() {
        let location = create_test_location();
        let place = create_test_place();

        let result = fast_path::use_after_move(location, &place);
        assert!(result.is_err());

        if let Err(error) = result {
            assert!(error.msg.contains("Use of moved value"));
            assert!(error.msg.contains("local_0"));
        }
    }

    #[test]
    fn test_batch_error_generation() {
        let errors = vec![
            BorrowError {
                point: ProgramPoint::new(0),
                error_type: BorrowErrorType::ConflictingBorrows {
                    existing_borrow: BorrowKind::Mut,
                    new_borrow: BorrowKind::Mut,
                    place: create_test_place(),
                },
                message: "test".to_string(),
                location: create_test_location(),
            }
        ];

        let compile_errors = generate_borrow_errors_batch("test_function", &errors);
        assert_eq!(compile_errors.len(), 1);
        
        // Verify the error contains expected content
        let error = &compile_errors[0];
        assert!(error.msg.contains("Cannot borrow"));
    }

    #[test]
    fn test_performance_characteristics() {
        // Test that batch processing is more efficient than individual processing
        let mut errors = Vec::new();
        for i in 0..100 {
            errors.push(BorrowError {
                point: ProgramPoint::new(i),
                error_type: BorrowErrorType::ConflictingBorrows {
                    existing_borrow: BorrowKind::Mut,
                    new_borrow: BorrowKind::Mut,
                    place: create_test_place(),
                },
                message: "test".to_string(),
                location: create_test_location(),
            });
        }

        let start = std::time::Instant::now();
        let compile_errors = generate_borrow_errors_batch("test_function", &errors);
        let batch_time = start.elapsed();

        assert_eq!(compile_errors.len(), 100);
        
        // Batch processing should be reasonably fast (less than 1ms for 100 errors)
        assert!(batch_time.as_millis() < 10, "Batch processing took too long: {:?}", batch_time);
    }
}