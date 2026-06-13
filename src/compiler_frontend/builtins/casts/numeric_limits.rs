//! Alpha cast integer range policy shared by Rust-side folding and JS runtime.
//!
//! WHAT: defines the portable integer range that explicit builtin casts to `Int` are
//!      allowed to materialize for the current Alpha JavaScript runtime target.
//! WHY: the JS `Number` type can only faithfully represent integers between
//!      `-(2^53 - 1)` and `2^53 - 1` (`Number.isSafeInteger`). Keeping one source of
//!      truth for this range in the compiler lets AST folding and JS helper emission
//!      agree instead of duplicating magic values in two stages.
//! NOTE: this is an explicit cast materialization policy, not a general `Int` semantic.
//!      Full-width `Int` runtime representation remains separate future work.

/// Maximum integer value that explicit builtin casts to `Int` may produce on the
/// Alpha JS runtime target.
///
/// Equal to `2^53 - 1`, the largest integer JS `Number` can represent exactly.
pub(crate) const JS_SAFE_INTEGER_MAX: i64 = 9_007_199_254_740_991;

/// Minimum integer value that explicit builtin casts to `Int` may produce on the
/// Alpha JS runtime target.
///
/// Equal to `-(2^53 - 1)`, the smallest integer JS `Number` can represent exactly.
pub(crate) const JS_SAFE_INTEGER_MIN: i64 = -9_007_199_254_740_991;

/// Returns `true` when `value` lies within the Alpha JS-safe integer range.
pub(crate) fn int_is_alpha_runtime_safe(value: i64) -> bool {
    (JS_SAFE_INTEGER_MIN..=JS_SAFE_INTEGER_MAX).contains(&value)
}
