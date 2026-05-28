//! Operator typing policy for AST expression evaluation.
//!
//! WHAT: resolves unary/binary operator result types for natural expressions.
//! WHY: AST is the policy owner for operator typing; contextual coercion happens at explicit
//! declaration/return boundaries after parsing.

mod arithmetic;
mod comparison;
mod diagnostics;
mod logical;
mod shared;
mod unary;

use super::result_type::ExpressionResultType;
use crate::compiler_frontend::ast::expressions::eval_expression::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;

pub(super) fn resolve_unary_operator_type(
    op: &Operator,
    operand: &ExpressionResultType,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    unary::resolve_unary_operator_type(op, operand, location, type_environment)
}

pub(super) fn resolve_binary_operator_type(
    lhs: &ExpressionResultType,
    rhs: &ExpressionResultType,
    op: &Operator,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    shared::reject_result_operands(lhs, rhs, op, location, type_environment)?;

    if logical::is_logical_operator(op) {
        return logical::resolve_logical_operator_type(lhs, rhs, op, location, type_environment);
    }

    if comparison::is_comparison_operator(op) {
        return comparison::resolve_comparison_operator_type(
            lhs,
            rhs,
            op,
            location,
            type_environment,
        );
    }

    arithmetic::resolve_arithmetic_operator_type(lhs, rhs, op, location, type_environment)
}
