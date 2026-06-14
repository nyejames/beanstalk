//! Numeric text grammar tests.
//!
//! WHAT: validates the shared parser before tokens are materialized by AST or future casts.
//! WHY: separator, exponent, and normalization rules need one owner so tokenizer and string casts
//!      cannot drift into subtly different grammars.

use super::*;
use crate::compiler_frontend::compiler_messages::NumberLiteralErrorReason;
use crate::compiler_frontend::numeric_text::token::{
    NumericExponentSign, NumericLiteralKind, NumericLiteralSign, NumericLiteralToken,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[test]
fn parses_lowercase_exponent_metadata() {
    let parsed = parse_numeric_literal("1_200.50e-3").expect("literal should parse");

    assert_eq!(parsed.normalized_text, "1200.50e-3");
    assert_eq!(parsed.kind, NumericLiteralKind::Exponent);
    assert_eq!(parsed.digit_count, 7);
    assert_eq!(parsed.fractional_digit_count, 2);
    assert_eq!(parsed.exponent_digit_count, 1);
    assert_eq!(parsed.exponent_sign, NumericExponentSign::Negative);
}

#[test]
fn rejects_missing_exponent_digits() {
    for source in ["1e", "1e+", "1e-"] {
        let reason = parse_numeric_literal(source).expect_err("literal should be rejected");
        assert_eq!(
            reason,
            NumberLiteralErrorReason::MissingExponentDigits,
            "{source}"
        );
    }
}

#[test]
fn rejects_bad_separator_placement() {
    let cases = [
        ("_1", NumberLiteralErrorReason::InvalidSeparatorPlacement),
        ("1_", NumberLiteralErrorReason::EndsWithSeparator),
        ("1__0", NumberLiteralErrorReason::InvalidSeparatorPlacement),
        ("1_e2", NumberLiteralErrorReason::InvalidSeparatorPlacement),
        ("1e_2", NumberLiteralErrorReason::InvalidSeparatorPlacement),
        ("1e+_2", NumberLiteralErrorReason::InvalidSeparatorPlacement),
    ];

    for (source, expected_reason) in cases {
        let reason = parse_numeric_literal(source).expect_err("literal should be rejected");
        assert_eq!(reason, expected_reason, "{source}");
    }
}

#[test]
fn materialize_i32_accepts_boundary_values() {
    let mut string_table = StringTable::new();

    let max_token = whole_number_token(
        "2147483647",
        NumericLiteralSign::Positive,
        &mut string_table,
    );
    assert_eq!(
        materialize_i32(&max_token, &string_table).unwrap(),
        i32::MAX
    );

    let min_token = whole_number_token(
        "2147483648",
        NumericLiteralSign::Negative,
        &mut string_table,
    );
    assert_eq!(
        materialize_i32(&min_token, &string_table).unwrap(),
        i32::MIN
    );
}

#[test]
fn materialize_i32_rejects_out_of_range_values() {
    let mut string_table = StringTable::new();

    let too_large = whole_number_token(
        "2147483648",
        NumericLiteralSign::Positive,
        &mut string_table,
    );
    assert_eq!(
        materialize_i32(&too_large, &string_table).unwrap_err(),
        NumberLiteralErrorReason::OutsideIntRange
    );

    let too_negative = whole_number_token(
        "2147483649",
        NumericLiteralSign::Negative,
        &mut string_table,
    );
    assert_eq!(
        materialize_i32(&too_negative, &string_table).unwrap_err(),
        NumberLiteralErrorReason::OutsideIntRange
    );
}

#[test]
fn materialize_i32_with_sign_allows_negative_fallback_boundary() {
    let mut string_table = StringTable::new();

    let positive_token = whole_number_token(
        "2147483648",
        NumericLiteralSign::Positive,
        &mut string_table,
    );
    assert_eq!(
        materialize_i32_with_sign(&positive_token, NumericLiteralSign::Negative, &string_table)
            .unwrap(),
        i32::MIN
    );
}

#[test]
fn parse_numeric_text_to_i32_accepts_signed_boundaries_and_separators() {
    assert_eq!(
        parse_numeric_text_to_i32("2_147_483_647").unwrap(),
        i32::MAX
    );
    assert_eq!(parse_numeric_text_to_i32("-2147483648").unwrap(), i32::MIN);
}

#[test]
fn parse_numeric_text_to_i32_rejects_out_of_range_values() {
    assert_eq!(
        parse_numeric_text_to_i32("2147483648").unwrap_err(),
        NumberLiteralErrorReason::OutsideIntRange
    );
    assert_eq!(
        parse_numeric_text_to_i32("-2147483649").unwrap_err(),
        NumberLiteralErrorReason::OutsideIntRange
    );
}

#[test]
fn parse_numeric_text_to_i32_rejects_non_whole_or_invalid_text() {
    for source in ["1.0", "1e3", "+1", " 1", "1 ", ""] {
        let reason = parse_numeric_text_to_i32(source).expect_err(source);
        assert_ne!(
            reason,
            NumberLiteralErrorReason::OutsideIntRange,
            "{source}"
        );
    }
}

#[test]
fn materialize_f64_rejects_non_finite_values() {
    let mut string_table = StringTable::new();

    let token = NumericLiteralToken::new(
        NumericLiteralSign::Positive,
        string_table.intern("1e309"),
        NumericLiteralKind::Exponent,
        2,
        0,
        3,
        NumericExponentSign::Positive,
    );

    assert_eq!(
        materialize_f64(&token, &string_table).unwrap_err(),
        NumberLiteralErrorReason::NonFiniteFloat
    );
}

#[test]
fn materialize_f64_applies_negative_sign() {
    let mut string_table = StringTable::new();

    let token = NumericLiteralToken::new(
        NumericLiteralSign::Negative,
        string_table.intern("1.5e2"),
        NumericLiteralKind::Exponent,
        3,
        1,
        2,
        NumericExponentSign::Positive,
    );

    assert_eq!(materialize_f64(&token, &string_table).unwrap(), -150.0);
}

#[test]
fn parse_numeric_text_to_f64_accepts_float_grammar_cases() {
    let cases = [
        ("1", 1.0),
        ("1.5", 1.5),
        ("-1.5", -1.5),
        ("1e6", 1e6),
        ("1e+21", 1e21),
        ("1e-6", 1e-6),
        ("1_000.5e-2", 1000.5e-2),
    ];

    for (source, expected) in cases {
        let result = parse_numeric_text_to_f64(source).expect(source);
        assert_eq!(result, expected, "{source}");
    }
}

#[test]
fn parse_numeric_text_to_f64_rejects_invalid_grammar_cases() {
    let invalid_cases = [
        "1E6",
        "NaN",
        "Infinity",
        "-Infinity",
        "+1.0",
        " 1.0",
        "1.0 ",
        ".5",
        "1.",
        "1e",
        "1e+",
        "1__0",
        "1e_2",
    ];

    for source in invalid_cases {
        let reason = parse_numeric_text_to_f64(source).expect_err(source);
        assert!(
            !matches!(reason, NumberLiteralErrorReason::NonFiniteFloat),
            "{source} should fail as invalid grammar, not as non-finite"
        );
    }
}

#[test]
fn parse_numeric_text_to_f64_rejects_non_finite_materialization() {
    let reason = parse_numeric_text_to_f64("1e10000").expect_err("should be non-finite");
    assert_eq!(reason, NumberLiteralErrorReason::NonFiniteFloat);
}

fn whole_number_token(
    text: &str,
    sign: NumericLiteralSign,
    string_table: &mut StringTable,
) -> NumericLiteralToken {
    let normalized_text = string_table.intern(text);

    NumericLiteralToken::new(
        sign,
        normalized_text,
        NumericLiteralKind::WholeNumber,
        text.chars().filter(|c| c.is_ascii_digit()).count() as u32,
        0,
        0,
        NumericExponentSign::None,
    )
}
