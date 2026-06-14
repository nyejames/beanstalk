//! Numeric literal tokenization.
//!
//! WHAT: consumes numeric literal text and delegates grammar validation to the shared
//!       `numeric_text` module.
//! WHY: keeping the grammar in one place makes separator, exponent, and sign rules
//!      consistent between source literals and future string casts.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::NumberLiteralErrorReason;
use crate::compiler_frontend::numeric_text::parse::parse_numeric_literal;
use crate::compiler_frontend::numeric_text::token::{NumericLiteralSign, NumericLiteralToken};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::return_token;

/// Tokenize an integer or float literal starting with `first_digit`.
///
/// WHAT: consumes the full literal run, then asks `numeric_text` to validate it and
///       build the lexical token payload.
/// WHY: the tokenizer owns source-location tracking, while `numeric_text` owns the
///      grammar; this boundary avoids leaking stream state into the grammar module.
pub(super) fn tokenize_numeric_literal(
    first_digit: char,
    stream: &mut TokenStream<'_>,
    string_table: &mut StringTable,
    sign: NumericLiteralSign,
) -> Result<Token, CompilerDiagnostic> {
    let mut literal_text = String::new();
    literal_text.push(first_digit);

    consume_digit_run(&mut literal_text, stream);

    if stream.peek() == Some(&'.') {
        let decimal_point =
            stream.advance_after_peek("Tokenizer peeked a decimal point but could not advance.");
        literal_text.push(decimal_point);
        let fractional_digits = consume_digit_run(&mut literal_text, stream);

        if fractional_digits > 0 && stream.peek() == Some(&'.') {
            let literal_text_id = string_table.intern(&literal_text);
            return Err(CompilerDiagnostic::invalid_number_literal(
                literal_text_id,
                NumberLiteralErrorReason::MultipleDecimalPoints,
                stream.new_location(),
            ));
        }
    }

    if let Some(&character) = stream.peek()
        && (character == 'e' || character == 'E')
    {
        let exponent_marker =
            stream.advance_after_peek("Tokenizer peeked an exponent marker but could not advance.");
        literal_text.push(exponent_marker);

        if let Some(&sign_character) = stream.peek()
            && (sign_character == '+' || sign_character == '-')
        {
            let exponent_sign = stream
                .advance_after_peek("Tokenizer peeked an exponent sign but could not advance.");
            literal_text.push(exponent_sign);
        }

        consume_digit_run(&mut literal_text, stream);
    }

    match parse_numeric_literal(&literal_text) {
        Ok(parsed) => {
            let normalized_text = string_table.intern(&parsed.normalized_text);
            let token = NumericLiteralToken::new(
                sign,
                normalized_text,
                parsed.kind,
                parsed.digit_count,
                parsed.fractional_digit_count,
                parsed.exponent_digit_count,
                parsed.exponent_sign,
            );
            return_token!(TokenKind::NumericLiteral(token), stream);
        }

        Err(reason) => {
            let literal_text_id = string_table.intern(&literal_text);
            Err(CompilerDiagnostic::invalid_number_literal(
                literal_text_id,
                reason,
                stream.new_location(),
            ))
        }
    }
}

/// Consume a run of digits and digit separators from the stream.
///
/// WHY: the scanner needs to know how far the literal extends before the grammar
///      validates separator placement; this helper centralizes that simple loop.
fn consume_digit_run(literal_text: &mut String, stream: &mut TokenStream<'_>) -> u32 {
    let mut digit_count = 0;

    while let Some(&character) = stream.peek() {
        if character.is_ascii_digit() || character == '_' {
            let digit_or_separator = stream
                .advance_after_peek("Tokenizer peeked a digit or separator but could not advance.");
            if digit_or_separator.is_ascii_digit() {
                digit_count += 1;
            }
            literal_text.push(digit_or_separator);
        } else {
            break;
        }
    }

    digit_count
}
