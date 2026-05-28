//! Numeric literal tokenization.
//!
//! WHAT: consumes integer and float literal text and validates separator/decimal
//! syntax before producing literal tokens.
//! WHY: keeping numeric rules outside the main lexer dispatch makes keyword,
//! delimiter, and template-mode routing easier to audit.

use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, NumberLiteralErrorReason};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::return_token;

/// Tokenize an integer or float literal starting with `first_digit`.
///
/// WHAT: consumes digits, optional `_` separators, and at most one `.` for floats.
/// WHY: keeps the main dispatch function focused on routing while centralizing
/// numeric literal rules.
///
/// NOTE: Error paths intern the partial literal text so the diagnostic can cite the
/// exact malformed token. This is diagnostic-only string-table mutation.
pub(super) fn tokenize_numeric_literal(
    first_digit: char,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
) -> Result<Token, CompilerDiagnostic> {
    let mut token_value = String::new();
    token_value.push(first_digit);

    let mut has_decimal_point = false;
    let mut saw_digit_after_decimal = false;
    let mut last_segment_was_digit = true;

    while let Some(&next_char) = stream.peek() {
        if next_char == '_' {
            if !last_segment_was_digit {
                let literal_text = string_table.intern(&token_value);
                return Err(CompilerDiagnostic::invalid_number_literal(
                    literal_text,
                    NumberLiteralErrorReason::SeparatorNotBetweenDigits,
                    stream.new_location(),
                ));
            }

            let _ = stream.next();
            last_segment_was_digit = false;
            continue;
        }

        if next_char == '.' {
            if has_decimal_point {
                let literal_text = string_table.intern(&token_value);
                return Err(CompilerDiagnostic::invalid_number_literal(
                    literal_text,
                    NumberLiteralErrorReason::MultipleDecimalPoints,
                    stream.new_location(),
                ));
            }

            if !last_segment_was_digit {
                let literal_text = string_table.intern(&token_value);
                return Err(CompilerDiagnostic::invalid_number_literal(
                    literal_text,
                    NumberLiteralErrorReason::DecimalPointNotAfterDigit,
                    stream.new_location(),
                ));
            }

            has_decimal_point = true;
            last_segment_was_digit = false;

            let dot = advance_after_peek(
                stream,
                "Tokenizer peeked a decimal point but could not advance the stream.",
            );
            token_value.push(dot);
            continue;
        }

        if next_char.is_numeric() {
            let digit = advance_after_peek(
                stream,
                "Tokenizer peeked a numeric character but could not advance the stream.",
            );
            token_value.push(digit);
            last_segment_was_digit = true;

            if has_decimal_point {
                saw_digit_after_decimal = true;
            }
        } else {
            break;
        }
    }

    if !last_segment_was_digit {
        let literal_text = string_table.intern(&token_value);
        return Err(CompilerDiagnostic::invalid_number_literal(
            literal_text,
            NumberLiteralErrorReason::EndsWithSeparator,
            stream.new_location(),
        ));
    }

    if has_decimal_point && !saw_digit_after_decimal {
        let literal_text = string_table.intern(&token_value);
        return Err(CompilerDiagnostic::invalid_number_literal(
            literal_text,
            NumberLiteralErrorReason::MissingFractionalDigits,
            stream.new_location(),
        ));
    }

    if !has_decimal_point {
        let parsed_value = token_value.parse::<i64>().map_err(|_error| {
            let literal_text = string_table.intern(&token_value);
            CompilerDiagnostic::invalid_number_literal(
                literal_text,
                NumberLiteralErrorReason::ParseOverflow,
                stream.new_location(),
            )
        })?;
        return_token!(TokenKind::IntLiteral(parsed_value), stream);
    }

    let parsed_value = token_value.parse::<f64>().map_err(|_error| {
        let literal_text = string_table.intern(&token_value);
        CompilerDiagnostic::invalid_number_literal(
            literal_text,
            NumberLiteralErrorReason::ParseOverflow,
            stream.new_location(),
        )
    })?;

    if !parsed_value.is_finite() {
        let literal_text = string_table.intern(&token_value);
        return Err(CompilerDiagnostic::invalid_number_literal(
            literal_text,
            NumberLiteralErrorReason::ParseOverflow,
            stream.new_location(),
        ));
    }

    return_token!(TokenKind::FloatLiteral(parsed_value), stream);
}

/// Advance after a successful `peek` in tokenizer loops.
///
/// WHAT: numeric tokenization inspects the next character before consuming it.
/// Once `peek` has returned `Some`, `next` returning `None` means the stream
/// invariant is broken, not that user source is malformed.
fn advance_after_peek(stream: &mut TokenStream<'_>, invariant_message: &'static str) -> char {
    stream.next().expect(invariant_message)
}
