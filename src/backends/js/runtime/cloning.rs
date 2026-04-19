//! Clone helpers for the JS runtime.
//!
//! WHAT: deep recursive copy for explicit `copy` semantics.
//! WHY: Beanstalk `copy` must produce a value that does not alias the original —
//! a shallow copy would silently break that contract for nested structures.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    /// Emits the deep-copy helper for explicit `copy` semantics.
    ///
    /// WHAT: `__bs_clone_value` recursively copies arrays element-by-element and plain objects
    /// key-by-key; primitives are returned as-is.
    /// WHY: Beanstalk `copy` must produce a value that does not alias the original — a shallow
    /// copy would silently break that contract for nested structures.
    pub(crate) fn emit_runtime_clone_helpers(&mut self) {
        self.emit_line("function __bs_clone_value(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (Array.isArray(value)) {");
            emitter.with_indent(|em| em.emit_line("return value.map(__bs_clone_value);"));
            emitter.emit_line("}");
            emitter.emit_line("if (value !== null && typeof value === \"object\") {");
            emitter.with_indent(|em| {
                em.emit_line("const result = {};");
                em.emit_line("for (const key of Object.keys(value)) {");
                em.with_indent(|inner| {
                    inner.emit_line("result[key] = __bs_clone_value(value[key]);");
                });
                em.emit_line("}");
                em.emit_line("return result;");
            });
            emitter.emit_line("}");
            emitter.emit_line("return value;");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
