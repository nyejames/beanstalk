//! Type compatibility checks for the Beanstalk compiler frontend.
//!
//! WHAT: determines whether a value of a given type is accepted in a position
//! expecting a target type.
//! WHY: this is the sole owner of compatibility policy. All call sites that
//! need to check type compatibility must go through `is_type_compatible` so
//! that structural compatibility rules are applied consistently.
//! `datatypes.rs` owns type structure only; it no longer carries any
//! compatibility logic.

use crate::compiler_frontend::datatypes::DataType;

/// Returns true when `actual` is acceptable in a position that expects `expected`.
///
/// WHAT: the central compatibility predicate for all type positions.
/// WHY: centralising here keeps structural compatibility rules out of parser
/// and lowering sites.
///
/// Rules:
/// - `Inferred` on either side is always compatible (type inference is not yet resolved).
/// - `Option<T>` accepts `None`, `T`, and `Option<T>`.
/// - `BuiltinErrorKind` accepts itself or `StringSlice`.
/// - `StringSlice` accepts `Template` and `TemplateWrapper` (all lower to the same HIR type).
/// - `Result<ok, err>` requires both sides to match structurally.
/// - All other cases require structural equality.
pub(crate) fn is_type_compatible(expected: &DataType, actual: &DataType) -> bool {
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

    // Template and TemplateWrapper lower to the same HIR representation as StringSlice, so
    // they are accepted wherever a StringSlice is expected (e.g. String declarations, function
    // parameters that take String, return slots typed String).
    if matches!(expected, DataType::StringSlice)
        && matches!(actual, DataType::Template | DataType::TemplateWrapper)
    {
        return true;
    }

    expected == actual
}

/// Returns true when `actual` is acceptable at an explicit declaration site
/// expecting `expected`.
///
/// WHAT: the compatibility predicate for `result T = expr` declarations.
/// WHY: declarations accept exact structural matches plus the single implicit
/// numeric promotion `Int → Float`.
pub(crate) fn is_declaration_compatible(expected: &DataType, actual: &DataType) -> bool {
    is_type_compatible(expected, actual) || is_numeric_coercible(actual, expected)
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
