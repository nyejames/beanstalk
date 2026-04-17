//! Shared operator-policy helpers.

use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_type_error;

pub(super) fn reject_result_operands(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if lhs.is_result() || rhs.is_result() {
        return_type_error!(
            format!(
                "Operator '{}' does not implicitly unwrap Result values (found '{}' and '{}').",
                op.to_str(),
                lhs.display_with_table(string_table),
                rhs.display_with_table(string_table)
            ),
            location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Handle the Result with '!' syntax before using it in an ordinary expression",
                ExpectedType => "Non-Result operands",
                FoundType => format!(
                    "{} and {}",
                    lhs.display_with_table(string_table),
                    rhs.display_with_table(string_table)
                ),
            }
        );
    }

    Ok(())
}

pub(super) fn is_mixed_int_float(lhs: &DataType, rhs: &DataType) -> bool {
    matches!(
        (lhs, rhs),
        (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int)
    )
}

pub(super) fn is_relational_operator(op: &Operator) -> bool {
    matches!(
        op,
        Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
    )
}

pub(super) fn is_optional_like(data_type: &DataType) -> bool {
    match data_type {
        DataType::Option(_) => true,
        DataType::Reference(inner) => is_optional_like(inner.as_ref()),
        _ => false,
    }
}
