//! # WIR Utilities Module
//!
//! This module contains utility functions and helper types used throughout
//! the WIR transformation process. It provides type checking and conversion
//! utilities, handles data type operations and comparisons, offers string
//! manipulation helpers, and supplies common error handling patterns.

use crate::compiler::{
    compiler_errors::CompileError,
    datatypes::DataType,
    parsers::expressions::expression::Operator,
    wir::wir_nodes::{BinOp, Constant, Operand},
};

use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::{return_compiler_error, return_type_error, return_wir_transformation_error};

/// Infer the result type of a binary operation based on operand types
///
/// This function implements type inference for binary operations in WIR,
/// ensuring type consistency and proper WASM type mapping. It validates
/// that operands have compatible types and determines the result type.
///
/// # Parameters
///
/// - `left_operand`: Left operand of the binary operation
/// - `right_operand`: Right operand of the binary operation  
/// - `wir_op`: WIR binary operation being performed
/// - `location`: Source location for error reporting
///
/// # Returns
///
/// - `Ok(DataType)`: Inferred result type of the operation
/// - `Err(CompileError)`: Type error if operands are incompatible
///
/// # Type Inference Rules
///
/// - **Arithmetic Operations** (`+`, `-`, `*`, `/`, `%`): Both operands must have
///   the same base type, result has the same type
/// - **Comparison Operations** (`==`, `!=`, `<`, `<=`, `>`, `>=`): Both operands
///   must have the same base type, result is always `Bool`
/// - **Logical Operations** (`&&`, `||`): Both operands must be `Bool`, result is `Bool`
///
/// # WASM Compatibility
///
/// Type inference ensures all operations map cleanly to WASM instructions:
/// - `Int` operations use WASM i32/i64 instructions
/// - `Float` operations use WASM f32/f64 instructions
/// - `Bool` operations use WASM i32 with 0/1 values
pub fn infer_binary_operation_result_type(
    left_operand: &Operand,
    right_operand: &Operand,
    wir_op: &BinOp,
    location: &TextLocation,
) -> Result<DataType, CompileError> {
    // Extract types from operands (simplified approach)
    let left_type = operand_to_datatype(left_operand);
    let right_type = operand_to_datatype(right_operand);

    // For arithmetic operations, both operands should have the same base type
    match wir_op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            if datatypes_match_base_type(&left_type, &right_type) {
                Ok(left_type)
            } else {
                return_type_error!(
                    location.clone(),
                    "Type mismatch in arithmetic operation: cannot perform {:?} on {} and {}. Both operands must have the same type.",
                    wir_op,
                    datatype_to_string(&left_type),
                    datatype_to_string(&right_type)
                );
            }
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            // Comparison operations return boolean
            if datatypes_match_base_type(&left_type, &right_type) {
                Ok(DataType::Bool)
            } else {
                return_type_error!(
                    location.clone(),
                    "Type mismatch in comparison operation: cannot compare {} and {}. Both operands must have the same type.",
                    datatype_to_string(&left_type),
                    datatype_to_string(&right_type)
                );
            }
        }
        BinOp::And | BinOp::Or => {
            // Logical operations require boolean operands and return boolean
            if is_bool_type(&left_type) && is_bool_type(&right_type) {
                Ok(DataType::Bool)
            } else {
                return_type_error!(
                    location.clone(),
                    "Type mismatch in logical operation: {:?} requires boolean operands but got {} and {}.",
                    wir_op,
                    datatype_to_string(&left_type),
                    datatype_to_string(&right_type)
                );
            }
        }
        _ => {
            return_wir_transformation_error!(
                location.clone(),
                "Binary operation {:?} not yet implemented in type inference at line {}, column {}",
                wir_op,
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}

/// Extract DataType from a WIR operand
///
/// Determines the data type of a WIR operand for type checking and inference.
/// This is essential for ensuring type safety and proper WASM instruction selection.
///
/// # Parameters
///
/// - `operand`: WIR operand to analyze
///
/// # Returns
///
/// The `DataType` of the operand
///
/// # Type Extraction Rules
///
/// - **Constants**: Type determined by constant value (I32 → Int, F32 → Float, etc.)
/// - **Places**: Type should be looked up from place manager (simplified to Int for now)
/// - **Function References**: Always treated as function type
/// - **Global References**: Treated as Int (should be enhanced with proper type lookup)
///
/// # Note
///
/// This is a simplified implementation. A complete version would look up
/// place types from the place manager to get accurate type information.
pub fn operand_to_datatype(operand: &Operand) -> DataType {
    match operand {
        Operand::Constant(constant) => {
            match constant {
                Constant::I32(_) | Constant::I64(_) => DataType::Int,
                Constant::F32(_) | Constant::F64(_) => DataType::Float,
                Constant::Bool(_) => DataType::Bool,
                Constant::String(_) => DataType::String,
                Constant::MutableString(_) => DataType::Template,
                Constant::Function(_) => DataType::Function(FunctionSignature::default()),
                Constant::Null => DataType::Int, // Null is represented as integer 0
                Constant::MemoryOffset(_) => DataType::Int, // Memory offsets are integers
                Constant::TypeSize(_) => DataType::Int, // Type sizes are integers
            }
        }
        // For places, we'd need to look up the type in the place manager
        // For now, assume Int as default (this should be enhanced)
        Operand::Copy(_) | Operand::Move(_) => DataType::Int,
        Operand::FunctionRef(_) => DataType::Function(FunctionSignature::default()),
        Operand::GlobalRef(_) => DataType::Int,
    }
}

/// Check if two DataTypes have the same base type (ignoring ownership)
///
/// Compares two data types for compatibility in binary operations, ignoring
/// ownership and mutability differences. This is used for type checking
/// where the base type must match but ownership can differ.
///
/// # Parameters
///
/// - `left`: First data type to compare
/// - `right`: Second data type to compare
///
/// # Returns
///
/// `true` if the base types match, `false` otherwise
///
/// # Matching Rules
///
/// - `Int` matches `Int` regardless of ownership
/// - `Float` matches `Float` regardless of ownership  
/// - `Bool` matches `Bool` regardless of ownership
/// - `String` matches `String` regardless of ownership
/// - Other types do not match (should be extended as needed)
///
/// # Example
///
/// ```rust
/// assert!(datatypes_match_base_type(&DataType::Int, &DataType::Int));
/// // Would also match if one was mutable and the other immutable
/// ```
pub fn datatypes_match_base_type(left: &DataType, right: &DataType) -> bool {
    use crate::compiler::datatypes::DataType as DT;

    matches!(
        (left, right),
        (DT::Int, DT::Int)
            | (DT::Float, DT::Float)
            | (DT::Bool, DT::Bool)
            | (DT::String, DT::String)
    )
}

/// Check if a DataType is a boolean type
///
/// Determines whether a data type represents a boolean value for logical
/// operations and conditional expressions.
///
/// # Parameters
///
/// - `data_type`: Data type to check
///
/// # Returns
///
/// `true` if the type is boolean, `false` otherwise
///
/// # Usage
///
/// Used to validate operands for logical operations (`&&`, `||`) and
/// conditional expressions that require boolean values.
pub fn is_bool_type(data_type: &DataType) -> bool {
    matches!(data_type, DataType::Bool)
}

/// Convert DataType to string representation for error messages
///
/// Provides human-readable names for data types in compiler error messages.
/// This helps users understand type mismatches and other type-related errors.
///
/// # Parameters
///
/// - `data_type`: Data type to convert to string
///
/// # Returns
///
/// Static string representation of the data type
///
/// # Supported Types
///
/// - `Int` → "Int"
/// - `Float` → "Float"  
/// - `Bool` → "Bool"
/// - `String` → "String"
/// - Others → "Unknown" (should be extended as new types are added)
///
/// # Usage
///
/// Primarily used in error messages to show users what types were expected
/// vs. what types were actually provided in type mismatches.
pub fn datatype_to_string(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Int => "Int",
        DataType::Float => "Float",
        DataType::Bool => "Bool",
        DataType::String => "String",
        _ => "Unknown",
    }
}

/// Convert AST Operator to WIR BinOp
///
/// Transforms AST-level operators into WIR binary operations for code generation.
/// This mapping ensures that Beanstalk's operator semantics are preserved in
/// the WIR and can be properly lowered to WASM instructions.
///
/// # Parameters
///
/// - `ast_op`: AST operator to convert
/// - `location`: Source location for error reporting
///
/// # Returns
///
/// - `Ok(BinOp)`: Corresponding WIR binary operation
/// - `Err(CompileError)`: Error if operator is not yet supported
///
/// # Operator Mappings
///
/// - **Arithmetic**: `+` → `Add`, `-` → `Sub`, `*` → `Mul`, `/` → `Div`, `%` → `Rem`
/// - **Comparison**: `==` → `Eq`, `!=` → `Ne`, `<` → `Lt`, `<=` → `Le`, `>` → `Gt`, `>=` → `Ge`
/// - **Logical**: `&&` → `And`, `||` → `Or`
///
/// # WASM Lowering
///
/// Each WIR BinOp maps directly to WASM instructions:
/// - Arithmetic operations use WASM arithmetic instructions
/// - Comparisons use WASM comparison instructions  
/// - Logical operations use WASM conditional and bitwise instructions
pub fn ast_operator_to_wir_binop(
    ast_op: &Operator,
    location: &TextLocation,
) -> Result<BinOp, CompileError> {
    match ast_op {
        Operator::Add => Ok(BinOp::Add),
        Operator::Subtract => Ok(BinOp::Sub),
        Operator::Multiply => Ok(BinOp::Mul),
        Operator::Divide => Ok(BinOp::Div),
        Operator::Modulus => Ok(BinOp::Rem),
        Operator::Equality => Ok(BinOp::Eq),
        Operator::Not => Ok(BinOp::Ne),
        Operator::LessThan => Ok(BinOp::Lt),
        Operator::LessThanOrEqual => Ok(BinOp::Le),
        Operator::GreaterThan => Ok(BinOp::Gt),
        Operator::GreaterThanOrEqual => Ok(BinOp::Ge),
        Operator::And => Ok(BinOp::And),
        Operator::Or => Ok(BinOp::Or),
        _ => {
            return_wir_transformation_error!(
                location.clone(),
                "Operator {:?} not yet implemented for WIR binary operations at line {}, column {}",
                ast_op,
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}
