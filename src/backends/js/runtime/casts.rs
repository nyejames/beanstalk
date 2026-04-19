//! Cast helpers for the JS runtime.
//!
//! WHAT: numeric/string cast behavior and Result-typed error paths.
//! WHY: cast operations must return structured errors rather than raw JS exceptions
//! so that Beanstalk's `Result` semantics work uniformly.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::builtins::error_type::{
    BuiltinErrorKind, ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT, ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE,
    ERROR_CODE_INT_PARSE_INVALID_FORMAT, ERROR_CODE_INT_PARSE_OUT_OF_RANGE,
};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_cast_helpers(&mut self) {
        let parse_kind = runtime_error_kind_tag(BuiltinErrorKind::Parse);
        self.emit_line("function __bs_normalize_numeric_text(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return value.trim().replace(/_/g, \"\");");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_cast_int(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (typeof value === \"number\") {");
            emitter.with_indent(|em| {
                em.emit_line("if (!Number.isFinite(value) || !Number.isSafeInteger(value)) {");
                em.with_indent(|inner| {
                    inner.emit_line(&format!(
                        "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_INT_PARSE_OUT_OF_RANGE}\", \"Int value is out of supported range\", null, null) }};",
                    ));
                });
                em.emit_line("}");
                em.emit_line("if (Number.isInteger(value)) {");
                em.with_indent(|inner| inner.emit_line("return { tag: \"ok\", value };"));
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_INT_PARSE_INVALID_FORMAT}\", \"Float value is not an exact integer\", null, null) }};",
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line("if (typeof value === \"string\") {");
            emitter.with_indent(|em| {
                em.emit_line("const normalized = __bs_normalize_numeric_text(value);");
                em.emit_line("if (/^[+-]?[0-9]+$/.test(normalized)) {");
                em.with_indent(|inner| {
                    inner.emit_line("const parsed = Number.parseInt(normalized, 10);");
                    inner.emit_line("if (!Number.isSafeInteger(parsed)) {");
                    inner.with_indent(|deep| {
                        deep.emit_line(&format!(
                            "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_INT_PARSE_OUT_OF_RANGE}\", \"Int value is out of supported range\", null, null) }};",
                        ));
                    });
                    inner.emit_line("}");
                    inner.emit_line("return { tag: \"ok\", value: parsed };");
                });
                em.emit_line("}");
                em.emit_line("if (/^[+-]?[0-9]+\\.[0-9]+$/.test(normalized)) {");
                em.with_indent(|inner| {
                    inner.emit_line("const parsed = Number.parseFloat(normalized);");
                    inner.emit_line("if (Number.isInteger(parsed) && Number.isSafeInteger(parsed)) {");
                    inner.with_indent(|deep| deep.emit_line("return { tag: \"ok\", value: parsed };"));
                    inner.emit_line("}");
                });
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_INT_PARSE_INVALID_FORMAT}\", \"Cannot parse Int from text\", null, null) }};",
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_INT_PARSE_INVALID_FORMAT}\", \"Int(...) only accepts Int, Float, or string values\", null, null) }};",
            ));
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_cast_float(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (typeof value === \"number\") {");
            emitter.with_indent(|em| {
                em.emit_line("if (!Number.isFinite(value)) {");
                em.with_indent(|inner| {
                    inner.emit_line(&format!(
                        "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE}\", \"Float value is out of supported range\", null, null) }};",
                    ));
                });
                em.emit_line("}");
                em.emit_line("return { tag: \"ok\", value };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (typeof value === \"string\") {");
            emitter.with_indent(|em| {
                em.emit_line("const normalized = __bs_normalize_numeric_text(value);");
                em.emit_line("if (/^[+-]?[0-9]+$/.test(normalized) || /^[+-]?[0-9]+\\.[0-9]+$/.test(normalized)) {");
                em.with_indent(|inner| {
                    inner.emit_line("const parsed = Number.parseFloat(normalized);");
                    inner.emit_line("if (!Number.isFinite(parsed)) {");
                    inner.with_indent(|deep| {
                        deep.emit_line(&format!(
                            "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE}\", \"Float value is out of supported range\", null, null) }};",
                        ));
                    });
                    inner.emit_line("}");
                    inner.emit_line("return { tag: \"ok\", value: parsed };");
                });
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT}\", \"Cannot parse Float from text\", null, null) }};",
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"{parse_kind}\", \"{ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT}\", \"Float(...) only accepts Int, Float, or string values\", null, null) }};",
            ));
        });
        self.emit_line("}");
        self.emit_line("");
    }
}

pub(crate) fn runtime_error_kind_tag(kind: BuiltinErrorKind) -> &'static str {
    kind.as_runtime_tag()
}
