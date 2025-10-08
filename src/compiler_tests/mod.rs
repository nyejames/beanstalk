// Test modules for the Beanstalk compiler
// Cleaned up and organized structure with redundancies removed

// === ESSENTIAL TESTS (Run First) ===
// These tests validate core functionality and should always pass

// Essential test runner - focused validation of core functionality
pub mod test_runner;

// Core compiler functionality tests - essential pipeline validation
#[cfg(test)]
pub mod core_compiler_tests;

// WIR place system tests - WASM-optimized memory management (comprehensive)
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

// WASM module generation and encoding tests - REMOVED (outdated API usage)

// WASM validation and error reporting tests
#[cfg(test)]
pub mod wasm_validation_tests;

// Comprehensive WASM codegen tests - REMOVED (outdated API usage)

// Memory layout management tests - REMOVED (outdated API usage)

// Interface VTable generation tests - REMOVED (outdated API usage)

// WASM terminator lowering tests - REMOVED (outdated API usage)

// WASM encoder module builder API tests
#[cfg(test)]
pub mod wasm_encoder_module_tests;

// WASM encoder API discovery tests - REMOVED (no longer needed)

// WASM encoder Function builder API tests
#[cfg(test)]
pub mod wasm_encoder_function_tests;

// WASM encoder Type system integration tests
#[cfg(test)]
pub mod wasm_encoder_type_tests;

// WASM encoder error handling tests
#[cfg(test)]
pub mod wasm_encoder_error_handling_tests;

// WASM encoder integration tests - Task 4.4: Comprehensive integration testing
#[cfg(test)]
pub mod wasm_encoder_integration_tests;

// Beanstalk language compliance tests - Tests for language-specific WASM generation
#[cfg(test)]
pub mod beanstalk_language_compliance_tests;

// Simple error validation test
#[cfg(test)]
pub mod simple_error_validation_test;

// WASM module generation tests - Basic WASM module creation and validation
#[cfg(test)]
pub mod wasm_module_generation_tests;

// === FOCUSED TEST CATEGORIES ===
// Organized tests for specific functionality

// WIR construction tests - REMOVED (outdated API usage)

// Borrow checker tests - REMOVED (outdated API usage)

// Error handling tests - Error message validation
#[cfg(test)]
pub mod error_handling_tests;

// Host function system tests - Registry, AST parsing, and WIR lowering
#[cfg(test)]
pub mod host_function_tests;

// WASI print functionality tests - End-to-end WASI integration testing
#[cfg(test)]
pub mod wasi_print_tests;

// WASIX integration tests - End-to-end WASIX functionality testing
#[cfg(test)]
pub mod wasix_integration_tests;

// === COMPREHENSIVE TESTS (CI/Development) ===
// These tests provide detailed analysis and may be slower

// Integration tests for end-to-end compiler testing
#[cfg(test)]
pub mod integration_tests;

// WASM execution tests for runtime validation
#[cfg(test)]
pub mod wasm_execution_tests;

// === CODE ANALYSIS TOOLS ===
// Tools for analyzing code usage and identifying dead code

// Code usage analyzer for backend cleanup
pub mod code_usage_analyzer;

// Code analysis runner for generating reports
pub mod run_code_analysis;

// === DEBUG TOOLS ===
// Debug tests for WASM compilation pipeline
#[cfg(test)]
pub mod wasm_compilation_debug;


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

// Re-export code analysis functions
pub use run_code_analysis::run_comprehensive_analysis;
