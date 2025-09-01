// Test modules for the Beanstalk compiler

// Active MIR place system tests - comprehensive and up-to-date
#[cfg(test)]
pub mod place_tests;

// New MIR borrow checking and dataflow validation tests
#[cfg(test)]
pub mod borrow_check_tests;

// Integration tests for end-to-end compiler testing
pub mod integration_tests;

// Performance validation and optimization tests
#[cfg(test)]
pub mod performance_tests;

// WASM-specific optimization tests
#[cfg(test)]
pub mod wasm_optimization_tests;

// WASM module generation tests
#[cfg(test)]
pub mod wasm_module_tests;

// Comprehensive benchmark runner
#[cfg(test)]
pub mod benchmark_runner;

// Task 15 performance validation
#[cfg(test)]
pub mod performance_validation;

// WASM terminator lowering tests
#[cfg(test)]
pub mod wasm_terminator_tests;

// Memory layout management tests
#[cfg(test)]
pub mod memory_layout_tests;

// Interface VTable generation tests
#[cfg(test)]
pub mod interface_vtable_tests;

// Re-export the function that CLI needs
pub use integration_tests::run_all_test_cases;