//! Collection helpers for the JS runtime.
//!
//! WHAT: runtime contracts for ordered collections.
//! WHY: fallible collection operations return structured carriers, while infallible operations
//! stay plain JS helpers so the backend surface matches the language semantics.
//!
//! Semantic policy:
//! - `get`, `set`, and `remove` return `{ tag: "ok", value: ... }` or `{ tag: "err", value: ... }`.
//! - `push` and `length` are infallible helpers.
//! - Invalid receivers for fallible operations → `BuiltinErrorCode::CollectionExpectedOrderedCollection`.
//! - Invalid index or out-of-bounds (get, set, remove) → `BuiltinErrorCode::CollectionIndexOutOfBounds`.
//!   This includes non-integer indices, negative indices, and `index >= length`.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::builtins::error_codes::BuiltinErrorCode;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_collection_helpers(&mut self) {
        let invalid_collection = BuiltinErrorCode::CollectionExpectedOrderedCollection;
        let invalid_collection_code = invalid_collection.as_i64();
        let invalid_collection_message = invalid_collection.default_message();

        let out_of_bounds = BuiltinErrorCode::CollectionIndexOutOfBounds;
        let out_of_bounds_code = out_of_bounds.as_i64();
        let out_of_bounds_message = out_of_bounds.default_message();

        self.emit_line("function __bs_collection_get(collection, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_message}\", {invalid_collection_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (!Number.isInteger(index) || index < 0 || index >= collection.length) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{out_of_bounds_message}\", {out_of_bounds_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("return { tag: \"ok\", value: collection[index] };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_set(collection, index, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_message}\", {invalid_collection_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (!Number.isInteger(index) || index < 0 || index >= collection.length) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{out_of_bounds_message}\", {out_of_bounds_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("collection[index] = value;");
            emitter.emit_line("return { tag: \"ok\", value: null };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_push(collection, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("collection.push(value);");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_remove(collection, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{invalid_collection_message}\", {invalid_collection_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (!Number.isInteger(index) || index < 0 || index >= collection.length) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{out_of_bounds_message}\", {out_of_bounds_code}, null, null);",
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("const removed = collection.splice(index, 1)[0];");
            emitter.emit_line("return { tag: \"ok\", value: removed };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_length(collection) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return collection.length;");
        });
        self.emit_line("}");
        self.emit_line("");
    }
}
