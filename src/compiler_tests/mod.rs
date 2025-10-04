// Test modules for the Beanstalk compiler
// Cleaned up and organized structure with redundancies removed

// === ESSENTIAL TESTS (Run First) ===
// These tests validate core functionality and should always pass

// Essential test runner - focused validation of core functionality
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

// Optimizer tests (moved to separate directory)
#[cfg(test)]
pub mod optimizer_tests;

// WASM module generation and encoding tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod wasm_module_tests;

// WASM validation and error reporting tests
#[cfg(test)]
pub mod wasm_validation_tests;

// Comprehensive WASM codegen tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod wasm_codegen_tests;

// Memory layout management tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod memory_layout_tests;

// Interface VTable generation tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod interface_vtable_tests;

// WASM terminator lowering tests
// Temporarily disabled due to API changes
// #[cfg(test)]
// pub mod wasm_terminator_tests;

// WASM encoder module builder API tests
#[cfg(test)]
pub mod wasm_encoder_module_tests;

// WASM encoder API discovery tests
#[cfg(test)]
pub mod wasm_encoder_api_discovery;

// WASM encoder Function builder API tests
#[cfg(test)]
pub mod wasm_encoder_function_tests;

// WASM encoder Type system integration tests
#[cfg(test)]
pub mod wasm_encoder_type_tests;

// WASM encoder error handling tests
#[cfg(test)]
pub mod wasm_encoder_error_handling_tests;

// Simple error validation test
#[cfg(test)]
pub mod simple_error_validation_test;

// === FOCUSED TEST CATEGORIES ===
// Organized tests for specific functionality

// MIR construction tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod mir_construction_tests;

// Borrow checker tests - DISABLED during backend simplification
// #[cfg(test)]
// pub mod borrow_checker_tests;

// Error handling tests - Error message validation
#[cfg(test)]
pub mod error_handling_tests;

// Host function system tests - Registry, AST parsing, and MIR lowering
#[cfg(test)]
pub mod host_function_tests;

// === COMPREHENSIVE TESTS (CI/Development) ===
// These tests provide detailed analysis and may be slower

// Integration tests for end-to-end compiler testing
#[cfg(test)]
pub mod integration_tests;

// WASM execution tests for runtime validation
#[cfg(test)]
pub mod wasm_execution_tests;


// === RE-EXPORTS ===

// Re-export functions that CLI needs
pub use test_runner::run_all_test_cases;

// Re-export essential test functions
pub use test_runner::run_essential_tests;

// Re-export consolidated performance functions
#[cfg(test)]
pub use consolidated_performance_tests::{run_performance_benchmarks, validate_wasm_optimizations};

// Re-export host function test functions
#[cfg(test)]
pub use host_function_tests::run_host_function_tests;
