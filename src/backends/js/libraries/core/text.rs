//! JavaScript helpers for `@core/text`.
//!
//! WHAT: implements the initial text helper skeleton on top of JS string operations.
//! WHY: Beanstalk `String`/string-slice values lower to JS string-compatible values in the JS
//! backend, so the package can use host string methods without a new runtime string type.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_core_text_helpers(&mut self) {
        let helpers: &[(&str, &str)] = &[
            (
                "__bs_text_length",
                "function __bs_text_length(text) { return String(text).length; }",
            ),
            (
                "__bs_text_is_empty",
                "function __bs_text_is_empty(text) { return String(text).length === 0; }",
            ),
            (
                "__bs_text_contains",
                "function __bs_text_contains(text, pattern) { return String(text).includes(String(pattern)); }",
            ),
            (
                "__bs_text_starts_with",
                "function __bs_text_starts_with(text, prefix) { return String(text).startsWith(String(prefix)); }",
            ),
            (
                "__bs_text_ends_with",
                "function __bs_text_ends_with(text, suffix) { return String(text).endsWith(String(suffix)); }",
            ),
        ];

        self.emit_referenced_core_helpers(helpers);
    }
}
