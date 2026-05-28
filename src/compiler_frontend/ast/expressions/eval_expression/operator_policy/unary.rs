//! Unary operator typing policy.
//!
//! WHAT: resolves the result type of unary `not` and unary minus (`-`) expressions.
//! WHY: AST expression evaluation must enforce that `not` is strictly boolean while
//!      unary minus preserves the underlying numeric type.

use super::super::result_type::ExpressionResultType;
use crate::compiler_frontend::ast::expressions::eval_expression::typing_error::ExpressionTypingError;
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, UnsupportedOperatorCategory,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;

/// Resolve the result type of a unary operator application.
///
/// `not` requires a boolean operand and returns `Bool`. Unary minus preserves
/// the operand's numeric type; the tokenizer/parser already distinguish negative
/// literals from runtime unary subtraction.
pub(super) fn resolve_unary_operator_type(
    op: &Operator,
    operand: &ExpressionResultType,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<ExpressionResultType, ExpressionTypingError> {
    match op {
        Operator::Not => {
            let bool_type_id = type_environment.builtins().bool;

            if operand.type_id == bool_type_id {
                Ok(ExpressionResultType::from_type_id(
                    bool_type_id,
                    type_environment,
                ))
            } else {
                Err(CompilerDiagnostic::unsupported_operator_types(
                    UnsupportedOperatorCategory::Unary,
                    operand.type_id,
                    None,
                    location.clone(),
                )
                .into())
            }
        }

        // Unary minus preserves the numeric payload type. The tokenizer/parser already own the
        // distinction between negative literals and a runtime unary subtraction operator.
        Operator::Subtract => Ok(operand.to_owned()),

        // Defensive fallback: `Not` and `Subtract` are the only operators that can appear in
        // unary position. Preserving the operand type keeps the function total without a panic.
        _ => Ok(operand.to_owned()),
    }
}
