//! Logical operator typing policy.
//!
//! WHAT: resolves the result type of `and` and `or` expressions.
//! WHY: logical operators require both operands to be `Bool`; any other combination is rejected
//!     with a structured diagnostic so that backend lowering never sees an ill-typed logical op.

use super::super::result_type::ExpressionResultType;
use super::diagnostics::diagnostic_operator_from_ast;
use crate::compiler_frontend::ast::expressions::eval_expression::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;

pub(super) fn is_logical_operator(op: &Operator) -> bool {
    matches!(op, Operator::And | Operator::Or)
}

pub(super) fn resolve_logical_operator_type(
    lhs: &ExpressionResultType,
    rhs: &ExpressionResultType,
    op: &Operator,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    let bool_type_id = type_environment.builtins().bool;

    // Both operands must be Bool. Mixed or non-bool combinations are rejected.
    if lhs.type_id == bool_type_id && rhs.type_id == bool_type_id {
        return Ok(ExpressionResultType::from_type_id(
            bool_type_id,
            type_environment,
        ));
    }

    Err(CompilerDiagnostic::unsupported_operator_types(
        diagnostic_operator_from_ast(op),
        lhs.type_id,
        Some(rhs.type_id),
        location.clone(),
    )
    .into())
}
