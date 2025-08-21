// Test modules for the Beanstalk compiler

// Legacy IR tests - temporarily disabled during MIR refactor
#[cfg(test)]
pub mod ir_tests;

// Legacy variable reference tests - temporarily disabled during MIR refactor
#[cfg(test)]
pub mod variable_reference_tests;

// Legacy block manager tests - temporarily disabled during MIR refactor
#[cfg(test)]
pub mod block_manager_tests;

// Legacy block structure tests - temporarily disabled during MIR refactor
#[cfg(test)]
pub mod block_structure_tests;

// Active MIR place system tests - comprehensive and up-to-date
#[cfg(test)]
pub mod place_tests;

// Integration tests for end-to-end compiler testing
pub mod integration_tests;

// Re-export the function that CLI needs
pub use integration_tests::run_all_test_cases;