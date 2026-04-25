//! Regression tests for constant-expression folding helpers.

use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[test]
fn evaluate_operator_concatenates_string_literals() {
    let mut string_table = StringTable::new();
    let lhs = Expression::string_slice(
        string_table.intern("bean"),
        Default::default(),
        ValueMode::ImmutableOwned,
    );
    let rhs = Expression::string_slice(
        string_table.intern("stalk"),
        Default::default(),
        ValueMode::ImmutableOwned,
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
    let lhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), ValueMode::ImmutableOwned);

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
    let lhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::bool(true, Default::default(), ValueMode::ImmutableOwned);

    let result = lhs
        .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
        .expect("mismatched types should not error");

    assert!(result.is_none());
}

#[test]
fn evaluate_operator_divides_ints_to_float() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(5, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);

    let result = lhs
        .evaluate_operator(&rhs, &Operator::Divide, &mut string_table)
        .expect("int division should fold")
        .expect("int division should produce folded expression");

    assert!(matches!(
        result.kind,
        ExpressionKind::Float(value) if (value - 2.5).abs() < f64::EPSILON
    ));
    assert_eq!(result.data_type, DataType::Float);
    assert!(
        result.contains_regular_division,
        "folded regular division should preserve provenance"
    );
}

#[test]
fn evaluate_operator_integer_division_truncates_toward_zero() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(-5, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);

    let result = lhs
        .evaluate_operator(&rhs, &Operator::IntDivide, &mut string_table)
        .expect("integer division should fold")
        .expect("integer division should produce folded expression");

    assert!(matches!(result.kind, ExpressionKind::Int(-2)));
    assert_eq!(result.data_type, DataType::Int);
}

#[test]
fn evaluate_operator_rejects_divide_by_zero_for_both_division_operators() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(5, Default::default(), ValueMode::ImmutableOwned);
    let zero = Expression::int(0, Default::default(), ValueMode::ImmutableOwned);

    let divide_error = lhs
        .evaluate_operator(&zero, &Operator::Divide, &mut string_table)
        .expect_err("regular division by zero should fail during fold");
    assert!(divide_error.msg.contains("Can't divide by zero"));

    let int_divide_error = lhs
        .evaluate_operator(&zero, &Operator::IntDivide, &mut string_table)
        .expect_err("integer division by zero should fail during fold");
    assert!(int_divide_error.msg.contains("Can't divide by zero"));
}

#[test]
fn evaluate_operator_rejects_integer_add_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MAX, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
        .expect_err("integer add overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'+'"));
}

#[test]
fn evaluate_operator_rejects_integer_subtract_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Subtract, &mut string_table)
        .expect_err("integer subtract overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'-'"));
}

#[test]
fn evaluate_operator_rejects_integer_multiply_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MAX, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Multiply, &mut string_table)
        .expect_err("integer multiply overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'*'"));
}

#[test]
fn evaluate_operator_rejects_integer_exponent_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(63, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
        .expect_err("integer exponent overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'^'"));
}

#[test]
fn evaluate_operator_rejects_integer_division_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::IntDivide, &mut string_table)
        .expect_err("integer division overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'//'"));
}

#[test]
fn evaluate_operator_rejects_integer_modulus_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Modulus, &mut string_table)
        .expect_err("integer modulus overflow should fail during fold");
    assert!(error.msg.contains("Compile-time integer overflow"));
    assert!(error.msg.contains("'%'"));
}

#[test]
fn evaluate_operator_rejects_non_finite_float_exponent_result() {
    let mut string_table = StringTable::new();
    let lhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::float(2.0, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
        .expect_err("non-finite float exponent result should fail during fold");
    assert!(
        error
            .msg
            .contains("Compile-time float overflow or non-finite result")
    );
    assert!(error.msg.contains("'^'"));
}

#[test]
fn evaluate_operator_rejects_non_finite_float_multiply_result() {
    let mut string_table = StringTable::new();
    let lhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Multiply, &mut string_table)
        .expect_err("non-finite float multiply result should fail during fold");
    assert!(
        error
            .msg
            .contains("Compile-time float overflow or non-finite result")
    );
    assert!(error.msg.contains("'*'"));
}

#[test]
fn eval_int_cast_rejects_out_of_range_float() {
    let string_table = StringTable::new();
    let value = Expression::float(
        9_223_372_036_854_775_808.0,
        Default::default(),
        ValueMode::ImmutableOwned,
    );

    let error = eval_int_cast(&value, &string_table)
        .expect_err("out-of-range float to int cast should fail");
    assert!(error.contains("exceeds Int range"));
}

#[test]
fn eval_int_cast_rejects_non_finite_float() {
    let string_table = StringTable::new();
    let value = Expression::float(f64::INFINITY, Default::default(), ValueMode::ImmutableOwned);

    let error =
        eval_int_cast(&value, &string_table).expect_err("non-finite float to int cast should fail");
    assert!(error.contains("not finite"));
}

#[test]
fn eval_int_cast_rejects_non_integer_float() {
    let string_table = StringTable::new();
    let value = Expression::float(1.5, Default::default(), ValueMode::ImmutableOwned);

    let error = eval_int_cast(&value, &string_table)
        .expect_err("non-integer float to int cast should fail");
    assert!(error.contains("not an exact integer value"));
}

#[test]
fn eval_float_cast_rejects_non_finite_string_value() {
    let mut string_table = StringTable::new();
    let huge = format!("{}.0", "9".repeat(400));
    let value = Expression::string_slice(
        string_table.get_or_intern(huge),
        Default::default(),
        ValueMode::ImmutableOwned,
    );

    let error = eval_float_cast(&value, &string_table)
        .expect_err("non-finite float string cast should fail");
    assert!(error.contains("not finite"));
}

fn rvalue_node(expression: Expression) -> AstNode {
    AstNode {
        kind: NodeKind::Rvalue(expression),
        location: SourceLocation::default(),
        scope: InternedPath::new(),
    }
}

fn operator_node(operator: Operator) -> AstNode {
    AstNode {
        kind: NodeKind::Operator(operator),
        location: SourceLocation::default(),
        scope: InternedPath::new(),
    }
}

#[test]
fn constant_fold_folds_comparison_then_boolean_chain() {
    let mut string_table = StringTable::new();
    let nodes = vec![
        rvalue_node(Expression::int(
            1,
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        )),
        rvalue_node(Expression::int(
            2,
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::LessThan),
        rvalue_node(Expression::bool(
            true,
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::And),
    ];

    let folded = constant_fold(&nodes, &mut string_table).expect("folding should succeed");
    assert_eq!(folded.len(), 1);
    assert!(matches!(
        folded[0].kind,
        NodeKind::Rvalue(Expression {
            kind: ExpressionKind::Bool(true),
            ..
        })
    ));
}

#[test]
fn constant_fold_keeps_unary_not_when_operand_is_not_bool_literal() {
    let mut string_table = StringTable::new();
    let nodes = vec![
        rvalue_node(Expression::int(
            1,
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::Not),
    ];

    let folded = constant_fold(&nodes, &mut string_table).expect("folding should not error");
    assert_eq!(folded.len(), 2);
    assert!(matches!(
        folded[0].kind,
        NodeKind::Rvalue(Expression {
            kind: ExpressionKind::Int(1),
            ..
        })
    ));
    assert!(matches!(folded[1].kind, NodeKind::Operator(Operator::Not)));
}

#[test]
fn constant_fold_stays_conservative_with_runtime_operands() {
    let mut string_table = StringTable::new();
    let flag_name = InternedPath::from_single_str("flag", &mut string_table);
    let nodes = vec![
        rvalue_node(Expression::reference(
            flag_name,
            DataType::Bool,
            SourceLocation::default(),
            ValueMode::ImmutableReference,
        )),
        rvalue_node(Expression::bool(
            true,
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        )),
        operator_node(Operator::And),
    ];

    let folded =
        constant_fold(&nodes, &mut string_table).expect("runtime-dependent folding should succeed");
    assert_eq!(folded.len(), nodes.len());
    assert!(matches!(folded[2].kind, NodeKind::Operator(Operator::And)));
}
