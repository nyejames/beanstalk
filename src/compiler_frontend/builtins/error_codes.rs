//! Compiler-owned builtin error codes.
//!
//! WHAT: gives backend/generated errors stable integer codes and fallback messages.
//! WHY: the public `Error` surface stores `code Int`, so generated errors need one
//! canonical Rust-side mapping rather than scattered string codes or implicit enum values.

#[allow(dead_code)] // Some codes are reserved for planned surfaces and must keep stable values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BuiltinErrorCode {
    UnknownOrUnassigned = 0,
    Unsupported = 1,
    CollectionExpectedOrderedCollection = 100,
    CollectionIndexOutOfBounds = 101,
    CollectionFixedCapacityExceeded = 102,
    IntParseInvalidFormat = 200,
    IntParseOutOfRange = 201,
    FloatParseInvalidFormat = 210,
    FloatParseOutOfRange = 211,
    /// Reserved for future checked/fallible math operators. This refactor does not emit it.
    DivisionByZero = 300,
}

impl BuiltinErrorCode {
    pub(crate) fn as_i64(self) -> i64 {
        self as i64
    }

    pub(crate) fn default_message(self) -> &'static str {
        match self {
            BuiltinErrorCode::UnknownOrUnassigned => "Unknown error",
            BuiltinErrorCode::Unsupported => "Unsupported operation",
            BuiltinErrorCode::CollectionExpectedOrderedCollection => {
                "Collection operation expects an ordered collection"
            }
            BuiltinErrorCode::CollectionIndexOutOfBounds => "Collection index out of bounds",
            BuiltinErrorCode::CollectionFixedCapacityExceeded => {
                "Fixed collection capacity exceeded"
            }
            BuiltinErrorCode::IntParseInvalidFormat => "Cannot parse Int from text",
            BuiltinErrorCode::IntParseOutOfRange => "Int value is out of supported range",
            BuiltinErrorCode::FloatParseInvalidFormat => "Cannot parse Float from text",
            BuiltinErrorCode::FloatParseOutOfRange => "Float value is out of supported range",
            BuiltinErrorCode::DivisionByZero => "Division by zero",
        }
    }
}
