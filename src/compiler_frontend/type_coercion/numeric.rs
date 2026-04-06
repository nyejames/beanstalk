//! Contextual numeric coercion for the Beanstalk compiler frontend.
//!
//! WHAT: applies Int → Float promotion at declaration and return sites and
//! rewrites the AST to represent the coercion explicitly.
//! WHY: the expression parser resolves `1 + 1` as `Int` regardless of the
//! surrounding declaration type. This module bridges that gap by wrapping
//! the expression in a typed coercion node after the parser returns.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::type_coercion::compatibility::is_numeric_coercible;

/// Applies a contextual numeric coercion to `expr` if the declared type
/// requires it and the actual type supports promotion.
///
/// WHAT: post-parse coercion step for explicit declarations.
/// WHY: `create_expression` resolves the natural type of an expression
/// (e.g. `Int` for `1 + 1`). When the declaration site says `Float`, this
/// function rewrites the expression to carry the correct coerced type.
///
/// Returns the original expression unchanged when no coercion is needed.
pub(crate) fn coerce_expression_to_declared_type(
    expr: Expression,
    declared: &DataType,
) -> Expression {
    apply_numeric_coercion(expr, declared)
}

/// Applies a contextual numeric coercion to `expr` if the return slot type
/// requires it and the actual type supports promotion.
///
/// WHAT: post-parse coercion step for function return values.
/// WHY: same as the declaration case — the expression was resolved
/// independently of the return type, so coercion is applied after the fact.
///
/// Returns the original expression unchanged when no coercion is needed.
pub(crate) fn coerce_expression_to_return_type(expr: Expression, expected: &DataType) -> Expression {
    apply_numeric_coercion(expr, expected)
}

/// Core numeric coercion rewrite.
///
/// WHAT: wraps `expr` in a coercion node when `target` accepts it via
/// numeric promotion. Constant int literals are folded directly to float
/// constants to keep the AST clean where possible.
fn apply_numeric_coercion(expr: Expression, target: &DataType) -> Expression {
    if !is_numeric_coercible(&expr.data_type, target) {
        return expr;
    }

    let location = expr.location.clone();

    // Constant int literals can be converted to float literals directly,
    // avoiding a runtime coercion wrapper for the common case.
    if let ExpressionKind::Int(value) = expr.kind {
        return Expression::float(value as f64, location, Ownership::ImmutableOwned);
    }

    Expression::coerced(expr, target.to_owned())
}

#[cfg(test)]
#[path = "tests/numeric_tests.rs"]
mod numeric_tests;
