//! Shared operator-policy diagnostics.

use super::shared::{is_optional_like, is_relational_operator};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::diagnostics::NUMERIC_MIX_HINT;
use crate::return_type_error;

pub(super) fn invalid_comparison_types(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    let found_type = format!(
        "{} and {}",
        lhs.display_with_table(string_table),
        rhs.display_with_table(string_table)
    );

    let (expected_type, suggestion) = if is_relational_operator(op) {
        (
            "Matching numeric or Char operands (<, <=, >, >=). Int/Float mixed comparisons are also supported",
            "Compare values with compatible ordering types or cast first",
        )
    } else if is_optional_like(lhs) || is_optional_like(rhs) {
        (
            "Matching scalar operands (Bool, Int, Float, Decimal, Char, or String)",
            "Handle the optional value first, then compare scalar payload values",
        )
    } else {
        (
            "Matching scalar operands (Bool, Int, Float, Decimal, Char, or String)",
            "Use compatible scalar operand types for this comparison",
        )
    };

    return_type_error!(
        format!(
            "Comparison operator '{}' cannot compare '{}'.",
            op.to_str(),
            found_type
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            ExpectedType => expected_type,
            FoundType => found_type,
            PrimarySuggestion => suggestion,
        }
    )
}

pub(super) fn invalid_operator_types(
    lhs: &DataType,
    rhs: &DataType,
    op: &Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    let found_type = format!(
        "{} and {}",
        lhs.display_with_table(string_table),
        rhs.display_with_table(string_table)
    );

    let (expected_type, suggestion, numeric_hint) = match op {
        Operator::Add
        | Operator::Subtract
        | Operator::Multiply
        | Operator::Divide
        | Operator::Modulus
        | Operator::Exponent => (
            "Numeric operands (Int, Float, or Decimal). '+' also supports String + String",
            "Use compatible numeric operands or cast explicitly before this operation",
            Some(NUMERIC_MIX_HINT),
        ),
        Operator::IntDivide => (
            "Int and Int",
            "Use integer operands with '//' or switch to '/' for real division",
            None,
        ),
        Operator::Range => (
            "Int and Int",
            "Use integer bounds for range expressions",
            None,
        ),
        _ => (
            "Compatible operands for this operator",
            "Use matching operand types or add an explicit cast first",
            None,
        ),
    };

    let hint_suffix = numeric_hint
        .map(|hint| format!(" {hint}"))
        .unwrap_or_default();

    let operator_category = if matches!(op, Operator::Range) {
        "Range"
    } else {
        "Operator"
    };

    return_type_error!(
        format!(
            "{operator_category} '{}' cannot be applied to '{}'.{}",
            op.to_str(),
            found_type,
            hint_suffix
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            ExpectedType => expected_type,
            FoundType => found_type,
            PrimarySuggestion => suggestion,
        }
    )
}
