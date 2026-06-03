//! Error helpers for the JS runtime.
//!
//! WHAT: canonical runtime `Error` construction plus hidden context helpers.
//! WHY: generated errors should expose only the public `message` and `code` fields while still
//! allowing the backend to preserve source context internally.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    /// Emits canonical builtin error helpers used by collection and cast lowering.
    ///
    /// WHAT: normalises location paths, constructs canonical error records, and provides hidden
    /// context helpers used by backend-generated propagation paths.
    /// WHY: public Beanstalk code can only see `message` and `code`; hidden metadata remains a
    /// backend detail and must not require frontend-visible fields.
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

        self.emit_line("function __bs_make_error(message, code, location, trace) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("message,");
                em.emit_line("code,");
                em.emit_line("__bst_location: location ?? null,");
                em.emit_line("__bst_trace: trace ?? null");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_result(message, code) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return { tag: \"err\", value: __bs_make_error(message, code, null, null) };",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_with_location(error, location) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return __bs_make_error(error.message, error.code, location, error.__bst_trace);",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_error_push_trace(error, frame) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const nextTrace = error.__bst_trace ? error.__bst_trace.concat([frame]) : [frame];");
            emitter.emit_line(
                "return __bs_make_error(error.message, error.code, error.__bst_location, nextTrace);",
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
            emitter.emit_line("const nextLocation = error.__bst_location ?? location;");
            emitter.emit_line("const located = __bs_error_with_location(error, nextLocation);");
            emitter.emit_line("return __bs_error_push_trace(located, frame);");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
