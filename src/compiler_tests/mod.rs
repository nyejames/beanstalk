// Test modules for the Beanstalk compiler
// Updated structure after removing redundant and outdated tests

// === ESSENTIAL TESTS (Run First) ===
// These tests validate core functionality and should always pass

// Essential test runner - focused validation of core functionality
#[cfg(test)]
pub mod test_runner;

// Core compiler functionality tests - essential pipeline validation
#[cfg(test)]
pub mod core_compiler_tests;

// MIR place system tests - WASM-optimized memory management
#[cfg(test)]
pub mod place_tests;

// Focused performance tests for key performance goals
#[cfg(test)]
pub mod focused_performance_tests;

// === SPECIALIZED TESTS (Run As Needed) ===
// These tests focus on specific subsystems

// Simplified borrow checking behavior tests
#[cfg(test)]
pub mod borrow_check_tests;

// Streamlined diagnostics tests
#[cfg(test)]
pub mod streamlined_diagnostics_tests;

// WASM module generation and encoding tests
#[cfg(test)]
pub mod wasm_module_tests;

// WASM module finish() method tests
#[cfg(test)]
pub mod wasm_module_finish_tests;

// WASM validation and error reporting tests
#[cfg(test)]
pub mod wasm_validation_tests;

// Comprehensive WASM codegen tests (Task 27)
#[cfg(test)]
pub mod wasm_codegen_tests;

// Memory layout management tests
#[cfg(test)]
pub mod memory_layout_tests;

// Lifetime-optimized memory management tests
#[cfg(test)]
pub mod lifetime_memory_manager_tests;

// Interface VTable generation tests
#[cfg(test)]
pub mod interface_vtable_tests;

// WASM terminator lowering tests
#[cfg(test)]
pub mod wasm_terminator_tests;

// Memory operation lowering tests
#[cfg(test)]
pub mod memory_operation_tests;

// === COMPREHENSIVE TESTS (CI/Development) ===
// These tests provide detailed analysis and may be slower

// Integration tests for end-to-end compiler testing
pub mod integration_tests;

// Comprehensive performance tests (more detailed than focused_performance_tests)
#[cfg(test)]
pub mod performance_tests;

// WASM-specific optimization tests (comprehensive)
#[cfg(test)]
pub mod wasm_optimization_tests;

// WASM performance and validation tests (Task 28)
#[cfg(test)]
pub mod wasm_performance_validation_tests;

// Comprehensive benchmark runner
#[cfg(test)]
pub mod benchmark_runner;

// Task 15 performance validation
#[cfg(test)]
pub mod performance_validation;

// === TEST VALIDATION ===
// Module to validate that tests are properly organized and working

#[cfg(test)]
pub mod test_validation;

// === RE-EXPORTS ===

// Re-export the function that CLI needs
pub use integration_tests::run_all_test_cases;

// Re-export essential test functions
#[cfg(test)]
pub use test_runner::{
    run_essential_tests, run_performance_benchmarks, validate_wasm_optimizations,
};
