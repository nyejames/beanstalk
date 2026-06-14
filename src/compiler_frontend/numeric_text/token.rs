//! Numeric literal token payload.
//!
//! WHAT: carries the lexical metadata extracted from a numeric literal without
//!       committing to a runtime representation such as `i32` or `f64`.
//! WHY: separating lexical classification from semantic materialization lets the
//!      tokenizer own source-shape facts while AST/config consumers decide how to
//!      interpret them.

use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};

/// Lexical classification of a numeric literal token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericLiteralKind {
    /// A whole-number literal such as `42` or `1_000`.
    WholeNumber,
    /// A decimal-point literal such as `3.14`.
    DecimalPoint,
    /// An exponent literal such as `1e6` or `1.0e-21`.
    Exponent,
}

/// Sign attached to a numeric literal token.
///
/// WHY: the tokenizer front-loads attached `-` for numeric literals, while normalized text stays
/// unsigned so materialization can apply range checks with the sign as explicit metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericLiteralSign {
    Positive,
    Negative,
}

/// Sign explicitly written on an exponent, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericExponentSign {
    None,
    Positive,
    Negative,
}

/// The tokenizer's numeric-literal payload.
///
/// `normalized_text` is unsigned, underscore-free, and uses lowercase `e` for
/// exponents. It preserves explicit exponent signs (`1e+21`) so that later
/// materialization can reconstruct the value from text alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumericLiteralToken {
    pub sign: NumericLiteralSign,
    pub normalized_text: StringId,
    pub kind: NumericLiteralKind,
    pub digit_count: u32,
    pub fractional_digit_count: u32,
    pub exponent_digit_count: u32,
    pub exponent_sign: NumericExponentSign,
}

impl NumericLiteralToken {
    pub fn new(
        sign: NumericLiteralSign,
        normalized_text: StringId,
        kind: NumericLiteralKind,
        digit_count: u32,
        fractional_digit_count: u32,
        exponent_digit_count: u32,
        exponent_sign: NumericExponentSign,
    ) -> Self {
        Self {
            sign,
            normalized_text,
            kind,
            digit_count,
            fractional_digit_count,
            exponent_digit_count,
            exponent_sign,
        }
    }

    /// Remap the interned normalized text after a string-table merge.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.normalized_text = remap.get(self.normalized_text);
    }

    /// Build a test numeric token from a valid source snippet.
    ///
    /// WHY: unit tests across the frontend need a concise way to construct
    ///      `TokenKind::NumericLiteral` payloads without hand-assembling counts.
    #[cfg(test)]
    pub fn test_new(
        source: &str,
        string_table: &mut crate::compiler_frontend::symbols::string_interning::StringTable,
    ) -> Self {
        use crate::compiler_frontend::numeric_text::parse::parse_numeric_literal;

        let parsed = parse_numeric_literal(source).unwrap_or_else(|reason| {
            panic!("test numeric literal '{source}' is invalid: {reason:?}")
        });
        let normalized_text = string_table.intern(&parsed.normalized_text);

        Self::new(
            NumericLiteralSign::Positive,
            normalized_text,
            parsed.kind,
            parsed.digit_count,
            parsed.fractional_digit_count,
            parsed.exponent_digit_count,
            parsed.exponent_sign,
        )
    }
}
