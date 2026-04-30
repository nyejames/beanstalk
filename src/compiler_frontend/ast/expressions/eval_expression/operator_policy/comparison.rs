//! Comparison operator typing policy.

use super::diagnostics::invalid_comparison_types;
use super::shared::is_mixed_int_float;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_rule_error;

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
            (DataType::Choices { .. }, Operator::Equality | Operator::NotEqual) => {
                return_rule_error!(
                    "Choice equality is deferred. Use pattern matching and compare variants or payload fields inside match arms.",
                    location.clone(),
                    {
                        CompilationStage => "Expression Evaluation",
                        PrimarySuggestion => "Use 'if value is: case Variant => ...' or payload captures such as 'case Variant(field) => ...'",
                    }
                )
            }
            _ => invalid_comparison_types(lhs, rhs, op, location, string_table),
        };
    }

    if is_mixed_int_float(lhs, rhs) {
        return Ok(DataType::Bool);
    }

    invalid_comparison_types(lhs, rhs, op, location, string_table)
}
