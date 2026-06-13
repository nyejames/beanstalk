//! Pure builtin cast policy implementations.
//!
//! WHAT: implements every initial builtin evidence row from the cast plan as a
//!      pure function over a `BuiltinCastLiteral` input. The helpers return
//!      either a folded `BuiltinCastLiteral` value or a `BuiltinCastError` that
//!      carries the stable `BuiltinErrorCode` so diagnostic and runtime layers
//!      can render the same code path.
//! WHY: the policy owner is the single source of truth for the actual rules.
//!      The constant folder and later backend phases can ask the policy owner
//!      for the same answer instead of duplicating per-cast ad hoc match logic.

use crate::compiler_frontend::builtins::casts::numeric_limits::int_is_alpha_runtime_safe;
use crate::compiler_frontend::builtins::casts::targets::BuiltinCastPolicyId;
use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;
use std::num::IntErrorKind;

/// A literal scalar value in policy space.
///
/// WHAT: policies operate on this narrow type so they do not depend on the
///      parser, AST, HIR, or runtime representation. Later phases will convert
///      their native expressions into this shape before calling the policy.
/// WHY: keeping policies pure and side-effect free allows sharing between the
///      constant folder and later backends without depending on `Expression`
///      or backend-specific types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BuiltinCastLiteral {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Char(char),
    Error { message: String, code: i64 },
}

impl BuiltinCastLiteral {
    /// Returns the type tag for a literal, used by policy diagnostics.
    fn type_name(&self) -> &'static str {
        match self {
            BuiltinCastLiteral::Bool(_) => "Bool",
            BuiltinCastLiteral::Int(_) => "Int",
            BuiltinCastLiteral::Float(_) => "Float",
            BuiltinCastLiteral::String(_) => "String",
            BuiltinCastLiteral::Char(_) => "Char",
            BuiltinCastLiteral::Error { .. } => "Error",
        }
    }
}

/// A single cast failure reported by a policy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BuiltinCastError {
    pub(crate) code: BuiltinErrorCode,
    pub(crate) message: String,
}

impl BuiltinCastError {
    fn new(code: BuiltinErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Dispatches a builtin policy by id.
pub(crate) fn apply_builtin_cast_policy(
    policy: BuiltinCastPolicyId,
    source: &BuiltinCastLiteral,
) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    match policy {
        BuiltinCastPolicyId::IntToFloat => int_to_float(source),
        BuiltinCastPolicyId::IntToString => int_to_string(source),
        BuiltinCastPolicyId::FloatToString => float_to_string(source),
        BuiltinCastPolicyId::BoolToString => bool_to_string(source),
        BuiltinCastPolicyId::CharToString => char_to_string(source),
        BuiltinCastPolicyId::CharToInt => char_to_int(source),
        BuiltinCastPolicyId::StringToError => string_to_error(source),
        BuiltinCastPolicyId::ErrorToString => error_to_string(source),
        BuiltinCastPolicyId::FloatToInt => float_to_int(source),
        BuiltinCastPolicyId::IntToChar => int_to_char(source),
        BuiltinCastPolicyId::StringToInt => string_to_int(source),
        BuiltinCastPolicyId::StringToFloat => string_to_float(source),
        BuiltinCastPolicyId::StringToBool => string_to_bool(source),
        BuiltinCastPolicyId::StringToChar => string_to_char(source),
    }
}

// -----------------------------------------------------------
//  Infallible policies
// -----------------------------------------------------------

fn int_to_float(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Int(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Int -> Float requires an Int source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::Float(*value as f64))
}

fn int_to_string(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Int(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Int -> String requires an Int source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::String(value.to_string()))
}

fn float_to_string(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Float(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Float -> String requires a Float source, found {}",
                source.type_name()
            ),
        ));
    };

    // Rust's default float formatting is a stable round-trippable decimal for finite
    // floats. The plan defers custom shortest-decimal formatting until backend
    // formatting parity becomes a hard requirement, so we use it directly here.
    Ok(BuiltinCastLiteral::String(value.to_string()))
}

fn bool_to_string(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Bool(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Bool -> String requires a Bool source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::String(value.to_string()))
}

fn char_to_string(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Char(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Char -> String requires a Char source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::String(value.to_string()))
}

fn char_to_int(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Char(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Char -> Int requires a Char source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::Int(*value as i64))
}

fn string_to_error(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::String(text) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "String -> Error requires a String source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::Error {
        message: text.to_owned(),
        code: 0,
    })
}

fn error_to_string(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Error { message, .. } = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Error -> String policy requires an Error source, found {}",
                source.type_name()
            ),
        ));
    };
    Ok(BuiltinCastLiteral::String(message.to_owned()))
}

// -----------------------------------------------------------
//  Fallible policies
// -----------------------------------------------------------

fn float_to_int(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Float(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Float -> Int requires a Float source, found {}",
                source.type_name()
            ),
        ));
    };

    if !value.is_finite() {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::FloatCastToIntInvalidValue,
            format!("Float -> Int source {value} is not finite"),
        ));
    }

    // Truncate toward zero, then apply the Alpha JS-safe integer materialization
    // policy so folded and runtime casts agree on the representable range.
    let truncated = value.trunc();
    let truncated_int = truncated as i64;
    if !int_is_alpha_runtime_safe(truncated_int) {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::FloatCastToIntOutOfRange,
            format!("Float -> Int source {value} is out of Int range"),
        ));
    }

    Ok(BuiltinCastLiteral::Int(truncated_int))
}

fn int_to_char(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::Int(value) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "Int -> Char requires an Int source, found {}",
                source.type_name()
            ),
        ));
    };

    if *value < 0 {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::IntCastToCharInvalidCodepoint,
            format!("Int -> Char source {value} is negative"),
        ));
    }

    if (0xD800..=0xDFFF).contains(value) {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::IntCastToCharInvalidCodepoint,
            format!("Int -> Char source {value} falls in the surrogate range"),
        ));
    }

    if *value > 0x10FFFF {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::IntCastToCharInvalidCodepoint,
            format!("Int -> Char source {value} exceeds the maximum Unicode scalar"),
        ));
    }

    let codepoint = u32::try_from(*value).map_err(|_| {
        BuiltinCastError::new(
            BuiltinErrorCode::IntCastToCharInvalidCodepoint,
            format!("Int -> Char source {value} is not a valid Unicode scalar"),
        )
    })?;
    let scalar = char::from_u32(codepoint).ok_or_else(|| {
        BuiltinCastError::new(
            BuiltinErrorCode::IntCastToCharInvalidCodepoint,
            format!("Int -> Char source {value} is not a valid Unicode scalar"),
        )
    })?;

    Ok(BuiltinCastLiteral::Char(scalar))
}

fn string_to_int(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::String(text) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "String -> Int requires a String source, found {}",
                source.type_name()
            ),
        ));
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::IntParseInvalidFormat,
            "String -> Int text is empty",
        ));
    }

    let parsed: i64 = trimmed.parse().map_err(|error: std::num::ParseIntError| {
        let code = match error.kind() {
            IntErrorKind::PosOverflow | IntErrorKind::NegOverflow => {
                BuiltinErrorCode::IntParseOutOfRange
            }
            _ => BuiltinErrorCode::IntParseInvalidFormat,
        };

        BuiltinCastError::new(code, format!("Cannot parse Int from {trimmed:?}"))
    })?;

    // Apply the Alpha JS-safe integer materialization policy so folded and
    // runtime casts agree on the representable range.
    if !int_is_alpha_runtime_safe(parsed) {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::IntParseOutOfRange,
            format!("Cannot parse Int from {trimmed:?}"),
        ));
    }

    Ok(BuiltinCastLiteral::Int(parsed))
}

fn string_to_float(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::String(text) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "String -> Float requires a String source, found {}",
                source.type_name()
            ),
        ));
    };

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::FloatParseInvalidFormat,
            "String -> Float text is empty",
        ));
    }

    let parsed: f64 = trimmed.parse().map_err(|_| {
        // Reject `NaN` and `Infinity` text with the dedicated
        // `FloatParseOutOfRange` code so callers can distinguish literal
        // non-finite text from arbitrary invalid format text.
        if trimmed.eq_ignore_ascii_case("nan") || trimmed.eq_ignore_ascii_case("infinity") {
            return BuiltinCastError::new(
                BuiltinErrorCode::FloatParseOutOfRange,
                format!("Cannot parse Float from {trimmed:?}"),
            );
        }
        BuiltinCastError::new(
            BuiltinErrorCode::FloatParseInvalidFormat,
            format!("Cannot parse Float from {trimmed:?}"),
        )
    })?;

    if !parsed.is_finite() {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::FloatParseOutOfRange,
            format!("String -> Float parsed {trimmed:?} is not finite"),
        ));
    }

    Ok(BuiltinCastLiteral::Float(parsed))
}

fn string_to_bool(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::String(text) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "String -> Bool requires a String source, found {}",
                source.type_name()
            ),
        ));
    };

    let trimmed = text.trim();
    match trimmed {
        "true" => Ok(BuiltinCastLiteral::Bool(true)),
        "false" => Ok(BuiltinCastLiteral::Bool(false)),
        _ => Err(BuiltinCastError::new(
            BuiltinErrorCode::StringParseBoolInvalidFormat,
            format!("Cannot parse Bool from {trimmed:?}"),
        )),
    }
}

fn string_to_char(source: &BuiltinCastLiteral) -> Result<BuiltinCastLiteral, BuiltinCastError> {
    let BuiltinCastLiteral::String(text) = source else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::Unsupported,
            format!(
                "String -> Char requires a String source, found {}",
                source.type_name()
            ),
        ));
    };

    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::StringParseCharInvalidFormat,
            "String -> Char text is empty",
        ));
    };

    if chars.next().is_some() {
        return Err(BuiltinCastError::new(
            BuiltinErrorCode::StringParseCharInvalidFormat,
            "String -> Char text contains more than one Unicode scalar",
        ));
    }

    Ok(BuiltinCastLiteral::Char(first))
}
