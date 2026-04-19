//! Collection helpers for the JS runtime.
//!
//! WHAT: runtime contracts for ordered collections (guarded get/push/remove/length).
//! WHY: collection operations must fail gracefully with Result-typed errors instead
//! of throwing raw JS exceptions.

use crate::backends::js::JsEmitter;
use crate::backends::js::runtime::casts::runtime_error_kind_tag;
use crate::compiler_frontend::builtins::error_type::{
    ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION, ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS,
    builtin_error_kind_for_code,
};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_collection_helpers(&mut self) {
        let invalid_collection_kind = runtime_error_kind_tag(builtin_error_kind_for_code(
            ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION,
        ));
        let out_of_bounds_kind = runtime_error_kind_tag(builtin_error_kind_for_code(
            ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS,
        ));

        self.emit_line("function __bs_collection_get(collection, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_kind}\", \"{ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION}\", \"Collection get(...) expects an ordered collection\", null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (!Number.isInteger(index) || index < 0 || index >= collection.length) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{out_of_bounds_kind}\", \"{ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS}\", \"Collection index out of bounds\", null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("return { tag: \"ok\", value: collection[index] };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_push(collection, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (Array.isArray(collection)) {");
            emitter.with_indent(|em| em.emit_line("collection.push(value);"));
            emitter.emit_line("}");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_remove(collection, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "if (Array.isArray(collection) && Number.isInteger(index) && index >= 0 && index < collection.length) {",
            );
            emitter.with_indent(|em| em.emit_line("collection.splice(index, 1);"));
            emitter.emit_line("}");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_length(collection) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| em.emit_line("return 0;"));
            emitter.emit_line("}");
            emitter.emit_line("return collection.length;");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
