//! JavaScript helpers for `@core/time`.
//!
//! WHAT: implements the initial wall-clock helper skeleton using `Date.now()`.
//! WHY: richer date, timezone, and monotonic-clock APIs remain deferred; this helper set
//! proves the optional core package path without expanding language semantics.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_time_helpers(&mut self) {
        let helpers: &[(&str, &str)] = &[
            (
                "__bs_time_now_millis",
                "function __bs_time_now_millis() { return Date.now(); }",
            ),
            (
                "__bs_time_now_seconds",
                "function __bs_time_now_seconds() { return Date.now() / 1000.0; }",
            ),
        ];

        self.emit_referenced_core_helpers(helpers);
    }
}
