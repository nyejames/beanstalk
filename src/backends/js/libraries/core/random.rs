//! JavaScript helpers for `@core/random`.
//!
//! WHAT: implements the initial JS-backed random helper skeleton.
//! WHY: random is an optional builder-provided core package; backend behavior belongs beside
//! the JS lowering target, not in frontend package metadata.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_random_helpers(&mut self) {
        let helpers: &[(&str, &str)] = &[
            (
                "__bs_random_float",
                "function __bs_random_float() { return Math.random(); }",
            ),
            (
                "__bs_random_int",
                "function __bs_random_int(min, max) { return Math.floor(Math.random() * (max - min + 1)) + min; }",
            ),
        ];

        self.emit_referenced_core_helpers(helpers);
    }
}
