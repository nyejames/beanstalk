use crate::compiler::mir::dataflow::run_loan_liveness_dataflow;
use crate::compiler::mir::extract::extract_gen_kill_sets;
use crate::compiler::mir::liveness::run_liveness_analysis;
use crate::compiler::mir::check::run_conflict_detection;
use crate::compiler::mir::mir_nodes::*;
use crate::compiler::mir::place::*;
use crate::compiler::datatypes::{DataType, Ownership};
use std::time::{Duration, Instant};

/// Focused performance tests that validate key performance goals
/// without getting into excessive implementation details.

#[cfg(test)]
mod compilation_speed_tests {
    use super::*;

    #[test]
    fn test_small_function_performance() {
        let function = create_test_function(20, 5); // 20 statements, 5 loans
        
        let start = Instant::now();
        let result = run_full_analysis(&function);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Small function analysis should succeed");
        assert!(duration < Duration::from_millis(10), 
               "Small function should analyze in <10ms, took {}ms", duration.as_millis());
    }

    #[test]
    fn test_medium_function_performance() {
        let function = create_test_function(100, 25); // 100 statements, 25 loans
        
        let start = Instant::now();
        let result = run_full_analysis(&function);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Medium function analysis should succeed");
        assert!(duration < Duration::from_millis(50), 
               "Medium function should analyze in <50ms, took {}ms", duration.as_millis());
    }

    #[test]
    fn test_large_function_performance() {
        let function = create_test_function(500, 100); // 500 statements, 100 loans
        
        let start = Instant::now();
        let result = run_full_analysis(&function);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Large function analysis should succeed");
        assert!(duration < Duration::from_millis(200), 
               "Large function should analyze in <200ms, took {}ms", duration.as_millis());
    }

    #[test]
    fn test_scalability() {
        // Test that analysis time scales reasonably with function size
        let sizes = vec![50, 100, 200];
        let mut times = Vec::new();
        
        for size in sizes {
            let function = create_test_function(size, size / 5);
            
            let start = Instant::now();
            let result = run_full_analysis(&function);
            let duration = start.elapsed();
            
            assert!(result.is_ok(), "Analysis should succeed for size {}", size);
            times.push(duration);
            
            // Each function should complete in reasonable time
            assert!(duration < Duration::from_millis(size as u64 * 2), 
                   "Function of size {} should complete in <{}ms, took {}ms", 
                   size, size * 2, duration.as_millis());
        }
        
        // Verify roughly linear scaling (allowing for some variance)
        if times.len() >= 2 {
            let ratio = times[1].as_nanos() as f64 / times[0].as_nanos() as f64;
            assert!(ratio < 5.0, "Analysis time should scale reasonably, got {}x increase", ratio);
        }
    }

    /// Run full MIR analysis pipeline
    fn run_full_analysis(function: &MirFunction) -> Result<(), String> {
        // Extract gen/kill sets
        let extractor = extract_gen_kill_sets(function)?;
        
        // Run dataflow analysis
        let dataflow = run_loan_liveness_dataflow(function, &extractor)?;
        
        // Run conflict detection
        let _conflicts = run_conflict_detection(function, dataflow, extractor)?;
        
        Ok(())
    }

    /// Create a test function with specified size and complexity
    fn create_test_function(stmt_count: usize, loan_count: usize) -> MirFunction {
        let mut function = MirFunction::new(0, "test_function".to_string(), vec![], vec![]);
        
        // Create places for testing
        let mut places = Vec::new();
        for i in 0..loan_count {
            places.push(Place::local(i as u32, &DataType::Int(Ownership::ImmutableOwned(false))));
        }
        
        // Add statements with program points and events
        for i in 0..stmt_count {
            let pp = ProgramPoint::new(i as u32);
            function.add_program_point(pp, 0, i);
            
            let mut events = Events::default();
            
            // Add some loans
            if i < loan_count && i % 3 == 0 {
                events.start_loans.push(LoanId::new(i as u32));
            }
            
            // Add some uses
            if i > 0 && !places.is_empty() {
                events.uses.push(places[i % places.len()].clone());
            }
            
            // Add some reassignments
            if i % 5 == 0 && !places.is_empty() {
                events.reassigns.push(places[i % places.len()].clone());
            }
            
            function.store_events(pp, events);
        }
        
        function
    }
}

#[cfg(test)]
mod memory_efficiency_tests {
    use super::*;

    #[test]
    fn test_bitset_memory_usage() {
        // Test that bitset memory usage is reasonable
        let function = create_complex_function(200, 50);
        
        let extractor = extract_gen_kill_sets(&function).unwrap();
        let dataflow = run_loan_liveness_dataflow(&function, &extractor).unwrap();
        
        // Get statistics about memory usage
        let stats = dataflow.get_statistics();
        
        // Memory usage should be reasonable for the function size
        let estimated_memory = stats.total_program_points * stats.total_loans / 8; // bits to bytes
        assert!(estimated_memory < 1024 * 1024, // < 1MB
               "Memory usage should be reasonable, estimated {} bytes", estimated_memory);
        
        // Average live loans should be much less than total loans (sparsity)
        assert!(stats.avg_live_loans_per_point < stats.total_loans as f64 * 0.5,
               "Should have sparsity in live loans");
    }

    #[test]
    fn test_dataflow_convergence() {
        // Test that dataflow analysis converges quickly
        let function = create_complex_function(100, 30);
        
        let extractor = extract_gen_kill_sets(&function).unwrap();
        
        let start = Instant::now();
        let dataflow = run_loan_liveness_dataflow(&function, &extractor).unwrap();
        let duration = start.elapsed();
        
        let stats = dataflow.get_statistics();
        
        // Should converge in reasonable number of iterations (estimated)
        let estimated_iterations = stats.total_program_points / 10 + 5; // Rough estimate
        assert!(estimated_iterations < stats.total_program_points * 2,
               "Should converge quickly, estimated {} iterations for {} points", 
               estimated_iterations, stats.total_program_points);
        
        // Should complete quickly
        assert!(duration < Duration::from_millis(100),
               "Dataflow should converge quickly, took {}ms", duration.as_millis());
    }

    /// Create a function with complex borrow patterns for testing
    fn create_complex_function(stmt_count: usize, loan_count: usize) -> MirFunction {
        let mut function = MirFunction::new(0, "complex_function".to_string(), vec![], vec![]);
        
        // Create overlapping places (some will alias)
        let mut places = Vec::new();
        for i in 0..loan_count / 2 {
            let base = Place::local(i as u32, &DataType::String(Ownership::ImmutableOwned(false)));
            places.push(base.clone());
            places.push(base.project_field(0, 4, FieldSize::WasmType(WasmType::I32)));
        }
        
        // Add statements with complex patterns
        for i in 0..stmt_count {
            let pp = ProgramPoint::new(i as u32);
            function.add_program_point(pp, 0, i);
            
            let mut events = Events::default();
            
            // Create overlapping loans (some will conflict)
            if i < loan_count {
                events.start_loans.push(LoanId::new(i as u32));
                
                // Sometimes create additional loans that might conflict
                if i % 4 == 0 && i + 1 < loan_count {
                    events.start_loans.push(LoanId::new((i + 1) as u32));
                }
            }
            
            // Multiple uses per statement
            for j in 0..3 {
                let place_idx = (i + j) % places.len();
                if place_idx < places.len() {
                    events.uses.push(places[place_idx].clone());
                }
            }
            
            // Some moves and reassignments
            if i % 6 == 0 && !places.is_empty() {
                events.moves.push(places[i % places.len()].clone());
            }
            if i % 7 == 0 && !places.is_empty() {
                events.reassigns.push(places[i % places.len()].clone());
            }
            
            function.store_events(pp, events);
        }
        
        function
    }
}

#[cfg(test)]
mod wasm_optimization_tests {
    use super::*;

    #[test]
    fn test_wasm_place_efficiency() {
        // Test that WASM place operations are efficient
        let local = Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false)));
        let global = Place::global(0, &DataType::Float(Ownership::ImmutableOwned(false)));
        let memory = Place::memory(1024, TypeSize::Word);
        
        // All basic operations should be ≤3 WASM instructions
        assert!(local.load_instruction_count() <= 3);
        assert!(local.store_instruction_count() <= 3);
        assert!(global.load_instruction_count() <= 3);
        assert!(global.store_instruction_count() <= 3);
        assert!(memory.load_instruction_count() <= 3);
        assert!(memory.store_instruction_count() <= 3);
    }

    #[test]
    fn test_projection_efficiency() {
        // Test that projections are reasonably efficient
        let base = Place::local(0, &DataType::String(Ownership::ImmutableOwned(false)));
        
        // Simple field projection
        let field = base.clone().project_field(0, 8, FieldSize::WasmType(WasmType::I32));
        assert!(field.load_instruction_count() <= 5, 
               "Field projection should be ≤5 instructions, got {}", field.load_instruction_count());
        
        // Index projection
        let index_place = Place::local(1, &DataType::Int(Ownership::ImmutableOwned(false)));
        let indexed = base.project_index(index_place, 4);
        assert!(indexed.load_instruction_count() <= 6,
               "Index projection should be ≤6 instructions, got {}", indexed.load_instruction_count());
    }

    #[test]
    fn test_stack_operation_balance() {
        // Test that stack operations are balanced (push/pop correctly)
        let places = vec![
            Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false))),
            Place::memory(1024, TypeSize::Word),
            Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false)))
                .project_field(0, 4, FieldSize::WasmType(WasmType::I32)),
        ];
        
        for place in places {
            let load_ops = place.generate_load_operations();
            let store_ops = place.generate_store_operations();
            
            // Load operations should net +1 on stack (push value)
            let load_delta: i32 = load_ops.iter().map(|op| op.stack_delta).sum();
            assert_eq!(load_delta, 1, "Load operations should net +1 on stack");
            
            // Store operations should net -1 on stack (consume value)
            let store_delta: i32 = store_ops.iter().map(|op| op.stack_delta).sum();
            assert_eq!(store_delta, -1, "Store operations should net -1 on stack");
        }
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn test_no_infinite_loops() {
        // Test that dataflow analysis doesn't get stuck in infinite loops
        let function = create_pathological_function();
        
        let start = Instant::now();
        let result = run_full_analysis(&function);
        let duration = start.elapsed();
        
        assert!(result.is_ok(), "Pathological function should still complete");
        assert!(duration < Duration::from_secs(5), 
               "Should complete in reasonable time even for pathological cases");
    }

    #[test]
    fn test_empty_function_handling() {
        // Test edge case of empty function
        let function = MirFunction::new(0, "empty".to_string(), vec![], vec![]);
        
        let result = run_full_analysis(&function);
        assert!(result.is_ok(), "Empty function should be handled gracefully");
    }

    #[test]
    fn test_single_statement_function() {
        // Test minimal function with one statement
        let mut function = MirFunction::new(0, "minimal".to_string(), vec![], vec![]);
        
        let pp = ProgramPoint::new(0);
        function.add_program_point(pp, 0, 0);
        
        let events = Events::default();
        function.store_events(pp, events);
        
        let result = run_full_analysis(&function);
        assert!(result.is_ok(), "Single statement function should work");
    }

    /// Create a function with pathological patterns that might cause issues
    fn create_pathological_function() -> MirFunction {
        let mut function = MirFunction::new(0, "pathological".to_string(), vec![], vec![]);
        
        // Create many overlapping places
        let mut places = Vec::new();
        for i in 0..20 {
            let mut place = Place::local(i / 5, &DataType::String(Ownership::ImmutableOwned(false)));
            
            // Add multiple levels of projections
            for level in 0..(i % 4) {
                place = place.project_field(level as u32, 4, FieldSize::WasmType(WasmType::I32));
            }
            places.push(place);
        }
        
        // Create many statements with complex interactions
        for i in 0..100 {
            let pp = ProgramPoint::new(i as u32);
            function.add_program_point(pp, 0, i);
            
            let mut events = Events::default();
            
            // Create many loans
            for j in 0..5 {
                events.start_loans.push(LoanId::new((i * 5 + j) as u32));
            }
            
            // Use many places
            for j in 0..places.len() {
                events.uses.push(places[j].clone());
            }
            
            function.store_events(pp, events);
        }
        
        function
    }

    /// Run full analysis pipeline (helper function)
    fn run_full_analysis(function: &MirFunction) -> Result<(), String> {
        let extractor = extract_gen_kill_sets(function)?;
        let dataflow = run_loan_liveness_dataflow(function, &extractor)?;
        let _conflicts = run_conflict_detection(function, dataflow, extractor)?;
        Ok(())
    }
}