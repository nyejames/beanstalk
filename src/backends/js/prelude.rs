//! Runtime prelude emission for the JavaScript backend.
//!
//! Emits the JS helper functions that implement Beanstalk's runtime semantics.
//! All nine helper groups are declared as JS `function` declarations, which means
//! JS hoisting guarantees correct behaviour regardless of emission order.
//! The ordering below is chosen for readability, not correctness.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::builtins::error_type::{
    BuiltinErrorKind, ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION,
    ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS, ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT,
    ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE, ERROR_CODE_INT_PARSE_INVALID_FORMAT,
    ERROR_CODE_INT_PARSE_OUT_OF_RANGE, builtin_error_kind_for_code,
};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn emit_runtime_prelude(&mut self) {
        // The JS backend preserves Beanstalk's aliasing semantics by modeling locals and computed
        // places as explicit reference records. The prelude is the concrete JS model for those
        // semantics — it is not incidental helper code.
        //
        // Helper groups and their responsibilities:
        //   binding helpers         — reference record construction, parameter normalisation, slot
        //                             read/write, and alias-chain resolution
        //   alias helpers           — binding-mode transitions for borrow and value assignment
        //   computed-place helpers  — closures capturing base reference + key for field/index access
        //   clone helpers           — deep value copy for explicit `copy` semantics
        //   error helpers           — normalises file paths, constructs canonical error records
        //   result helpers          — `?` propagation and `or` fallback helpers
        //   collection helpers      — guarded get/push/remove/length for ordered collections
        //   string helpers          — value-to-string conversion and IO output
        //   cast helpers            — numeric and string casting with Result-typed errors
        //
        // All groups use JS `function` declarations, which are hoisted by the JS engine.
        // Ordering here is for readability only; correctness does not depend on it.
        self.emit_runtime_binding_helpers();
        self.emit_runtime_alias_helpers();
        self.emit_runtime_computed_place_helpers();
        self.emit_runtime_clone_helpers();
        self.emit_runtime_error_helpers();
        self.emit_runtime_result_helpers();
        self.emit_runtime_collection_helpers();
        self.emit_runtime_string_helpers();
        self.emit_runtime_cast_helpers();
    }

    /// Emits the core binding and slot read/write helpers.
    ///
    /// WHAT: `__bs_is_ref` identifies reference records; `__bs_binding` constructs slot bindings;
    /// `__bs_param_binding` normalises call arguments from plain JS values or alias refs;
    /// `__bs_resolve` walks alias chains; `__bs_read`/`__bs_write` perform guarded slot or
    /// computed-place reads and writes.
    /// WHY: every local and parameter in emitted JS flows through this layer so higher-level
    /// emission code can assume uniform binding semantics.
    fn emit_runtime_binding_helpers(&mut self) {
        self.emit_line("function __bs_is_ref(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return value !== null && typeof value === \"object\" && value.__bs_ref === true;",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_binding(value) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return { __bs_ref: true, __bs_kind: \"binding\", __bs_mode: \"slot\", __bs_slot: { value }, __bs_target: null };",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_param_binding(value) {");
        self.with_indent(|emitter| {
            // Calls from JS hosts can pass plain values; Beanstalk-to-Beanstalk calls pass
            // reference records. Normalise both so function bodies only deal with bindings.
            emitter.emit_line("if (!__bs_is_ref(value)) {");
            emitter.with_indent(|em| em.emit_line("return __bs_binding(value);"));
            emitter.emit_line("}");
            emitter.emit_line("if (value.__bs_kind === \"binding\") {");
            emitter.with_indent(|em| em.emit_line("return value;"));
            emitter.emit_line("}");
            // Computed-place ref: wrap in an alias binding so callers get a uniform handle.
            emitter.emit_line("const binding = __bs_binding(undefined);");
            emitter.emit_line("binding.__bs_mode = \"alias\";");
            emitter.emit_line("binding.__bs_target = value;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_resolve(ref) {");
        self.with_indent(|emitter| {
            // Walk alias chains until a slot binding or computed-place ref is reached.
            emitter.emit_line(
                "while (ref.__bs_kind === \"binding\" && ref.__bs_mode === \"alias\") {",
            );
            emitter.with_indent(|em| em.emit_line("ref = ref.__bs_target;"));
            emitter.emit_line("}");
            emitter.emit_line("return ref;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_read(ref) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const resolved = __bs_resolve(ref);");
            emitter.emit_line(
                "return resolved.__bs_kind === \"binding\" ? resolved.__bs_slot.value : resolved.__bs_get();",
            );
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_write(ref, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const resolved = __bs_resolve(ref);");
            emitter.emit_line("if (resolved.__bs_kind === \"binding\") {");
            emitter.with_indent(|em| em.emit_line("resolved.__bs_slot.value = value;"));
            emitter.emit_line("} else {");
            emitter.with_indent(|em| em.emit_line("resolved.__bs_set(value);"));
            emitter.emit_line("}");
            emitter.emit_line("return value;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits binding-mode transition helpers for borrow and value assignment.
    ///
    /// WHAT: `__bs_assign_borrow` makes a fresh slot binding point at another reference (alias
    /// mode), or write-through if already an alias; `__bs_assign_value` collapses an alias and
    /// writes a plain value into the binding's slot.
    /// WHY: Beanstalk has distinct borrow-assign and value-assign semantics that must map to
    /// distinct JS operations — conflating them would silently break aliasing.
    fn emit_runtime_alias_helpers(&mut self) {
        self.emit_line("function __bs_assign_borrow(binding, ref) {");
        self.with_indent(|emitter| {
            // If the binding is already an alias, write through to the existing target rather
            // than rebinding — this preserves the observable aliasing contract.
            emitter.emit_line("if (binding.__bs_mode === \"alias\") {");
            emitter.with_indent(|em| em.emit_line("return __bs_write(binding, __bs_read(ref));"));
            emitter.emit_line("}");
            emitter.emit_line("binding.__bs_mode = \"alias\";");
            emitter.emit_line("binding.__bs_target = ref;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_assign_value(binding, value) {");
        self.with_indent(|emitter| {
            // If the binding is an alias, write through so the aliased location gets the value.
            emitter.emit_line("if (binding.__bs_mode === \"alias\") {");
            emitter.with_indent(|em| em.emit_line("return __bs_write(binding, value);"));
            emitter.emit_line("}");
            // Slot mode: clear any stale alias target and write directly.
            emitter.emit_line("binding.__bs_mode = \"slot\";");
            emitter.emit_line("binding.__bs_target = null;");
            emitter.emit_line("binding.__bs_slot.value = value;");
            emitter.emit_line("return binding;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits computed-place helpers for field and index access.
    ///
    /// WHAT: `__bs_field` and `__bs_index` each return a computed-place record capturing the base
    /// reference and key. The record implements `__bs_get`/`__bs_set` so it composes correctly
    /// with `__bs_read` and `__bs_write`.
    /// WHY: struct field and collection index mutations must route through the same reference
    /// layer as slot bindings — returning a composable computed ref achieves this uniformly.
    fn emit_runtime_computed_place_helpers(&mut self) {
        self.emit_line("function __bs_field(baseRef, field) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("__bs_ref: true,");
                em.emit_line("__bs_kind: \"computed\",");
                em.emit_line("__bs_get() {");
                em.with_indent(|inner| inner.emit_line("return __bs_read(baseRef)[field];"));
                em.emit_line("},");
                em.emit_line("__bs_set(value) {");
                em.with_indent(|inner| inner.emit_line("__bs_read(baseRef)[field] = value;"));
                em.emit_line("}");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_index(baseRef, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("return {");
            emitter.with_indent(|em| {
                em.emit_line("__bs_ref: true,");
                em.emit_line("__bs_kind: \"computed\",");
                em.emit_line("__bs_get() {");
                em.with_indent(|inner| inner.emit_line("return __bs_read(baseRef)[index];"));
                em.emit_line("},");
                em.emit_line("__bs_set(value) {");
                em.with_indent(|inner| inner.emit_line("__bs_read(baseRef)[index] = value;"));
                em.emit_line("}");
            });
            emitter.emit_line("};");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    /// Emits the deep-copy helper for explicit `copy` semantics.
    ///
    /// WHAT: `__bs_clone_value` recursively copies arrays element-by-element and plain objects
    /// key-by-key; primitives are returned as-is.
    /// WHY: Beanstalk `copy` must produce a value that does not alias the original — a shallow
    /// copy would silently break that contract for nested structures.
    fn emit_runtime_clone_helpers(&mut self) {
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

    /// Emits canonical builtin error helpers used by collection and cast lowering.
    ///
    /// WHAT: normalizes location paths, constructs canonical error records, and provides
    /// context helpers for builtin `Error` methods.
    /// WHY: all backend-owned error values should flow through one stable runtime shape.
    fn emit_runtime_error_helpers(&mut self) {
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

    /// Emits helpers for internal Result propagation lowering.
    ///
    /// WHAT: `__bs_result_propagate` unwraps `{ tag: "ok", value }` and throws a structured
    /// sentinel for `{ tag: "err", value }`.
    /// WHY: expression-position `call(...)!` propagation needs an effectful runtime path that can
    /// unwind to the nearest result-returning function boundary.
    fn emit_runtime_result_helpers(&mut self) {
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

    fn emit_runtime_string_helpers(&mut self) {
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

    fn emit_runtime_collection_helpers(&mut self) {
        let invalid_collection_kind = runtime_error_kind_tag(builtin_error_kind_for_code(
            ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION,
        ));
        let out_of_bounds_kind = runtime_error_kind_tag(builtin_error_kind_for_code(
            ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS,
        ));

        self.emit_line("function __bs_collection_get(collectionRef, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const collection = __bs_read(collectionRef);");
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{}\", \"{}\", \"Collection get(...) expects an ordered collection\", null, null);",
                    invalid_collection_kind, ERROR_CODE_COLLECTION_EXPECTED_ORDERED_COLLECTION
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("if (!Number.isInteger(index) || index < 0 || index >= collection.length) {");
            emitter.with_indent(|em| {
                em.emit_line(&format!(
                    "const err = __bs_make_error(\"{}\", \"{}\", \"Collection index out of bounds\", null, null);",
                    out_of_bounds_kind, ERROR_CODE_COLLECTION_INDEX_OUT_OF_BOUNDS
                ));
                em.emit_line("return { tag: \"err\", value: err };");
            });
            emitter.emit_line("}");
            emitter.emit_line("return { tag: \"ok\", value: collection[index] };");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_push(collectionRef, value) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const collection = __bs_read(collectionRef);");
            emitter.emit_line("if (Array.isArray(collection)) {");
            emitter.with_indent(|em| em.emit_line("collection.push(value);"));
            emitter.emit_line("}");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_remove(collectionRef, index) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const collection = __bs_read(collectionRef);");
            emitter.emit_line(
                "if (Array.isArray(collection) && Number.isInteger(index) && index >= 0 && index < collection.length) {",
            );
            emitter.with_indent(|em| em.emit_line("collection.splice(index, 1);"));
            emitter.emit_line("}");
        });
        self.emit_line("}");
        self.emit_line("");

        self.emit_line("function __bs_collection_length(collectionRef) {");
        self.with_indent(|emitter| {
            emitter.emit_line("const collection = __bs_read(collectionRef);");
            emitter.emit_line("if (!Array.isArray(collection)) {");
            emitter.with_indent(|em| em.emit_line("return 0;"));
            emitter.emit_line("}");
            emitter.emit_line("return collection.length;");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    fn emit_runtime_cast_helpers(&mut self) {
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
                        "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Int value is out of supported range\", null, null) }};",
                        parse_kind, ERROR_CODE_INT_PARSE_OUT_OF_RANGE
                    ));
                });
                em.emit_line("}");
                em.emit_line("if (Number.isInteger(value)) {");
                em.with_indent(|inner| inner.emit_line("return { tag: \"ok\", value };"));
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Float value is not an exact integer\", null, null) }};",
                    parse_kind, ERROR_CODE_INT_PARSE_INVALID_FORMAT
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
                            "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Int value is out of supported range\", null, null) }};",
                            parse_kind, ERROR_CODE_INT_PARSE_OUT_OF_RANGE
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
                    "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Cannot parse Int from text\", null, null) }};",
                    parse_kind, ERROR_CODE_INT_PARSE_INVALID_FORMAT
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Int(...) only accepts Int, Float, or string values\", null, null) }};",
                parse_kind, ERROR_CODE_INT_PARSE_INVALID_FORMAT
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
                        "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Float value is out of supported range\", null, null) }};",
                        parse_kind, ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE
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
                            "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Float value is out of supported range\", null, null) }};",
                            parse_kind, ERROR_CODE_FLOAT_PARSE_OUT_OF_RANGE
                        ));
                    });
                    inner.emit_line("}");
                    inner.emit_line("return { tag: \"ok\", value: parsed };");
                });
                em.emit_line("}");
                em.emit_line(&format!(
                    "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Cannot parse Float from text\", null, null) }};",
                    parse_kind, ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT
                ));
            });
            emitter.emit_line("}");
            emitter.emit_line(&format!(
                "return {{ tag: \"err\", value: __bs_make_error(\"{}\", \"{}\", \"Float(...) only accepts Int, Float, or string values\", null, null) }};",
                parse_kind, ERROR_CODE_FLOAT_PARSE_INVALID_FORMAT
            ));
        });
        self.emit_line("}");
        self.emit_line("");
    }
}

fn runtime_error_kind_tag(kind: BuiltinErrorKind) -> &'static str {
    kind.as_runtime_tag()
}
