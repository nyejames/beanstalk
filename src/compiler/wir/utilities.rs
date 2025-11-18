//! # WIR Utilities Module
//!
//! This module contains utility functions and helper types used throughout
//! the WIR transformation process. These functions consolidate common patterns
//! to reduce code duplication and improve maintainability.

use crate::compiler::wir::context::WirTransformContext;
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::wir::wir_nodes::{BorrowKind, Rvalue, Statement};
use crate::compiler::{
    compiler_errors::{CompileError, ErrorMetaDataKey, ErrorType},
    datatypes::DataType,
    parsers::{
        expressions::expression::{Expression, ExpressionKind},
        tokenizer::tokens::TextLocation,
    },
    string_interning::StringTable,
};

/// Look up a variable in the context or return a detailed error
///
/// This consolidates the common pattern of looking up a variable and generating
/// a detailed error message with metadata if the variable is not found.
///
/// # Parameters
///
/// - `context`: Transformation context containing variable bindings
/// - `name`: Variable name to look up
/// - `location`: Source location for error reporting
/// - `string_table`: String table for error location conversion
/// - `context_msg`: Context-specific message fragment (e.g., "mutation", "shared borrow")
///
/// # Returns
///
/// - `Ok(Place)`: The place representing the variable
/// - `Err(CompileError)`: Detailed error if variable is not found
///
/// # Example
///
/// ```rust
/// let place = lookup_variable_or_error(
///     context,
///     "my_var",
///     &location,
///     string_table,
///     "mutation"
/// )?;
/// ```
pub fn lookup_variable_or_error(
    context: &WirTransformContext,
    name: &str,
    location: &TextLocation,
    string_table: &StringTable,
    context_msg: &str,
) -> Result<Place, CompileError> {
    context
        .lookup_variable(name)
        .ok_or_else(|| {
            let error_location = location.clone().to_error_location(string_table);
            let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
            CompileError {
                msg: format!("Undefined variable '{}' in {}", name, context_msg),
                location: error_location,
                error_type: ErrorType::WirTransformation,
                metadata: {
                    let mut map = std::collections::HashMap::new();
                    map.insert(ErrorMetaDataKey::VariableName, name_static);
                    map.insert(ErrorMetaDataKey::CompilationStage, "WIR Transformation");
                    map.insert(
                        ErrorMetaDataKey::PrimarySuggestion,
                        "Ensure the variable is declared before using it",
                    );
                    map
                },
            }
        })
        .map(|place| place.clone())
}

/// Create a borrow rvalue from an expression
///
/// This consolidates the common pattern of creating either a mutable or shared
/// borrow from a variable reference expression, or converting other expressions
/// to rvalues.
///
/// # Parameters
///
/// - `value`: Expression to convert (may be a variable reference or other expression)
/// - `is_mutable`: Whether to create a mutable borrow (true) or shared borrow (false)
/// - `location`: Source location for error reporting
/// - `context`: Transformation context for variable lookup
/// - `string_table`: String table for error location conversion
///
/// # Returns
///
/// - `Ok((statements, rvalue))`: Supporting statements and the resulting rvalue
/// - `Err(CompileError)`: Transformation error
///
/// # Behavior
///
/// - **Variable Reference + Mutable**: Creates `Rvalue::Ref { borrow_kind: Mut }`
/// - **Variable Reference + Immutable**: Creates `Rvalue::Ref { borrow_kind: Shared }`
/// - **Other Expression**: Converts expression to rvalue normally
///
/// # Example
///
/// ```rust
/// // x ~= y  (mutable borrow)
/// let (stmts, rvalue) = create_borrow_rvalue(&expr, true, &loc, context, string_table)?;
///
/// // x = y  (shared borrow)
/// let (stmts, rvalue) = create_borrow_rvalue(&expr, false, &loc, context, string_table)?;
/// ```
pub fn create_borrow_rvalue(
    value: &Expression,
    is_mutable: bool,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut StringTable,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    use crate::compiler::wir::expressions::expression_to_rvalue_with_context;

    match &value.kind {
        ExpressionKind::Reference(var_name) => {
            // This is a borrow: x = y or x ~= y
            let resolved_var_name = string_table.resolve(*var_name);
            let context_msg = if is_mutable {
                "mutable borrow"
            } else {
                "shared borrow"
            };
            let source_place = lookup_variable_or_error(
                context,
                resolved_var_name,
                location,
                string_table,
                context_msg,
            )?;

            let borrow_kind = if is_mutable {
                BorrowKind::Mut
            } else {
                BorrowKind::Shared
            };

            Ok((
                vec![],
                Rvalue::Ref {
                    place: source_place,
                    borrow_kind,
                },
            ))
        }
        _ => {
            // For non-reference expressions, convert normally
            expression_to_rvalue_with_context(value, location, context, string_table)
        }
    }
}

/// Convert WasmType to DataType
///
/// This consolidates the common pattern of converting WASM types to Beanstalk
/// data types. Used throughout expression and statement transformation.
///
/// # Parameters
///
/// - `wasm_type`: WASM type to convert
///
/// # Returns
///
/// - `DataType`: Corresponding Beanstalk data type
///
/// # Mapping
///
/// - `I32` → `Int`
/// - `I64` → `Int`
/// - `F32` → `Float`
/// - `F64` → `Float`
/// - `ExternRef` → `String` (external references default to string)
/// - `FuncRef` → `Int` (function references default to int)
pub fn wasm_type_to_datatype(wasm_type: &WasmType) -> DataType {
    match wasm_type {
        WasmType::I32 => DataType::Int,
        WasmType::F32 => DataType::Float,
        WasmType::I64 => DataType::Int,
        WasmType::F64 => DataType::Float,
        WasmType::ExternRef => DataType::String,
        WasmType::FuncRef => DataType::Int,
    }
}
