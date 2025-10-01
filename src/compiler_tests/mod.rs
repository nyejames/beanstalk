// Test modules for the Beanstalk compiler
// Cleaned up and organized structure with redundancies removed

// === ESSENTIAL TESTS (Run First) ===
// These tests validate core functionality and should always pass

// Essential test runner - focused validation of core functionality
#[cfg(test)]
pub mod test_runner;

// Core compiler functionality tests - essential pipeline validation
#[cfg(test)]
pub mod core_compiler_tests;

// MIR place system tests - WASM-optimized memory management (comprehensive)
#[cfg(test)]
pub mod place_tests;

// Consolidated performance tests
#[cfg(test)]
pub mod consolidated_performance_tests;

// === SPECIALIZED TESTS (Run As Needed) ===
// These tests focus on specific subsystem

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

// Interface VTable generation tests
#[cfg(test)]
pub mod interface_vtable_tests;

// WASM terminator lowering tests
#[cfg(test)]
pub mod wasm_terminator_tests;

// === COMPREHENSIVE TESTS (CI/Development) ===
// These tests provide detailed analysis and may be slower

// Integration tests for end-to-end compiler testing
pub mod integration_tests;


// === RE-EXPORTS ===

// Re-export the function that CLI needs
pub use integration_tests::run_all_test_cases;

// Re-export essential test functions
#[cfg(test)]
pub use test_runner::run_essential_tests;

// Re-export consolidated performance functions
#[cfg(test)]
pub use consolidated_performance_tests::{run_performance_benchmarks, validate_wasm_optimizations};
