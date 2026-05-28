//! Contextual coercion for explicit type boundaries.
//!
//! WHAT: applies the small set of implicit conversions that Beanstalk allows
//! only when a surrounding declaration, argument, field, collection, or return
//! slot supplies the target type.
//! WHY: the expression parser resolves `1 + 1` as `Int` regardless of the
//! surrounding declaration type, and a normal `T` expression remains `T` even
//! when assigned to `T?`. This module bridges that gap by inserting explicit
//! AST coercion nodes after natural expression typing has completed.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

use crate::compiler_frontend::type_coercion::compatibility::is_numeric_coercible_by_id;
use crate::compiler_frontend::value_mode::ValueMode;

/// Applies contextual coercion to `expr` if the target type requires it.
///
/// WHAT: post-parse coercion step for explicit type boundaries.
/// WHY: `create_expression` resolves the natural type of an expression. When a
/// boundary says `Float` or `T?`, this function rewrites the expression to carry
/// the explicit target type so HIR/backend lowering can materialize the
/// conversion instead of silently mistyping the inner value.
///
/// Returns the original expression unchanged when no coercion is needed.
pub(crate) fn coerce_expression_to_declared_type(
    expr: Expression,
    declared_type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> Expression {
    if should_wrap_in_option(expr.type_id, declared_type_id, type_environment) {
        return Expression::coerced(expr, declared_type_id);
    }

    if is_numeric_coercible_by_id(expr.type_id, declared_type_id, type_environment) {
        if let ExpressionKind::Int(value) = &expr.kind {
            return Expression::float(
                *value as f64,
                expr.location.clone(),
                ValueMode::ImmutableOwned,
            );
        }

        return Expression::coerced(expr, declared_type_id);
    }

    expr
}

fn should_wrap_in_option(
    actual_type_id: TypeId,
    declared_type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> bool {
    if actual_type_id == declared_type_id {
        return false;
    }

    type_environment.option_inner_type(declared_type_id) == Some(actual_type_id)
}

#[cfg(test)]
#[path = "tests/contextual_tests.rs"]
mod contextual_tests;
