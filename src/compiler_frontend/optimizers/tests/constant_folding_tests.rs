//! Regression tests for constant-expression folding helpers.

use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Operator;
use crate::compiler_frontend::ast::expressions::expression_kind::ResolvedCastExpression;
use crate::compiler_frontend::ast::expressions::expression_types::{
    CastHandling, FallibleHandling, ResolvedCastEvidence,
};
use crate::compiler_frontend::ast::statements::value_production::ProducedValues;
use crate::compiler_frontend::builtins::casts::targets::{BuiltinCastPolicyId, BuiltinCastTarget};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, DiagnosticPayload, InvalidCastReason,
};
use crate::compiler_frontend::datatypes::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{GenericParameterId, TypeId};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId};

fn assert_compile_time_error(
    error: &ConstantFoldError,
    expected_reason: CompileTimeEvaluationErrorReason,
    expected_operation: Option<&str>,
    string_table: &StringTable,
) {
    let diagnostic = match error {
        ConstantFoldError::Diagnostic(diagnostic) => diagnostic,
        ConstantFoldError::Infrastructure(error) => {
            panic!("expected compile-time diagnostic, found infrastructure error: {error:?}")
        }
    };

    match &diagnostic.payload {
        DiagnosticPayload::CompileTimeEvaluationError { reason, operation } => {
            assert_eq!(*reason, expected_reason);

            let operation_text = operation.map(|operation| string_table.resolve(operation));
            assert_eq!(operation_text, expected_operation);
        }
        payload => panic!("expected compile-time evaluation payload, found {payload:?}"),
    }
}

fn assert_invalid_cast_error(error: &ConstantFoldError, expected_reason: InvalidCastReason) {
    let diagnostic = match error {
        ConstantFoldError::Diagnostic(diagnostic) => diagnostic,
        ConstantFoldError::Infrastructure(error) => {
            panic!("expected invalid-cast diagnostic, found infrastructure error: {error:?}")
        }
    };

    match &diagnostic.payload {
        DiagnosticPayload::InvalidCast { reason, .. } => {
            assert_eq!(*reason, expected_reason);
        }
        payload => panic!("expected invalid-cast payload, found {payload:?}"),
    }
}

fn cast_expression(
    source: Expression,
    target: BuiltinCastTarget,
    target_type_id: TypeId,
    evidence: ResolvedCastEvidence,
    handling: CastHandling,
    requires_optional_wrap_after_cast: bool,
    type_environment: &mut TypeEnvironment,
) -> Expression {
    let source_type_id = source.type_id;
    let location = source.location.clone();
    let cast = ResolvedCastExpression {
        source: Box::new(source),
        source_type_id,
        target_type_id,
        target,
        requires_optional_wrap_after_cast,
        evidence,
        handling,
        location,
    };

    let result_type_id = if requires_optional_wrap_after_cast {
        type_environment.intern_option(target_type_id)
    } else {
        target_type_id
    };

    Expression::cast(cast, result_type_id, type_environment)
}

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
    assert_eq!(result.diagnostic_type, DataType::Float);
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
    assert_eq!(result.diagnostic_type, DataType::Float);
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
    assert_eq!(result.diagnostic_type, DataType::Int);
}

#[test]
fn evaluate_operator_rejects_divide_by_zero_for_both_division_operators() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(5, Default::default(), ValueMode::ImmutableOwned);
    let zero = Expression::int(0, Default::default(), ValueMode::ImmutableOwned);

    let divide_error = lhs
        .evaluate_operator(&zero, &Operator::Divide, &mut string_table)
        .expect_err("regular division by zero should fail during fold");
    assert_compile_time_error(
        &divide_error,
        CompileTimeEvaluationErrorReason::DivideByZero,
        None,
        &string_table,
    );

    let int_divide_error = lhs
        .evaluate_operator(&zero, &Operator::IntDivide, &mut string_table)
        .expect_err("integer division by zero should fail during fold");
    assert_compile_time_error(
        &int_divide_error,
        CompileTimeEvaluationErrorReason::DivideByZero,
        None,
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_add_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MAX, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
        .expect_err("integer add overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("+"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_subtract_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Subtract, &mut string_table)
        .expect_err("integer subtract overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("-"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_multiply_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MAX, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Multiply, &mut string_table)
        .expect_err("integer multiply overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("*"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_exponent_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(2, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(63, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
        .expect_err("integer exponent overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("^"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_division_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::IntDivide, &mut string_table)
        .expect_err("integer division overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("//"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_integer_modulus_overflow() {
    let mut string_table = StringTable::new();
    let lhs = Expression::int(i64::MIN, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::int(-1, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Modulus, &mut string_table)
        .expect_err("integer modulus overflow should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some("%"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_non_finite_float_exponent_result() {
    let mut string_table = StringTable::new();
    let lhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::float(2.0, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
        .expect_err("non-finite float exponent result should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::FloatOverflow,
        Some("^"),
        &string_table,
    );
}

#[test]
fn evaluate_operator_rejects_non_finite_float_multiply_result() {
    let mut string_table = StringTable::new();
    let lhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);
    let rhs = Expression::float(1.0e308, Default::default(), ValueMode::ImmutableOwned);

    let error = lhs
        .evaluate_operator(&rhs, &Operator::Multiply, &mut string_table)
        .expect_err("non-finite float multiply result should fail during fold");
    assert_compile_time_error(
        &error,
        CompileTimeEvaluationErrorReason::FloatOverflow,
        Some("*"),
        &string_table,
    );
}

#[test]
fn fold_int_cast_rejects_out_of_range_float_with_dedicated_code() {
    use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
    use crate::compiler_frontend::builtins::casts::{
        BuiltinCastLiteral, apply_builtin_cast_policy,
    };
    use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

    let source = BuiltinCastLiteral::Float(9_223_372_036_854_775_808.0);
    let error = apply_builtin_cast_policy(BuiltinCastPolicyId::FloatToInt, &source)
        .expect_err("out-of-range float to int cast should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntOutOfRange);
}

#[test]
fn fold_int_cast_rejects_non_finite_float_with_dedicated_code() {
    use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
    use crate::compiler_frontend::builtins::casts::{
        BuiltinCastLiteral, apply_builtin_cast_policy,
    };
    use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

    let source = BuiltinCastLiteral::Float(f64::INFINITY);
    let error = apply_builtin_cast_policy(BuiltinCastPolicyId::FloatToInt, &source)
        .expect_err("non-finite float to int cast should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntInvalidValue);
}

#[test]
fn fold_int_cast_truncates_toward_zero() {
    use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
    use crate::compiler_frontend::builtins::casts::{
        BuiltinCastLiteral, apply_builtin_cast_policy,
    };

    let source = BuiltinCastLiteral::Float(1.9);
    let result = apply_builtin_cast_policy(BuiltinCastPolicyId::FloatToInt, &source)
        .expect("float to int cast should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(1));

    let source = BuiltinCastLiteral::Float(-1.9);
    let result = apply_builtin_cast_policy(BuiltinCastPolicyId::FloatToInt, &source)
        .expect("negative float to int cast should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(-1));
}

#[test]
fn fold_float_cast_rejects_non_finite_string_value() {
    use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
    use crate::compiler_frontend::builtins::casts::{
        BuiltinCastLiteral, apply_builtin_cast_policy,
    };
    use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

    let huge = format!("{}.0", "9".repeat(400));
    let source = BuiltinCastLiteral::String(huge);
    let error = apply_builtin_cast_policy(BuiltinCastPolicyId::StringToFloat, &source)
        .expect_err("non-finite float string cast should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatParseOutOfRange);
}

#[test]
fn fold_string_to_int_cast_uses_string_policy_row() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern(" 42 ".to_string());
    let source = Expression::string_slice(text, Default::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Propagate,
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("valid string to int cast should fold");

    assert_eq!(folded.type_id, target_type_id);
    assert!(matches!(folded.kind, ExpressionKind::Int(42)));
}

#[test]
fn fold_string_to_float_cast_uses_string_policy_row() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern(" 3.5e2 ".to_string());
    let source = Expression::string_slice(text, Default::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().float;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Float,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToFloat,
        },
        CastHandling::Propagate,
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("valid string to float cast should fold");

    assert_eq!(folded.type_id, target_type_id);
    assert!(matches!(folded.kind, ExpressionKind::Float(value) if value == 350.0));
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

    let folded = match constant_fold(&nodes, &mut string_table).expect("folding should succeed") {
        ConstantFoldResult::Folded(stack) => stack,
        ConstantFoldResult::Unchanged => panic!("expected folded result"),
    };
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

    let folded = match constant_fold(&nodes, &mut string_table).expect("folding should not error") {
        ConstantFoldResult::Folded(stack) => stack,
        ConstantFoldResult::Unchanged => panic!("expected folded result"),
    };
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

    match constant_fold(&nodes, &mut string_table)
        .expect("runtime-dependent folding should succeed")
    {
        ConstantFoldResult::Unchanged => {}
        ConstantFoldResult::Folded(_) => panic!("expected unchanged result for runtime operands"),
    }
}

#[test]
fn constant_fold_unchanged_reuses_original_stack() {
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

    let result = constant_fold(&nodes, &mut string_table).expect("folding should succeed");

    match result {
        ConstantFoldResult::Unchanged => {
            // The original nodes slice length should match what we passed in.
            assert_eq!(nodes.len(), 3);
        }
        ConstantFoldResult::Folded(_) => {
            panic!("expected Unchanged for runtime-dependent operands")
        }
    }
}

#[test]
fn fold_cast_infallible_int_to_string_folds_to_string_literal() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let source = Expression::int(42, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().string;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::String,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::IntToString,
        },
        CastHandling::Infallible,
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("infallible builtin cast should fold");

    assert_eq!(folded.type_id, target_type_id);

    let ExpressionKind::StringSlice(interned) = folded.kind else {
        panic!("expected folded Int -> String cast to produce a string slice");
    };

    assert_eq!(string_table.resolve(interned), "42");
}

#[test]
fn fold_cast_optional_wrap_coerces_value_to_optional() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let source = Expression::int(7, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().string;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::String,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::IntToString,
        },
        CastHandling::Infallible,
        true,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("optional-wrapped infallible cast should fold");

    assert_eq!(
        folded.type_id,
        type_environment.intern_option(target_type_id)
    );

    let ExpressionKind::Coerced { value, .. } = folded.kind else {
        panic!("expected optional-wrapped cast to produce a Coerced expression");
    };

    let ExpressionKind::StringSlice(interned) = value.kind else {
        panic!("expected coerced inner value to be a string slice");
    };

    assert_eq!(string_table.resolve(interned), "7");
}

#[test]
fn fold_cast_fallible_string_to_int_success_folds_to_int() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("123".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Propagate,
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("successful fallible builtin cast should fold");

    assert_eq!(folded.type_id, target_type_id);
    assert!(matches!(folded.kind, ExpressionKind::Int(123)));
}

#[test]
fn fold_cast_fallible_string_to_int_failure_reports_builtin_cast_failed_in_const() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("not a number".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Propagate,
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("failed fallible builtin cast should report a const diagnostic");

    assert_invalid_cast_error(&error, InvalidCastReason::BuiltinCastFailedInConst);
}

#[test]
fn fold_cast_user_defined_evidence_rejected_in_const_context() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let source = Expression::int(42, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().string;
    let method_path = InternedPath::from_single_str("to_string", &mut string_table);

    let cast = cast_expression(
        source,
        BuiltinCastTarget::String,
        target_type_id,
        ResolvedCastEvidence::UserDefined {
            evidence_id: TraitEvidenceId(0),
            method_path,
        },
        CastHandling::Infallible,
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("user-defined evidence should not fold in a const context");

    assert_invalid_cast_error(
        &error,
        InvalidCastReason::UserDefinedEvidenceNotConstFoldable,
    );
}

#[test]
fn fold_cast_generic_bound_evidence_rejected_in_const_context() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let source = Expression::int(42, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().string;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::String,
        target_type_id,
        ResolvedCastEvidence::GenericBound {
            trait_id: TraitId(0),
            parameter_id: GenericParameterId(0),
        },
        CastHandling::Infallible,
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("generic-bound evidence should not fold in a const context");

    assert_invalid_cast_error(
        &error,
        InvalidCastReason::GenericBoundEvidenceNotConstFoldable,
    );
}

fn catch_handler_body(value: Expression) -> Vec<AstNode> {
    let location = value.location.clone();

    vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: vec![value],
            location: location.clone(),
        }),
        location,
        scope: InternedPath::new(),
    }]
}

#[test]
fn fold_cast_fallible_builtin_failure_with_catch_folds_to_handler_value() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("nope".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;
    let handler_value = Expression::int(0, SourceLocation::default(), ValueMode::ImmutableOwned);

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Recover(FallibleHandling::Handler {
            error: None,
            body: catch_handler_body(handler_value),
        }),
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("failed builtin cast with foldable catch handler should fold to handler value");

    assert_eq!(folded.type_id, target_type_id);
    assert!(matches!(folded.kind, ExpressionKind::Int(0)));
}

#[test]
fn fold_cast_fallible_builtin_success_with_catch_ignores_handler() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("123".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;
    let handler_value = Expression::int(999, SourceLocation::default(), ValueMode::ImmutableOwned);

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Recover(FallibleHandling::Handler {
            error: None,
            body: catch_handler_body(handler_value),
        }),
        false,
        &mut type_environment,
    );

    let folded = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect("successful builtin cast should fold to success value even with catch handler");

    assert_eq!(folded.type_id, target_type_id);
    assert!(matches!(folded.kind, ExpressionKind::Int(123)));
}

#[test]
fn fold_cast_fallible_builtin_failure_with_non_foldable_catch_rejects_handler() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("nope".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;

    let handler_value = Expression::reference(
        InternedPath::from_single_str("runtime_value", &mut string_table),
        DataType::Int,
        SourceLocation::default(),
        ValueMode::ImmutableReference,
    );

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Recover(FallibleHandling::Handler {
            error: None,
            body: catch_handler_body(handler_value),
        }),
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("non-foldable catch handler should be rejected in const context");

    assert_invalid_cast_error(&error, InvalidCastReason::CatchHandlerNotConstFoldable);
}

#[test]
fn fold_cast_fallible_builtin_failure_with_empty_catch_rejects_handler() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("nope".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Recover(FallibleHandling::Handler {
            error: None,
            body: Vec::new(),
        }),
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("empty catch handler should be rejected in const context");

    assert_invalid_cast_error(&error, InvalidCastReason::CatchHandlerNotConstFoldable);
}

#[test]
fn fold_cast_fallible_builtin_failure_with_branching_catch_rejects_handler() {
    let mut string_table = StringTable::new();
    let mut type_environment = TypeEnvironment::new();
    let text = string_table.get_or_intern("nope".to_string());
    let source =
        Expression::string_slice(text, SourceLocation::default(), ValueMode::ImmutableOwned);
    let target_type_id = type_environment.builtins().int;
    let location = SourceLocation::default();

    let then_body = catch_handler_body(Expression::int(
        1,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    ));
    let else_body = catch_handler_body(Expression::int(
        2,
        SourceLocation::default(),
        ValueMode::ImmutableOwned,
    ));
    let branching_handler = vec![AstNode {
        kind: NodeKind::If(
            Expression::bool(false, location.clone(), ValueMode::ImmutableOwned),
            then_body,
            Some(else_body),
        ),
        location,
        scope: InternedPath::new(),
    }];

    let cast = cast_expression(
        source,
        BuiltinCastTarget::Int,
        target_type_id,
        ResolvedCastEvidence::Builtin {
            policy: BuiltinCastPolicyId::StringToInt,
        },
        CastHandling::Recover(FallibleHandling::Handler {
            error: None,
            body: branching_handler,
        }),
        false,
        &mut type_environment,
    );

    let error = fold_compile_time_expression(&cast, &mut string_table, true)
        .expect_err("branching catch handler needs real const statement evaluation");

    assert_invalid_cast_error(&error, InvalidCastReason::CatchHandlerNotConstFoldable);
}
