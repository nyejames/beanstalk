//! Cast helpers for the JS runtime.
//!
//! WHAT: numeric/string cast behavior and Result-typed error paths.
//! WHY: cast operations must return structured errors rather than raw JS exceptions
//! so that Beanstalk's `Result` semantics work uniformly.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_cast_helpers(&mut self) {
        let int_invalid_format = BuiltinErrorCode::IntParseInvalidFormat;
        let int_invalid_format_code = int_invalid_format.as_i64();
        let int_invalid_format_message = int_invalid_format.default_message();

        let int_out_of_range = BuiltinErrorCode::IntParseOutOfRange;
        let int_out_of_range_code = int_out_of_range.as_i64();
        let int_out_of_range_message = int_out_of_range.default_message();

        let float_invalid_format = BuiltinErrorCode::FloatParseInvalidFormat;
        let float_invalid_format_code = float_invalid_format.as_i64();
        let float_invalid_format_message = float_invalid_format.default_message();

        let float_out_of_range = BuiltinErrorCode::FloatParseOutOfRange;
        let float_out_of_range_code = float_out_of_range.as_i64();
        let float_out_of_range_message = float_out_of_range.default_message();

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
                        "return {{ tag: \"err\", value: __bs_make_error(\"{int_out_of_range_message}\", {int_out_of_range_code}, null, null) }};",
                    ));
                });
                em.emit_line("}");
                em.emit_line("if (Number.isInteger(value)) {");
                em.with_indent(|inner| inner.emit_line("return { tag: \"ok\", value };"));
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"Float value is not an exact integer\", {int_invalid_format_code}, null, null) }};",
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
                            "return {{ tag: \"err\", value: __bs_make_error(\"{int_out_of_range_message}\", {int_out_of_range_code}, null, null) }};",
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
                    "return {{ tag: \"err\", value: __bs_make_error(\"{int_invalid_format_message}\", {int_invalid_format_code}, null, null) }};",
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"Int(...) only accepts Int, Float, or string values\", {int_invalid_format_code}, null, null) }};",
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
                        "return {{ tag: \"err\", value: __bs_make_error(\"{float_out_of_range_message}\", {float_out_of_range_code}, null, null) }};",
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
                            "return {{ tag: \"err\", value: __bs_make_error(\"{float_out_of_range_message}\", {float_out_of_range_code}, null, null) }};",
                        ));
                    });
                    inner.emit_line("}");
                    inner.emit_line("return { tag: \"ok\", value: parsed };");
                });
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{float_invalid_format_message}\", {float_invalid_format_code}, null, null) }};",
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"Float(...) only accepts Int, Float, or string values\", {float_invalid_format_code}, null, null) }};",
            ));
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
