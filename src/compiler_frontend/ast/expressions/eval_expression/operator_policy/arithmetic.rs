//! Arithmetic and non-comparison binary operator typing policy.

use super::diagnostics::invalid_operator_types;
use super::shared::is_mixed_int_float;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(super) fn resolve_arithmetic_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    if lhs == rhs {
        // Same-type operator handling stays explicit so broad "compatible" types cannot quietly
        // weaken arithmetic rules.
        return match (lhs, op) {
            (
                DataType::Int,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Int),
            (
                DataType::Float,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Float),
            (
                DataType::Decimal,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Decimal),
            (DataType::StringSlice, Operator::Add) => Ok(DataType::StringSlice),
            (DataType::Int, Operator::Range) => Ok(DataType::Range),
            _ => invalid_operator_types(lhs, rhs, op, location, string_table),
        };
    }

    if is_mixed_int_float(lhs, rhs) {
        // Mixed numeric promotion is intentionally narrow: only Int/Float pairs mix implicitly,
        // and only for numeric arithmetic/comparisons.
        return match op {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Modulus
            | Operator::Exponent
            | Operator::Root => Ok(DataType::Float),
            _ => invalid_operator_types(lhs, rhs, op, location, string_table),
        };
    }

    invalid_operator_types(lhs, rhs, op, location, string_table)
}
