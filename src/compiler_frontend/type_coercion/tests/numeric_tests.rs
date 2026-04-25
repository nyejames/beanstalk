//! Numeric coercion tests for `type_coercion::numeric`.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::numeric::{
    coerce_expression_to_declared_type, coerce_expression_to_return_type,
};
use crate::compiler_frontend::value_mode::ValueMode;

fn int_literal(value: i64) -> Expression {
    Expression::int(value, SourceLocation::default(), ValueMode::ImmutableOwned)
}

fn float_literal(value: f64) -> Expression {
    Expression::float(value, SourceLocation::default(), ValueMode::ImmutableOwned)
}

#[test]
fn float_declaration_from_int_literal_becomes_float() {
    let expr = int_literal(1);
    let result = coerce_expression_to_declared_type(expr, &DataType::Float);
    assert_eq!(result.data_type, DataType::Float);
    assert!(
        matches!(result.kind, ExpressionKind::Float(v) if (v - 1.0).abs() < f64::EPSILON),
        "constant int should fold to float literal"
    );
}

#[test]
fn float_declaration_from_int_expression_becomes_coerced() {
    // Simulate a runtime Int expression (non-constant)
    let runtime_expr = Expression::new(
        ExpressionKind::Runtime(vec![]),
        SourceLocation::default(),
        DataType::Int,
        ValueMode::ImmutableOwned,
    );
    let result = coerce_expression_to_declared_type(runtime_expr, &DataType::Float);
    assert_eq!(result.data_type, DataType::Float);
    assert!(
        matches!(result.kind, ExpressionKind::Coerced { .. }),
        "runtime int should become Coerced node"
    );
}

#[test]
fn float_declaration_from_float_is_unchanged() {
    let expr = float_literal(1.5);
    let result = coerce_expression_to_declared_type(expr, &DataType::Float);
    assert_eq!(result.data_type, DataType::Float);
    assert!(
        matches!(result.kind, ExpressionKind::Float(_)),
        "float should not be wrapped in Coerced"
    );
}

#[test]
fn int_declaration_from_int_is_unchanged() {
    let expr = int_literal(42);
    let result = coerce_expression_to_declared_type(expr, &DataType::Int);
    assert_eq!(result.data_type, DataType::Int);
    assert!(matches!(result.kind, ExpressionKind::Int(42)));
}

#[test]
fn float_declaration_rejects_bool_unchanged() {
    // Bool → Float is not coercible; the expression should be returned unchanged.
    let expr = Expression::bool(true, SourceLocation::default(), ValueMode::ImmutableOwned);
    let result = coerce_expression_to_declared_type(expr, &DataType::Float);
    // No coercion applied — type stays Bool.
    assert_eq!(result.data_type, DataType::Bool);
}

#[test]
fn float_return_from_int_literal_becomes_float() {
    let expr = int_literal(7);
    let result = coerce_expression_to_return_type(expr, &DataType::Float);
    assert_eq!(result.data_type, DataType::Float);
    assert!(matches!(result.kind, ExpressionKind::Float(_)));
}
