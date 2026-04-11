//! Logical operator typing policy.

use super::shared::is_optional_like;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_type_error;

pub(super) fn is_logical_operator(op: &Operator) -> bool {
    matches!(op, Operator::And | Operator::Or)
}

pub(super) fn resolve_logical_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    if lhs == &DataType::Bool && rhs == &DataType::Bool {
        return Ok(DataType::Bool);
    }

    let found_type = format!(
        "{} and {}",
        lhs.display_with_table(string_table),
        rhs.display_with_table(string_table)
    );
    let suggestion = if is_optional_like(lhs) || is_optional_like(rhs) {
        "Handle the optional value first before using logical operators"
    } else {
        "Use Bool operands on both sides of this logical operator"
    };

    return_type_error!(
        format!(
            "Logical operator '{}' requires Bool operands, found '{}'.",
            op.to_str(),
            found_type
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            ExpectedType => "Bool and Bool",
            FoundType => found_type,
            PrimarySuggestion => suggestion,
        }
    )
}
