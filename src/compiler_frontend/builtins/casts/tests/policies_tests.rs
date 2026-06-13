//! Policy unit tests for the builtin cast surface.
//!
//! WHAT: covers every builtin cast policy row, error code selection, and edge
//!      cases called out in the cast plan.
//! WHY: the policy owner is the single source of truth for cast rules. These
//!      tests pin the policy behaviour down so later phases can rely on it
//!      without re-deriving the expected outcomes in code.

use crate::compiler_frontend::builtins::casts::numeric_limits::{
    JS_SAFE_INTEGER_MAX, JS_SAFE_INTEGER_MIN,
};
use crate::compiler_frontend::builtins::casts::policies::{
    BuiltinCastLiteral, apply_builtin_cast_policy,
};
use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

#[test]
fn float_to_int_truncates_toward_zero() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(1.9),
    )
    .expect("1.9 should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(1));

    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(-1.9),
    )
    .expect("-1.9 should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(-1));
}

#[test]
fn float_to_int_rejects_non_finite_with_invalid_value_code() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(f64::NAN),
    )
    .expect_err("NaN should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntInvalidValue);

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(f64::INFINITY),
    )
    .expect_err("infinity should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntInvalidValue);
}

#[test]
fn float_to_int_rejects_out_of_safe_integer_range_with_out_of_range_code() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(9_223_372_036_854_775_808.0),
    )
    .expect_err("above JS safe integer range should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntOutOfRange);

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float((i64::MIN as f64) * 2.0),
    )
    .expect_err("below JS safe integer range should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntOutOfRange);
}

#[test]
fn float_to_int_accepts_safe_integer_max_boundary() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(JS_SAFE_INTEGER_MAX as f64),
    )
    .expect("exact JS safe integer max should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(JS_SAFE_INTEGER_MAX));
}

#[test]
fn float_to_int_accepts_safe_integer_min_boundary() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float(JS_SAFE_INTEGER_MIN as f64),
    )
    .expect("exact JS safe integer min should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(JS_SAFE_INTEGER_MIN));
}

#[test]
fn float_to_int_rejects_one_above_safe_integer_max() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float((JS_SAFE_INTEGER_MAX as f64) + 1.0),
    )
    .expect_err("one above JS safe integer max should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntOutOfRange);
}

#[test]
fn float_to_int_rejects_one_below_safe_integer_min() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToInt,
        &BuiltinCastLiteral::Float((JS_SAFE_INTEGER_MIN as f64) - 1.0),
    )
    .expect_err("one below JS safe integer min should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatCastToIntOutOfRange);
}

#[test]
fn int_to_char_accepts_valid_unicode_scalars() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::IntToChar,
        &BuiltinCastLiteral::Int(0x41),
    )
    .expect("'A' should fold");
    assert_eq!(result, BuiltinCastLiteral::Char('A'));
}

#[test]
fn int_to_char_rejects_negatives_with_invalid_codepoint_code() {
    let error =
        apply_builtin_cast_policy(BuiltinCastPolicyId::IntToChar, &BuiltinCastLiteral::Int(-1))
            .expect_err("negative codepoint should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntCastToCharInvalidCodepoint);
}

#[test]
fn int_to_char_rejects_surrogate_range_with_invalid_codepoint_code() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::IntToChar,
        &BuiltinCastLiteral::Int(0xD800),
    )
    .expect_err("surrogate codepoint should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntCastToCharInvalidCodepoint);
}

#[test]
fn int_to_char_rejects_above_max_scalar_with_invalid_codepoint_code() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::IntToChar,
        &BuiltinCastLiteral::Int(0x110000),
    )
    .expect_err("above max scalar should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntCastToCharInvalidCodepoint);
}

#[test]
fn char_to_int_returns_unicode_scalar_value() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::CharToInt,
        &BuiltinCastLiteral::Char('A'),
    )
    .expect("Char -> Int should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(0x41));
}

#[test]
fn string_to_int_is_strict_base_10_with_optional_sign() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String("  -42  ".to_string()),
    )
    .expect("trimmed signed integer should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(-42));

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String("3.14".to_string()),
    )
    .expect_err("decimal text should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntParseInvalidFormat);

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String("1_000".to_string()),
    )
    .expect_err("underscore-separated text should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntParseInvalidFormat);
}

#[test]
fn string_to_int_reports_overflow_as_out_of_range() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String("9223372036854775808".to_string()),
    )
    .expect_err("one above i64::MAX should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntParseOutOfRange);
}

#[test]
fn string_to_int_accepts_safe_integer_max_boundary() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String(JS_SAFE_INTEGER_MAX.to_string()),
    )
    .expect("JS safe integer max should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(JS_SAFE_INTEGER_MAX));
}

#[test]
fn string_to_int_rejects_one_above_safe_integer_max() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String((JS_SAFE_INTEGER_MAX + 1).to_string()),
    )
    .expect_err("one above JS safe integer max should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntParseOutOfRange);
}

#[test]
fn string_to_int_accepts_safe_integer_min_boundary() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String(JS_SAFE_INTEGER_MIN.to_string()),
    )
    .expect("JS safe integer min should fold");
    assert_eq!(result, BuiltinCastLiteral::Int(JS_SAFE_INTEGER_MIN));
}

#[test]
fn string_to_int_rejects_one_below_safe_integer_min() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToInt,
        &BuiltinCastLiteral::String((JS_SAFE_INTEGER_MIN - 1).to_string()),
    )
    .expect_err("one below JS safe integer min should fail");
    assert_eq!(error.code, BuiltinErrorCode::IntParseOutOfRange);
}

#[test]
fn string_to_float_rejects_nan_and_infinity() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToFloat,
        &BuiltinCastLiteral::String("NaN".to_string()),
    )
    .expect_err("NaN text should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatParseOutOfRange);

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToFloat,
        &BuiltinCastLiteral::String("Infinity".to_string()),
    )
    .expect_err("Infinity text should fail");
    assert_eq!(error.code, BuiltinErrorCode::FloatParseOutOfRange);
}

#[test]
fn string_to_float_parses_ordinary_decimal_text() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToFloat,
        &BuiltinCastLiteral::String(" 3.5e2 ".to_string()),
    )
    .expect("decimal exponent should fold");
    assert_eq!(result, BuiltinCastLiteral::Float(350.0));
}

#[test]
fn string_to_bool_accepts_only_lowercase_true_and_false() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToBool,
        &BuiltinCastLiteral::String(" true ".to_string()),
    )
    .expect("lowercase true should fold");
    assert_eq!(result, BuiltinCastLiteral::Bool(true));

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToBool,
        &BuiltinCastLiteral::String("TRUE".to_string()),
    )
    .expect_err("uppercase true should fail");
    assert_eq!(error.code, BuiltinErrorCode::StringParseBoolInvalidFormat);
}

#[test]
fn string_to_char_succeeds_only_for_single_scalar() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToChar,
        &BuiltinCastLiteral::String("A".to_string()),
    )
    .expect("single char should fold");
    assert_eq!(result, BuiltinCastLiteral::Char('A'));

    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToChar,
        &BuiltinCastLiteral::String("AB".to_string()),
    )
    .expect_err("multi-char string should fail");
    assert_eq!(error.code, BuiltinErrorCode::StringParseCharInvalidFormat);
}

#[test]
fn string_to_char_rejects_empty_with_invalid_format_code() {
    let error = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToChar,
        &BuiltinCastLiteral::String(String::new()),
    )
    .expect_err("empty string should fail");
    assert_eq!(error.code, BuiltinErrorCode::StringParseCharInvalidFormat);
}

#[test]
fn int_to_string_uses_signed_base_10() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::IntToString,
        &BuiltinCastLiteral::Int(-42),
    )
    .expect("Int -> String should fold");
    assert_eq!(result, BuiltinCastLiteral::String("-42".to_string()));
}

#[test]
fn float_to_string_uses_stable_decimal() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::FloatToString,
        &BuiltinCastLiteral::Float(1.5),
    )
    .expect("Float -> String should fold");
    assert_eq!(result, BuiltinCastLiteral::String("1.5".to_string()));
}

#[test]
fn bool_to_string_returns_true_or_false() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::BoolToString,
        &BuiltinCastLiteral::Bool(true),
    )
    .expect("Bool -> String should fold");
    assert_eq!(result, BuiltinCastLiteral::String("true".to_string()));
}

#[test]
fn char_to_string_returns_one_character_string() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::CharToString,
        &BuiltinCastLiteral::Char('Z'),
    )
    .expect("Char -> String should fold");
    assert_eq!(result, BuiltinCastLiteral::String("Z".to_string()));
}

#[test]
fn string_to_error_uses_text_as_message_and_default_code() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::StringToError,
        &BuiltinCastLiteral::String("Missing number".to_string()),
    )
    .expect("String -> Error should fold");

    assert_eq!(
        result,
        BuiltinCastLiteral::Error {
            message: "Missing number".to_string(),
            code: 0,
        }
    );
}

#[test]
fn error_to_string_returns_error_message_only() {
    let result = apply_builtin_cast_policy(
        BuiltinCastPolicyId::ErrorToString,
        &BuiltinCastLiteral::Error {
            message: "Missing number".to_string(),
            code: 200,
        },
    )
    .expect("Error -> String should fold");

    assert_eq!(
        result,
        BuiltinCastLiteral::String("Missing number".to_string())
    );
}

#[test]
fn const_foldable_policy_marker_excludes_error_materialization_policies() {
    assert!(!BuiltinCastPolicyId::StringToError.is_const_foldable());
    assert!(!BuiltinCastPolicyId::ErrorToString.is_const_foldable());
    assert!(BuiltinCastPolicyId::StringToInt.is_const_foldable());
    assert!(BuiltinCastPolicyId::IntToString.is_const_foldable());
}
