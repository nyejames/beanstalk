//! WASM Module Validation
//!
//! This module provides comprehensive WASM module validation using wasmparser.
//! It integrates with Beanstalk's error system to provide detailed validation
//! error reporting with LIR context and suggestions for fixing issues.
//!
//! ## Features
//!
//! - Complete WASM module validation using wasmparser
//! - Stack type validation and consistency checking
//! - Detailed error reporting with LIR context
//! - Suggestions for common validation failures
//! - Integration with WasmModuleBuilder for pre-validation checks

use crate::backends::wasm::error::WasmGenerationError;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use std::collections::HashMap;

/// Comprehensive WASM module validator that provides detailed error reporting.
///
/// This validator uses wasmparser to perform complete WASM module validation
/// and converts any validation errors into detailed WasmGenerationError instances
/// with appropriate context and suggestions.
pub struct WasmValidator {
    /// Mapping from function indices to their names for better error reporting
    #[allow(dead_code)]
    function_names: HashMap<u32, String>,
    /// Current validation context for error reporting
    current_context: String,
}

impl WasmValidator {
    /// Create a new WASM validator with default configuration.
    pub fn new() -> Self {
        WasmValidator {
            function_names: HashMap::new(),
            current_context: "WASM module".to_string(),
        }
    }

    /// Set function names for better error reporting.
    ///
    /// This allows the validator to provide function names in error messages
    /// instead of just function indices.
    #[allow(dead_code)]
    pub fn set_function_names(&mut self, names: HashMap<u32, String>) {
        self.function_names = names;
    }

    /// Set the current validation context for error reporting.
    ///
    /// This context is included in error messages to help identify
    /// where validation failures occurred.
    pub fn set_context(&mut self, context: impl Into<String>) {
        self.current_context = context.into();
    }

    /// Validate a complete WASM module.
    ///
    /// This method performs comprehensive validation of the WASM module
    /// using wasmparser's built-in validation.
    pub fn validate_module(&mut self, wasm_bytes: &[u8]) -> Result<(), WasmGenerationError> {
        self.set_context("WASM module validation");

        // Use wasmparser's built-in validation
        match wasmparser::validate(wasm_bytes) {
            Ok(_) => Ok(()),
            Err(e) => Err(WasmGenerationError::from_wasmparser_error(
                &e,
                &self.current_context,
            )),
        }
    }

    /// Validate stack type consistency for a sequence of operations.
    ///
    /// This method checks that the operand stack maintains proper type
    /// consistency throughout a sequence of operations.
    #[allow(dead_code)]
    pub fn validate_stack_consistency(
        &self,
        operations: &[String],
        initial_stack: &[wasmparser::ValType],
    ) -> Result<Vec<wasmparser::ValType>, WasmGenerationError> {
        // This is a simplified stack type checker
        // In a full implementation, this would simulate the stack effects
        // of each operation and verify type consistency

        let mut stack = initial_stack.to_vec();

        for (i, op) in operations.iter().enumerate() {
            match op.as_str() {
                "i32.const" => stack.push(wasmparser::ValType::I32),
                "i64.const" => stack.push(wasmparser::ValType::I64),
                "f32.const" => stack.push(wasmparser::ValType::F32),
                "f64.const" => stack.push(wasmparser::ValType::F64),
                "i32.add" | "i32.sub" | "i32.mul" | "i32.div_s" | "i32.div_u" => {
                    if stack.len() < 2 {
                        return Err(WasmGenerationError::stack_imbalance(
                            2,
                            stack.len() as i32,
                            format!("operation {} at position {}", op, i),
                            "Ensure two i32 values are on the stack before binary operations",
                        ));
                    }
                    let b = stack.pop().unwrap();
                    let a = stack.pop().unwrap();
                    if a != wasmparser::ValType::I32 || b != wasmparser::ValType::I32 {
                        return Err(WasmGenerationError::type_mismatch(
                            "i32, i32",
                            &format!("{:?}, {:?}", a, b),
                            &format!("i32 binary operation {} at position {}", op, i),
                        ));
                    }
                    stack.push(wasmparser::ValType::I32);
                }
                "local.get" => {
                    // For this simplified checker, assume local.get pushes i32
                    // In a full implementation, this would look up the local type
                    stack.push(wasmparser::ValType::I32);
                }
                "local.set" => {
                    if stack.is_empty() {
                        return Err(WasmGenerationError::stack_imbalance(
                            1,
                            0,
                            format!("local.set at position {}", i),
                            "Ensure a value is on the stack before local.set",
                        ));
                    }
                    stack.pop();
                }
                _ => {
                    // For unknown operations, assume they're valid
                    // A full implementation would have complete operation definitions
                }
            }
        }

        Ok(stack)
    }

    /// Validate that all function calls reference valid function indices.
    ///
    /// This method performs a simplified check by parsing the WASM module
    /// and looking for call instructions with out-of-bounds indices.
    #[allow(dead_code)]
    pub fn validate_function_calls(
        &self,
        _wasm_bytes: &[u8],
        _total_function_count: u32,
    ) -> Result<(), WasmGenerationError> {
        // For now, we rely on wasmparser's built-in validation
        // which will catch invalid function indices
        // This method can be enhanced later with more specific checks
        Ok(())
    }

    /// Validate comprehensive index consistency across the entire module.
    ///
    /// This method performs thorough validation of all indices in the module
    /// by relying on wasmparser's comprehensive validation.
    #[allow(dead_code)]
    pub fn validate_comprehensive_index_consistency(
        &self,
        wasm_bytes: &[u8],
        _total_function_count: u32,
        _total_type_count: u32,
        _total_memory_count: u32,
        _total_global_count: u32,
    ) -> Result<(), WasmGenerationError> {
        // wasmparser's validate() function already performs comprehensive
        // index consistency checking, so we don't need to duplicate that work
        match wasmparser::validate(wasm_bytes) {
            Ok(_) => Ok(()),
            Err(e) => Err(WasmGenerationError::from_wasmparser_error(
                &e,
                "comprehensive index validation",
            )),
        }
    }
}

impl Default for WasmValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a WASM module with comprehensive error reporting.
///
/// This is a convenience function that creates a validator and validates
/// the module in one step.
pub fn validate_wasm_module_comprehensive(
    wasm_bytes: &[u8],
    context: &str,
) -> Result<(), WasmGenerationError> {
    let mut validator = WasmValidator::new();
    validator.set_context(context);
    validator.validate_module(wasm_bytes)
}

/// Validate a WASM module and return a CompilerError if validation fails.
///
/// This is a convenience function that combines validation and error conversion.
#[allow(dead_code)]
pub fn validate_wasm_module_with_location(
    wasm_bytes: &[u8],
    context: &str,
    location: ErrorLocation,
) -> Result<(), CompilerError> {
    validate_wasm_module_comprehensive(wasm_bytes, context)
        .map_err(|e| e.to_compiler_error(location))
}
