//! JavaScript helpers for `@core/time`.
//!
//! WHAT: implements the initial wall-clock helper skeleton using `Date.now()`.
//! WHY: `now_millis` now uses `InlineExpression` lowering; only `now_seconds` retains a helper.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_time_helpers(&mut self) {
        let helpers: &[(&str, &str)] = &[(
            "__bs_time_now_seconds",
            "function __bs_time_now_seconds() { return Date.now() / 1000.0; }",
        )];

        self.emit_referenced_core_helpers(helpers);
    }
}
