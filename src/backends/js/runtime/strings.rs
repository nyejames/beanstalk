//! String helpers for the JS runtime.
//!
//! WHAT: string coercion and IO output.
//! WHY: host IO and user-facing string conversion need uniform value-to-string
//! semantics that handle `undefined`/`null` gracefully.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_string_helpers(&mut self) {
        self.emit_line("function __bs_value_to_string(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (value === undefined || value === null) {");
            emitter.with_indent(|em| em.emit_line("return \"\";"));
            emitter.emit_line("}");
            emitter.emit_line("return String(value);");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_io(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("console.log(__bs_value_to_string(value));");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
