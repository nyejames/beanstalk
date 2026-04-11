//! Comparison operator typing policy.

use super::diagnostics::invalid_comparison_types;
use super::shared::is_mixed_int_float;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringTable;

pub(super) fn is_comparison_operator(op: &Operator) -> bool {
    matches!(
        op,
        Operator::Equality
            | Operator::NotEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
    )
}

pub(super) fn resolve_comparison_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    if lhs == rhs {
        return match (lhs, op) {
            (
                DataType::Int | DataType::Float | DataType::Decimal,
                Operator::Equality
                | Operator::NotEqual
                | Operator::GreaterThan
                | Operator::GreaterThanOrEqual
                | Operator::LessThan
                | Operator::LessThanOrEqual,
            ) => Ok(DataType::Bool),
            (DataType::Bool, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (DataType::StringSlice, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (
                DataType::Char,
                Operator::Equality
                | Operator::NotEqual
                | Operator::LessThan
                | Operator::LessThanOrEqual
                | Operator::GreaterThan
                | Operator::GreaterThanOrEqual,
            ) => Ok(DataType::Bool),
            _ => invalid_comparison_types(lhs, rhs, op, location, string_table),
        };
    }

    if is_mixed_int_float(lhs, rhs) {
        return Ok(DataType::Bool);
    }

    invalid_comparison_types(lhs, rhs, op, location, string_table)
}
