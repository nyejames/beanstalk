//! JavaScript helpers for `@core/math`.
//!
//! WHAT: wraps JavaScript `Math` methods for `@core/math` external functions.
//! WHY: helper names are stable backend lowering targets, while JS host calls stay isolated here.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_math_helpers(&mut self) {
        let helpers: &[(&str, &str)] = &[
            (
                "__bs_math_sin",
                "function __bs_math_sin(x) { return Math.sin(x); }",
            ),
            (
                "__bs_math_cos",
                "function __bs_math_cos(x) { return Math.cos(x); }",
            ),
            (
                "__bs_math_tan",
                "function __bs_math_tan(x) { return Math.tan(x); }",
            ),
            (
                "__bs_math_atan2",
                "function __bs_math_atan2(y, x) { return Math.atan2(y, x); }",
            ),
            (
                "__bs_math_log",
                "function __bs_math_log(x) { return Math.log(x); }",
            ),
            (
                "__bs_math_log2",
                "function __bs_math_log2(x) { return Math.log2(x); }",
            ),
            (
                "__bs_math_log10",
                "function __bs_math_log10(x) { return Math.log10(x); }",
            ),
            (
                "__bs_math_exp",
                "function __bs_math_exp(x) { return Math.exp(x); }",
            ),
            (
                "__bs_math_pow",
                "function __bs_math_pow(base, exponent) { return Math.pow(base, exponent); }",
            ),
            (
                "__bs_math_sqrt",
                "function __bs_math_sqrt(x) { return Math.sqrt(x); }",
            ),
            (
                "__bs_math_abs",
                "function __bs_math_abs(x) { return Math.abs(x); }",
            ),
            (
                "__bs_math_floor",
                "function __bs_math_floor(x) { return Math.floor(x); }",
            ),
            (
                "__bs_math_ceil",
                "function __bs_math_ceil(x) { return Math.ceil(x); }",
            ),
            (
                "__bs_math_round",
                "function __bs_math_round(x) { return Math.round(x); }",
            ),
            (
                "__bs_math_trunc",
                "function __bs_math_trunc(x) { return Math.trunc(x); }",
            ),
            (
                "__bs_math_min",
                "function __bs_math_min(a, b) { return Math.min(a, b); }",
            ),
            (
                "__bs_math_max",
                "function __bs_math_max(a, b) { return Math.max(a, b); }",
            ),
            (
                "__bs_math_clamp",
                "function __bs_math_clamp(x, min, max) { return Math.min(Math.max(x, min), max); }",
            ),
        ];

        self.emit_referenced_core_helpers(helpers);
    }
}
