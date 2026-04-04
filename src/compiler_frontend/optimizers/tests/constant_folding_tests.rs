//! Regression tests for constant-expression folding helpers.

use super::*;
use crate::compiler_frontend::string_interning::StringTable;

#[test]
fn evaluate_operator_concatenates_string_literals() {
    let mut string_table = StringTable::new();
    let lhs = Expression::string_slice(
        string_table.intern("bean"),
        Default::default(),
        Ownership::ImmutableOwned,
    );
    let rhs = Expression::string_slice(
        string_table.intern("stalk"),
        Default::default(),
        Ownership::ImmutableOwned,
    );

    let result = lhs
        .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
        .expect("string concatenation should succeed")
        .expect("string concatenation should fold");

    assert!(matches!(result.kind, ExpressionKind::StringSlice(_)));
    let ExpressionKind::StringSlice(interned) = result.kind else {
        unreachable!("checked above");
    };
    assert_eq!(string_table.resolve(interned), "beanstalk");
}

#[test]
fn evaluate_operator_promotes_negative_integer_exponent_to_float() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(2, Default::default(), Ownership::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), Ownership::ImmutableOwned);

    let result = lhs
        .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
        .expect("integer exponentiation should succeed")
        .expect("integer exponentiation should fold");

    assert!(matches!(
        result.kind,
        ExpressionKind::Float(value) if (value - 0.5).abs() < f64::EPSILON
    ));
    assert_eq!(result.data_type, DataType::Float);
}

#[test]
fn evaluate_operator_returns_none_for_mismatched_constant_types() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(2, Default::default(), Ownership::ImmutableOwned);
    let rhs = Expression::bool(true, Default::default(), Ownership::ImmutableOwned);

    let result = lhs
        .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
        .expect("mismatched types should not error");

    assert!(result.is_none());
}
