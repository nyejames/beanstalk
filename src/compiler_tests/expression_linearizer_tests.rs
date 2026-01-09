//! Unit tests for ExpressionLinearizer
//!
//! Tests for expression linearization, temporary allocation, and operator conversion.

use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::hir::build_hir::HirBuilderContext;
use crate::compiler::hir::expression_linearizer::ExpressionLinearizer;
use crate::compiler::hir::nodes::{BinOp, HirExpr, HirExprKind};
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

/// Helper to create a test expression
fn create_test_int_expr(value: i64) -> Expression {
    Expression::int(value, TextLocation::default(), Ownership::default())
}

/// Helper to create a test float expression
fn create_test_float_expr(value: f64) -> Expression {
    Expression::float(value, TextLocation::default(), Ownership::default())
}

/// Helper to create a test bool expression
fn create_test_bool_expr(value: bool) -> Expression {
    Expression::bool(value, TextLocation::default(), Ownership::default())
}

#[test]
fn test_linearize_int_literal() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut linearizer = ExpressionLinearizer::new();

    let expr = create_test_int_expr(42);
    let (nodes, result) = linearizer.linearize_expression(&expr, &mut ctx).unwrap();

    assert!(
        nodes.is_empty(),
        "Int literal should produce no intermediate nodes"
    );
    assert!(matches!(result.kind, HirExprKind::Int(42)));
    assert!(matches!(result.data_type, DataType::Int));
}

#[test]
fn test_linearize_float_literal() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut linearizer = ExpressionLinearizer::new();

    let expr = create_test_float_expr(3.14);
    let (nodes, result) = linearizer.linearize_expression(&expr, &mut ctx).unwrap();

    assert!(
        nodes.is_empty(),
        "Float literal should produce no intermediate nodes"
    );
    if let HirExprKind::Float(val) = result.kind {
        assert!((val - 3.14).abs() < f64::EPSILON);
    } else {
        panic!("Expected Float expression");
    }
    assert!(matches!(result.data_type, DataType::Float));
}

#[test]
fn test_linearize_bool_literal() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut linearizer = ExpressionLinearizer::new();

    let expr = create_test_bool_expr(true);
    let (nodes, result) = linearizer.linearize_expression(&expr, &mut ctx).unwrap();

    assert!(
        nodes.is_empty(),
        "Bool literal should produce no intermediate nodes"
    );
    assert!(matches!(result.kind, HirExprKind::Bool(true)));
    assert!(matches!(result.data_type, DataType::Bool));
}

#[test]
fn test_allocate_compiler_local() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut linearizer = ExpressionLinearizer::new();

    let temp1 =
        linearizer.allocate_compiler_local(DataType::Int, TextLocation::default(), &mut ctx);
    let temp2 =
        linearizer.allocate_compiler_local(DataType::Float, TextLocation::default(), &mut ctx);

    // Temporaries should have unique names
    assert_ne!(temp1, temp2);

    // Both should be tracked as compiler locals
    assert!(linearizer.is_compiler_local(&temp1));
    assert!(linearizer.is_compiler_local(&temp2));

    // Types should be correct
    assert!(matches!(
        linearizer.get_compiler_local_type(&temp1),
        Some(DataType::Int)
    ));
    assert!(matches!(
        linearizer.get_compiler_local_type(&temp2),
        Some(DataType::Float)
    ));
}

#[test]
fn test_create_temporary_with_value() {
    let mut string_table = StringTable::new();
    let mut ctx = HirBuilderContext::new(&mut string_table);
    let mut linearizer = ExpressionLinearizer::new();

    let value = HirExpr {
        kind: HirExprKind::Int(42),
        data_type: DataType::Int,
        location: TextLocation::default(),
    };

    let (nodes, load_expr) = linearizer.create_temporary_with_value(value, &mut ctx);

    // Should produce one assignment node
    assert_eq!(nodes.len(), 1);

    // The result should be a load expression
    assert!(matches!(load_expr.kind, HirExprKind::Load(_)));
}

#[test]
fn test_operator_conversion() {
    let linearizer = ExpressionLinearizer::new();

    assert!(matches!(
        linearizer.convert_operator(&Operator::Add),
        Ok(BinOp::Add)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::Subtract),
        Ok(BinOp::Sub)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::Multiply),
        Ok(BinOp::Mul)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::Divide),
        Ok(BinOp::Div)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::Equality),
        Ok(BinOp::Eq)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::And),
        Ok(BinOp::And)
    ));
    assert!(matches!(
        linearizer.convert_operator(&Operator::Or),
        Ok(BinOp::Or)
    ));
}

#[test]
fn test_binop_type_inference() {
    let linearizer = ExpressionLinearizer::new();

    // Comparison operators return bool
    assert!(matches!(
        linearizer.infer_binop_type(&DataType::Int, &DataType::Int, &BinOp::Eq),
        DataType::Bool
    ));
    assert!(matches!(
        linearizer.infer_binop_type(&DataType::Float, &DataType::Float, &BinOp::Lt),
        DataType::Bool
    ));

    // Arithmetic with floats returns float
    assert!(matches!(
        linearizer.infer_binop_type(&DataType::Int, &DataType::Float, &BinOp::Add),
        DataType::Float
    ));
    assert!(matches!(
        linearizer.infer_binop_type(&DataType::Float, &DataType::Int, &BinOp::Mul),
        DataType::Float
    ));

    // Arithmetic with ints returns int
    assert!(matches!(
        linearizer.infer_binop_type(&DataType::Int, &DataType::Int, &BinOp::Add),
        DataType::Int
    ));
}
