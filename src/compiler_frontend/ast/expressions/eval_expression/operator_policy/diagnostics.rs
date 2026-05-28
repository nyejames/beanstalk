//! Shared operator-policy diagnostics.
//!
//! WHAT: constructs `CompilerDiagnostic` values for unsupported operator-type combinations so
//!      comparison, arithmetic, and unary policy files do not duplicate diagnostic creation.
//! WHY: operator category and operand-type reporting is a single responsibility; keeping it here
//!      ensures consistent error messages and stable diagnostic codes across all operator policies.

use super::super::result_type::ExpressionResultType;
use crate::compiler_frontend::ast::expressions::eval_expression::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, UnsupportedOperatorCategory,
};

pub(super) fn invalid_comparison_types(
    lhs: &ExpressionResultType,
    rhs: &ExpressionResultType,
    _op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    Err(CompilerDiagnostic::unsupported_operator_types(
        UnsupportedOperatorCategory::Comparison,
        lhs.type_id,
        Some(rhs.type_id),
        location.clone(),
    )
    .into())
}

pub(super) fn invalid_operator_types(
    lhs: &ExpressionResultType,
    rhs: &ExpressionResultType,
    op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    // Range construction uses its own diagnostic category so errors clearly distinguish
    // slice-range bounds from ordinary arithmetic operands.
    let category = if matches!(op, Operator::Range) {
        UnsupportedOperatorCategory::Range
    } else {
        UnsupportedOperatorCategory::Arithmetic
    };

    Err(CompilerDiagnostic::unsupported_operator_types(
        category,
        lhs.type_id,
        Some(rhs.type_id),
        location.clone(),
    )
    .into())
}
