//! Error Helper Methods
//!
//! This module provides helper methods for creating LIR transformation errors
//! with consistent formatting.

use crate::compiler::compiler_messages::compiler_errors::{CompilerError, ErrorLocation};

/// Creates an unsupported node error.
pub fn unsupported_node_error(node_type: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Unsupported HIR node type: {}", node_type))
}

/// Creates an unsupported node error with location.
pub fn unsupported_node_error_with_location(
    node_type: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err =
        CompilerError::lir_transformation(format!("Unsupported HIR node type: {}", node_type));
    err.location = location;
    err
}

/// Creates a type mismatch error.
pub fn type_mismatch_error(expected: &str, found: &str) -> CompilerError {
    CompilerError::lir_transformation(format!(
        "Type mismatch: expected {}, found {}",
        expected, found
    ))
}

/// Creates a type mismatch error with location.
pub fn type_mismatch_error_with_location(
    expected: &str,
    found: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!(
        "Type mismatch: expected {}, found {}",
        expected, found
    ));
    err.location = location;
    err
}

/// Creates a control flow error.
pub fn control_flow_error(message: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Control flow error: {}", message))
}

/// Creates a control flow error with location.
pub fn control_flow_error_with_location(message: &str, location: ErrorLocation) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!("Control flow error: {}", message));
    err.location = location;
    err
}

/// Creates a memory operation error.
pub fn memory_operation_error(message: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Memory operation error: {}", message))
}

/// Creates a memory operation error with location.
pub fn memory_operation_error_with_location(
    message: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!("Memory operation error: {}", message));
    err.location = location;
    err
}

/// Creates an undefined variable error.
pub fn undefined_variable_error(var_name: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Undefined variable: {}", var_name))
}

/// Creates an undefined variable error with location.
pub fn undefined_variable_error_with_location(
    var_name: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!("Undefined variable: {}", var_name));
    err.location = location;
    err
}

/// Creates an unknown struct error.
pub fn unknown_struct_error(struct_name: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Unknown struct type: {}", struct_name))
}

/// Creates an unknown struct error with location.
pub fn unknown_struct_error_with_location(
    struct_name: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err =
        CompilerError::lir_transformation(format!("Unknown struct type: {}", struct_name));
    err.location = location;
    err
}

/// Creates an unknown field error.
pub fn unknown_field_error(field_name: &str, struct_name: &str) -> CompilerError {
    CompilerError::lir_transformation(format!(
        "Unknown field '{}' in struct '{}'",
        field_name, struct_name
    ))
}

/// Creates an unknown field error with location.
pub fn unknown_field_error_with_location(
    field_name: &str,
    struct_name: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!(
        "Unknown field '{}' in struct '{}'",
        field_name, struct_name
    ));
    err.location = location;
    err
}

/// Creates an unknown function error.
pub fn unknown_function_error(func_name: &str) -> CompilerError {
    CompilerError::lir_transformation(format!("Unknown function: {}", func_name))
}

/// Creates an unknown function error with location.
pub fn unknown_function_error_with_location(
    func_name: &str,
    location: ErrorLocation,
) -> CompilerError {
    let mut err = CompilerError::lir_transformation(format!("Unknown function: {}", func_name));
    err.location = location;
    err
}

/// Creates an internal error (compiler bug).
pub fn internal_error(message: &str) -> CompilerError {
    CompilerError::compiler_error(format!("Internal LIR lowering error: {}", message))
}

/// Creates an internal error with location.
pub fn internal_error_with_location(message: &str, location: ErrorLocation) -> CompilerError {
    let mut err =
        CompilerError::compiler_error(format!("Internal LIR lowering error: {}", message));
    err.location = location;
    err
}
