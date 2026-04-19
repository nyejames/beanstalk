//! Error helpers for the JS runtime.
//!
//! WHAT: canonical runtime `Error` construction and context helpers.
//! WHY: all backend-owned error values should flow through one stable runtime shape.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    /// Emits canonical builtin error helpers used by collection and cast lowering.
    ///
    /// WHAT: normalises location paths, constructs canonical error records, and provides
    /// context helpers for builtin `Error` methods.
    /// WHY: all backend-owned error values should flow through one stable runtime shape.
    pub(crate) fn emit_runtime_error_helpers(&mut self) {
        self.emit_line("function __bs_error_normalize_file(file) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (typeof file !== \"string\") {");
            emitter.with_indent(|em| em.emit_line("return \"\";"));
            emitter.emit_line("}");
            emitter.emit_line("if (file.startsWith(\"/\")) {");
            emitter.with_indent(|em| {
                em.emit_line("const parts = file.split(/[\\\\/]/).filter(Boolean);");
                em.emit_line("return parts.length > 0 ? parts[parts.length - 1] : file;");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (/^[A-Za-z]:[\\\\/]/.test(file)) {");
            emitter.with_indent(|em| {
                em.emit_line("const parts = file.split(/[\\\\/]/).filter(Boolean);");
                em.emit_line("return parts.length > 0 ? parts[parts.length - 1] : file;");
            });
            emitter.emit_line("}");
            emitter.emit_line("return file;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_make_error(kind, code, message, location, trace) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("kind,");
                em.emit_line("code,");
                em.emit_line("message,");
                em.emit_line("location: location ?? null,");
                em.emit_line("trace: trace ?? null");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_with_location(error, location) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return __bs_make_error(error.kind, error.code, error.message, location, error.trace);",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_push_trace(error, frame) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const nextTrace = error.trace ? error.trace.concat([frame]) : [frame];");
            emitter.emit_line(
                "return __bs_make_error(error.kind, error.code, error.message, error.location, nextTrace);",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_bubble(error, file, line, column, functionName) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const safeFunction = typeof functionName === \"string\" && functionName.length > 0 ? functionName : \"<unknown>\";");
            emitter.emit_line("const location = {");
            emitter.with_indent(|em| {
                em.emit_line("file: __bs_error_normalize_file(file),");
                em.emit_line("line,");
                em.emit_line("column,");
                em.emit_line("function: safeFunction === \"<unknown>\" ? null : safeFunction");
            });
            emitter.emit_line("};");
            emitter.emit_line("const frame = { function: safeFunction, location };");
            emitter.emit_line("const nextLocation = error.location ?? location;");
            emitter.emit_line("const located = __bs_error_with_location(error, nextLocation);");
            emitter.emit_line("return __bs_error_push_trace(located, frame);");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
