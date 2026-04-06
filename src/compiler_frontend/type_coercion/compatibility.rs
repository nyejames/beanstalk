//! Type compatibility checks for the Beanstalk compiler frontend.
//!
//! WHAT: determines whether a value of a given type is accepted in a position
//! expecting a target type, taking the surrounding context into account.
//! WHY: previously this logic lived on `DataType::accepts_value_type`, which
//! had no way to express context-dependent allowances. Moving it here lets
//! declaration and return contexts permit Int → Float without widening the
//! rules for function arguments or match patterns.

use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::type_coercion::CompatibilityContext;

/// Returns true when `actual` is acceptable in a position that expects `expected`.
///
/// WHAT: the central compatibility predicate for assignment-like contexts.
/// WHY: callers should use this instead of `DataType::accepts_value_type` so
/// that context-specific promotions are applied in exactly the right places.
///
/// Rules:
/// - `Inferred` on either side is always compatible (type inference is not yet resolved).
/// - `Option<T>` accepts `None`, `T`, and `Option<T>`.
/// - `BuiltinErrorKind` accepts itself or `StringSlice`.
/// - `Result<ok, err>` requires both sides to match structurally.
/// - `Declaration` and `ReturnSlot` contexts also accept `Int` where `Float` is expected.
/// - All other cases require structural equality.
pub(crate) fn is_type_compatible(
    expected: &DataType,
    actual: &DataType,
    context: CompatibilityContext,
) -> bool {
    if matches!(expected, DataType::Inferred) || matches!(actual, DataType::Inferred) {
        return true;
    }

    if let DataType::Option(expected_inner) = expected {
        if matches!(actual, DataType::None) {
            return true;
        }

        if actual == expected_inner.as_ref()
            || matches!(
                (expected_inner.as_ref(), actual),
                (DataType::BuiltinErrorKind, DataType::StringSlice)
            )
        {
            return true;
        }

        if let DataType::Option(actual_inner) = actual {
            if matches!(actual_inner.as_ref(), DataType::Inferred)
                || matches!(expected_inner.as_ref(), DataType::Inferred)
            {
                return true;
            }

            return actual_inner.as_ref() == expected_inner.as_ref();
        }
    }

    if let (
        DataType::Result {
            ok: expected_ok,
            err: expected_err,
        },
        DataType::Result {
            ok: actual_ok,
            err: actual_err,
        },
    ) = (expected, actual)
    {
        if matches!(expected_ok.as_ref(), DataType::Inferred)
            || matches!(actual_ok.as_ref(), DataType::Inferred)
            || matches!(expected_err.as_ref(), DataType::Inferred)
            || matches!(actual_err.as_ref(), DataType::Inferred)
        {
            return true;
        }

        return expected_ok.as_ref() == actual_ok.as_ref()
            && expected_err.as_ref() == actual_err.as_ref();
    }

    if matches!(expected, DataType::BuiltinErrorKind) {
        return matches!(actual, DataType::BuiltinErrorKind | DataType::StringSlice);
    }

    // Contextual numeric promotion: Int is accepted where Float is declared.
    if matches!(
        context,
        CompatibilityContext::Declaration | CompatibilityContext::ReturnSlot
    ) && is_numeric_coercible(actual, expected)
    {
        return true;
    }

    expected == actual
}

/// Returns true when `actual` can be implicitly promoted to `expected` as a
/// contextual numeric coercion.
///
/// WHAT: the narrow set of implicit numeric promotions the language allows.
/// WHY: only Int → Float is supported today. All other numeric combinations
/// require explicit user casts (`Float(x)` / `Int(x)`).
pub(crate) fn is_numeric_coercible(actual: &DataType, expected: &DataType) -> bool {
    matches!((actual, expected), (DataType::Int, DataType::Float))
}

#[cfg(test)]
#[path = "tests/compatibility_tests.rs"]
mod compatibility_tests;
