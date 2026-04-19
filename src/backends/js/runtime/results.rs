//! Result helpers for the JS runtime.
//!
//! WHAT: propagation and fallback behavior for internal Result carriers.
//! WHY: expression-position `call(...)!` propagation needs an effectful runtime
//! path that can unwind to the nearest result-returning function boundary.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    /// Emits helpers for internal Result propagation lowering.
    ///
    /// WHAT: `__bs_result_propagate` unwraps `{ tag: "ok", value }` and throws a structured
    /// sentinel for `{ tag: "err", value }`.
    /// WHY: expression-position `call(...)!` propagation needs an effectful runtime path that can
    /// unwind to the nearest result-returning function boundary.
    pub(crate) fn emit_runtime_result_helpers(&mut self) {
        self.emit_line("function __bs_result_propagate(result) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (result && result.tag === \"ok\") {");
            emitter.with_indent(|em| em.emit_line("return result.value;"));
            emitter.emit_line("}");
            emitter.emit_line("if (result && result.tag === \"err\") {");
            emitter.with_indent(|em| {
                em.emit_line("throw { __bs_result_propagate: true, value: result.value };");
            });
            emitter.emit_line("}");
            emitter.emit_line(
                "throw new Error(\"Expected internal Result carrier during propagation\");",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_result_fallback(result, fallback) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (result && result.tag === \"ok\") {");
            emitter.with_indent(|em| em.emit_line("return result.value;"));
            emitter.emit_line("}");
            emitter.emit_line("if (result && result.tag === \"err\") {");
            emitter.with_indent(|em| em.emit_line("return fallback();"));
            emitter.emit_line("}");
            emitter.emit_line(
                "throw new Error(\"Expected internal Result carrier during fallback handling\");",
            );
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
