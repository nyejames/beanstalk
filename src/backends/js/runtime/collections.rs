//! Collection helpers for the JS runtime.
//!
//! WHAT: runtime contracts for ordered collections (guarded get/push/remove/length).
//! WHY: all collection operations return a structured Result carrier so the backend can
//! choose between propagation, fallback, or hard failure. Silent no-ops are not permitted.
//!
//! Semantic policy:
//! - Every helper returns `{ tag: "ok", value: ... }` on success.
//! - Every helper returns `{ tag: "err", value: __bs_make_error(...) }` on contract violation.
//! - Invalid receiver (not an array) → `InvalidArgument` / `collection.expected_ordered_collection`.
//! - Invalid index or out-of-bounds (get, remove) → `OutOfBounds` / `collection.index_out_of_bounds`.
//!   This includes non-integer indices, negative indices, and `index >= length`.
//! - The JS backend wraps push/remove/length calls with `__bs_result_propagate(...)` because
//!   the frontend does not currently expose Result handling for those operations.

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
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_kind}\", \"{ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION}\", \"Collection push(...) expects an ordered collection\", null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("collection.push(value);");
            emitter.emit_line("return { tag: \"ok\", value: null };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_remove(collection, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_kind}\", \"{ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION}\", \"Collection remove(...) expects an ordered collection\", null, null);",
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
            emitter.emit_line("collection.splice(index, 1);");
            emitter.emit_line("return { tag: \"ok\", value: null };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_length(collection) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_kind}\", \"{ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION}\", \"Collection length() expects an ordered collection\", null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("return { tag: \"ok\", value: collection.length };");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
