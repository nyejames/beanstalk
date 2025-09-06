// Test modules for the Beanstalk compiler

// Essential test runner - focused validation of core functionality
#[cfg(test)]
pub mod test_runner;

// Core compiler functionality tests - essential pipeline validation
#[cfg(test)]
pub mod core_compiler_tests;

// Integration tests for end-to-end compiler testing
pub mod integration_tests;

// MIR place system tests - WASM-optimized memory management
#[cfg(test)]
pub mod place_tests;

// Borrow checking and dataflow validation tests
#[cfg(test)]
pub mod borrow_check_tests;

// Focused performance tests for key performance goals
#[cfg(test)]
pub mod focused_performance_tests;

// WASM module generation and encoding tests
#[cfg(test)]
pub mod wasm_module_tests;

// WASM-specific optimization tests (comprehensive)
#[cfg(test)]
pub mod wasm_optimization_tests;

// Legacy performance tests (comprehensive but may be overly detailed)
#[cfg(test)]
pub mod performance_tests;

// Memory layout management tests
#[cfg(test)]
pub mod memory_layout_tests;

// Interface VTable generation tests
#[cfg(test)]
pub mod interface_vtable_tests;

// WASM terminator lowering tests
#[cfg(test)]
pub mod wasm_terminator_tests;

// Comprehensive benchmark runner
#[cfg(test)]
pub mod benchmark_runner;

// Task 15 performance validation
#[cfg(test)]
pub mod performance_validation;

// Re-export the function that CLI needs
pub use integration_tests::run_all_test_cases;

// Re-export essential test functions
#[cfg(test)]
pub use test_runner::{run_essential_tests, run_performance_benchmarks, validate_wasm_optimizations};