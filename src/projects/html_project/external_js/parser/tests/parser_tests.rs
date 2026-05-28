use crate::projects::html_project::external_js::parser::{
    parse_js_library, parsed_js_library::JsDiagnosticKind,
};
use crate::projects::html_project::external_js::runtime_module_registry::RuntimeModuleRegistry;

// ------------------------
//  Helpers
// ------------------------

fn parse(
    source: &str,
) -> crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary {
    let registry = RuntimeModuleRegistry::v1();
    parse_js_library(source, &registry)
}

fn assert_opaque_types(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
    expected: &[&str],
) {
    let names: Vec<&str> = library
        .opaque_types
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, expected, "opaque types mismatch");
}

fn assert_free_functions(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
    expected: &[&str],
) {
    let names: Vec<&str> = library
        .free_functions
        .iter()
        .map(|f| f.beanstalk_name.as_str())
        .collect();
    assert_eq!(names, expected, "free functions mismatch");
}

fn assert_receiver_methods(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
    expected: &[&str],
) {
    let names: Vec<&str> = library
        .receiver_methods
        .iter()
        .map(|f| f.beanstalk_name.as_str())
        .collect();
    assert_eq!(names, expected, "receiver methods mismatch");
}

fn assert_diagnostic_kinds(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
    expected: &[JsDiagnosticKind],
) {
    let kinds: Vec<JsDiagnosticKind> = library.diagnostics.iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        expected,
        "diagnostic kinds mismatch. Messages: {:?}",
        library
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
    );
}

fn assert_runtime_imports(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
    expected: &[(&str, &[&str])],
) {
    assert_eq!(
        library.runtime_imports.len(),
        expected.len(),
        "runtime import count mismatch"
    );
    for (index, (module_name, names)) in expected.iter().enumerate() {
        let runtime_import = &library.runtime_imports[index];
        assert_eq!(
            runtime_import.module_name, *module_name,
            "runtime import module name mismatch at index {index}"
        );
        let expected_names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        assert_eq!(
            runtime_import.imported_names, expected_names,
            "runtime import names mismatch at index {index}"
        );
    }
}

fn assert_no_diagnostics(
    library: &crate::projects::html_project::external_js::parser::parsed_js_library::ParsedJsLibrary,
) {
    assert!(
        library.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        library.diagnostics
    );
}

// ------------------------
//  Opaque types
// ------------------------

#[test]
fn opaque_type_declarations_are_parsed() {
    let source = r#"
/**
 * @bst.opaque Canvas
 * @bst.opaque Canvas2d
 */
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_opaque_types(&library, &["Canvas", "Canvas2d"]);
}

#[test]
fn opaque_type_single_line_block() {
    let source = r#"/** @bst.opaque Handle */"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_opaque_types(&library, &["Handle"]);
}

// ------------------------
//  Free function signatures
// ------------------------

#[test]
fn free_function_signature_parsed() {
    let source = r#"
/**
 * @bst.opaque Canvas
 * @bst.sig get_canvas |id String| -> Canvas, Error!
 */
export function getCanvas(id) {
    return bstOk(document.getElementById(id));
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["get_canvas"]);

    let func = &library.free_functions[0];
    assert_eq!(func.js_name, "getCanvas");
    assert_eq!(func.signature.parameters.len(), 1);
    assert_eq!(func.signature.parameters[0].name, "id");
    assert_eq!(func.signature.parameters[0].type_name, "String");
    assert!(!func.signature.parameters[0].is_receiver);
    assert_eq!(func.signature.returns.len(), 1);
    assert_eq!(func.signature.returns[0].type_name, "Canvas");
    assert!(func.signature.has_error_return);
}

#[test]
fn free_function_no_return() {
    let source = r#"
/**
 * @bst.sig log_message |msg String|
 */
export function logMessage(msg) {
    console.log(msg);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["log_message"]);
    let func = &library.free_functions[0];
    assert_eq!(func.signature.returns.len(), 0);
    assert!(!func.signature.has_error_return);
}

#[test]
fn free_function_error_only_return() {
    let source = r#"
/**
 * @bst.sig do_fallible || -> Error!
 */
export function doFallible() {
    return bstOk();
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["do_fallible"]);
    assert!(library.free_functions[0].signature.has_error_return);
    assert_eq!(library.free_functions[0].signature.returns.len(), 0);
}

#[test]
fn const_arrow_export_parsed() {
    let source = r#"
/**
 * @bst.sig add |a Int, b Int| -> Int
 */
export const add = (a, b) => {
    return a + b;
};
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["add"]);
    assert_eq!(library.free_functions[0].js_name, "add");
    assert_eq!(library.free_functions[0].signature.parameters.len(), 2);
}

#[test]
fn const_export_must_be_arrow_function() {
    let source = r#"
/**
 * @bst.sig answer || -> Int
 */
export const answer = 42;
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnsupportedParameterPattern,
            JsDiagnosticKind::MissingExportAfterSig,
        ],
    );
}

// ------------------------
//  Receiver method signatures
// ------------------------

#[test]
fn receiver_method_signature_parsed() {
    let source = r#"
/**
 * @bst.opaque Canvas2d
 */

/**
 * @bst.sig fill_rect |this ~Canvas2d, x Float, y Float, width Float, height Float|
 */
export function fillRect(ctx, x, y, width, height) {
    ctx.fillRect(x, y, width, height);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_receiver_methods(&library, &["fill_rect"]);

    let func = &library.receiver_methods[0];
    assert_eq!(func.js_name, "fillRect");
    assert_eq!(func.signature.parameters.len(), 5);
    assert!(func.signature.parameters[0].is_receiver);
    assert_eq!(func.signature.parameters[0].name, "this");
    assert_eq!(func.signature.parameters[0].type_name, "Canvas2d");
    assert!(func.signature.parameters[0].is_mutable);
}

#[test]
fn receiver_method_immutable_receiver() {
    let source = r#"
/**
 * @bst.sig describe |this String| -> String
 */
export const describe = (self) => {
    return self;
};
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_receiver_methods(&library, &["describe"]);
    assert!(!library.receiver_methods[0].signature.parameters[0].is_mutable);
}

#[test]
fn regular_mutable_parameter_marker_is_parsed() {
    let source = r#"
/**
 * @bst.opaque Buffer
 * @bst.sig write |buffer ~Buffer, text String|
 */
export function write(buffer, text) {
    buffer.value = text;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["write"]);
    assert!(library.free_functions[0].signature.parameters[0].is_mutable);
    assert_eq!(
        library.free_functions[0].signature.parameters[0].type_name,
        "Buffer"
    );
}

// ------------------------
//  Invalid receiver parameter
// ------------------------

#[test]
fn receiver_parameter_must_be_first() {
    let source = r#"
/**
 * @bst.opaque Canvas2d
 * @bst.sig bad |x Float, this ~Canvas2d|
 */
export function bad(x, ctx) {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::InvalidReceiverParameter]);
    // Malformed `this` at index 1 means has_receiver() is false, so the function
    // lands in free_functions rather than receiver_methods.
    assert_free_functions(&library, &["bad"]);
    assert!(library.receiver_methods.is_empty());
}

#[test]
fn duplicate_receiver_parameter_rejected() {
    let source = r#"
/**
 * @bst.opaque Canvas2d
 * @bst.sig bad |this ~Canvas2d, this Canvas2d|
 */
export function bad(ctx, other) {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::InvalidReceiverParameter,
            JsDiagnosticKind::InvalidReceiverParameter,
        ],
    );
    // First parameter is a valid receiver, so it still becomes a receiver method.
    assert_receiver_methods(&library, &["bad"]);
}

#[test]
fn receiver_parameter_after_recovered_invalid_parameter_is_rejected() {
    let source = r#"
/**
 * @bst.opaque Canvas2d
 * @bst.sig bad |...values, this Canvas2d|
 */
export function bad(values, ctx) {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnsupportedParameterPattern,
            JsDiagnosticKind::InvalidReceiverParameter,
            JsDiagnosticKind::ArityMismatch,
        ],
    );
}

#[test]
fn receiver_parameter_missing_type_annotation_still_reported() {
    let source = r#"
/**
 * @bst.sig bad |this|
 */
export function bad(ctx) {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedTypeSyntax]);
    assert_receiver_methods(&library, &["bad"]);
}

// ------------------------
//  Arity validation
// ------------------------

#[test]
fn arity_mismatch_reported() {
    let source = r#"
/**
 * @bst.opaque Canvas
 * @bst.sig get_canvas |id String, extra String| -> Canvas, Error!
 */
export function getCanvas(id) {
    return bstOk(id);
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArityMismatch]);
    assert_free_functions(&library, &["get_canvas"]);
}

#[test]
fn receiver_this_counts_in_arity() {
    let source = r#"
/**
 * @bst.opaque Canvas2d
 * @bst.sig fill_rect |this ~Canvas2d, x Float|
 */
export function fillRect(ctx, x, y) {
    ctx.fillRect(x, y);
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArityMismatch]);
    assert_receiver_methods(&library, &["fill_rect"]);
}

// ------------------------
//  Missing export after @bst.sig
// ------------------------

#[test]
fn missing_export_after_sig_reported() {
    let source = r#"
/**
 * @bst.sig orphaned |id String| -> String
 */
// no export here
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::MissingExportAfterSig]);
    assert!(library.free_functions.is_empty());
}

#[test]
fn unknown_external_type_reported() {
    let source = r#"
/**
 * @bst.sig get_canvas |id String| -> Canvas, Error!
 */
export function getCanvas(id) {
    return bstOk(id);
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnknownExternalType]);
}

#[test]
fn unknown_receiver_type_reported() {
    let source = r#"
/**
 * @bst.sig fill_rect |this ~Canvas2d, x Float|
 */
export function fillRect(ctx, x) {
    ctx.fillRect(x, x);
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnknownExternalType]);
    assert_receiver_methods(&library, &["fill_rect"]);
}

// ------------------------
//  Duplicate names
// ------------------------

#[test]
fn duplicate_beanstalk_name_reported() {
    let source = r#"
/**
 * @bst.sig get_canvas |id String| -> String
 */
export function getCanvas1(id) { return id; }

/**
 * @bst.sig get_canvas |name String| -> String
 */
export function getCanvas2(name) { return name; }
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::DuplicateBeanstalkName]);
}

#[test]
fn duplicate_js_export_name_reported() {
    let source = r#"
/**
 * @bst.sig first |id String| -> String
 */
export function getCanvas(id) { return id; }

/**
 * @bst.sig second |name String| -> String
 */
export function getCanvas(name) { return name; }
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::DuplicateJsExportName]);
}

#[test]
fn duplicate_opaque_type_name_reported() {
    let source = r#"
/**
 * @bst.opaque Handle
 * @bst.opaque Handle
 */
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::DuplicateBeanstalkName]);
}

// ------------------------
//  Unannotated exports
// ------------------------

#[test]
fn unannotated_export_rejected() {
    let source = r#"
export function helper(x) {
    return x;
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnannotatedExport]);
}

#[test]
fn unannotated_and_annotated_exports_mixed() {
    let source = r#"
/**
 * @bst.sig public_fn |x Int| -> Int
 */
export function publicFn(x) { return x; }

export function privateHelper(x) { return x; }
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnannotatedExport]);
    assert_free_functions(&library, &["public_fn"]);
}

// ------------------------
//  @bst.package rejection
// ------------------------

#[test]
fn bst_package_rejected() {
    let source = r#"
/**
 * @bst.package my_package
 */
export function foo() {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnsupportedPackageTag,
            JsDiagnosticKind::UnannotatedExport,
        ],
    );
}

#[test]
fn unknown_bst_directive_rejected() {
    let source = r#"
/**
 * @bst.future value
 */
export function foo() {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnknownBstDirective,
            JsDiagnosticKind::UnannotatedExport,
        ],
    );
}

// ------------------------
//  Default export rejection
// ------------------------

#[test]
fn default_export_rejected() {
    let source = r#"
export default function foo() {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::DefaultExport]);
}

// ------------------------
//  Re-export rejection
// ------------------------

#[test]
fn re_export_rejected() {
    let source = r#"
export { foo };
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ReExport]);
}

// ------------------------
//  CommonJS rejection
// ------------------------

#[test]
fn commonjs_module_exports_rejected() {
    let source = r#"
module.exports = { foo };
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::CommonJsExport]);
}

#[test]
fn commonjs_exports_dot_rejected() {
    let source = r#"
exports.foo = function() {};
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::CommonJsExport]);
}

// ------------------------
//  Class export rejection
// ------------------------

#[test]
fn class_export_rejected() {
    let source = r#"
export class Widget {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ClassExport]);
}

// ------------------------
//  Import rejection
// ------------------------

#[test]
fn dynamic_import_rejected() {
    let source = r#"
const m = import("./helper.js");
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::DynamicImport]);
}

#[test]
fn arbitrary_static_import_rejected() {
    let source = r#"
import { foo } from "./helper.js";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArbitraryImport]);
}

#[test]
fn namespace_static_import_rejected() {
    let source = r#"
import * as helper from "./helper.js";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArbitraryImport]);
}

#[test]
fn side_effect_static_import_rejected() {
    let source = r#"
import "./helper.js";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArbitraryImport]);
}

#[test]
fn registered_runtime_import_accepted() {
    let source = r#"
import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.sig do_thing || -> Error!
 */
export function doThing() {
    return bstOk();
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["do_thing"]);
}

#[test]
fn unregistered_runtime_looking_module_is_rejected() {
    let source = r#"
import { foo } from "@beanstalk/other-runtime";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArbitraryImport]);
}

#[test]
fn v1_runtime_registry_contains_only_beanstalk_runtime() {
    let registry = RuntimeModuleRegistry::v1();
    assert!(registry.is_registered("@beanstalk/runtime"));
    assert!(!registry.is_registered("@beanstalk/other-runtime"));
    assert!(!registry.is_registered("./helper.js"));
    let modules = registry.registered_modules();
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].specifier, "@beanstalk/runtime");
}

#[test]
fn runtime_named_import_is_recorded() {
    let source = r#"
import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.sig do_thing || -> Int, Error!
 */
export function doThing() {
    return bstOk(7);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_runtime_imports(&library, &[("@beanstalk/runtime", &["bstErr", "bstOk"])]);
}

#[test]
fn multiline_registered_runtime_import_accepted() {
    let source = r#"
import {
    bstOk,
    bstErr,
} from "@beanstalk/runtime";

/**
 * @bst.sig do_thing || -> Int, Error!
 */
export function doThing() {
    return bstOk(7);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_runtime_imports(&library, &[("@beanstalk/runtime", &["bstErr", "bstOk"])]);
}

#[test]
fn multiline_arbitrary_import_rejected() {
    let source = r#"
import {
    foo,
    bar,
} from "./helper.js";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::ArbitraryImport]);
    assert!(library.runtime_imports.is_empty());
}

#[test]
fn non_fallible_function_with_runtime_import_records_import() {
    let source = r#"
import { bstOk } from "@beanstalk/runtime";

/**
 * @bst.sig get_number || -> Int
 */
export function getNumber() {
    return bstOk(7).value;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_eq!(library.free_functions.len(), 1);
    assert!(!library.free_functions[0].signature.has_error_return);
    assert_runtime_imports(&library, &[("@beanstalk/runtime", &["bstOk"])]);
}

#[test]
fn runtime_import_alias_rejected() {
    let source = r#"
import { bstOk as ok } from "@beanstalk/runtime";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedRuntimeImportForm]);
    assert!(library.runtime_imports.is_empty());
}

#[test]
fn runtime_default_import_rejected() {
    let source = r#"
import runtime from "@beanstalk/runtime";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedRuntimeImportForm]);
    assert!(library.runtime_imports.is_empty());
}

#[test]
fn runtime_namespace_import_rejected() {
    let source = r#"
import * as runtime from "@beanstalk/runtime";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedRuntimeImportForm]);
    assert!(library.runtime_imports.is_empty());
}

#[test]
fn unknown_runtime_import_name_rejected() {
    let source = r#"
import { nope } from "@beanstalk/runtime";
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnknownRuntimeImportName]);
    assert!(library.runtime_imports.is_empty());
}

#[test]
fn duplicate_runtime_imports_deduplicate() {
    let source = r#"
import { bstOk } from "@beanstalk/runtime";
import { bstErr } from "@beanstalk/runtime";

/**
 * @bst.sig do_thing || -> Int, Error!
 */
export function doThing() {
    return bstOk(7);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_eq!(library.runtime_imports.len(), 1);
    assert_eq!(library.runtime_imports[0].module_name, "@beanstalk/runtime");
    assert_eq!(
        library.runtime_imports[0].imported_names,
        vec!["bstErr", "bstOk"]
    );
}

#[test]
fn explicit_registry_injected_into_parser() {
    let source = r#"
import { bstOk } from "@beanstalk/runtime";

/**
 * @bst.sig do_thing || -> Error!
 */
export function doThing() {
    return bstOk();
}
"#;
    let registry = RuntimeModuleRegistry::v1();
    let library = parse_js_library(source, &registry);
    assert!(
        library.diagnostics.is_empty(),
        "got: {:?}",
        library.diagnostics
    );
    assert_eq!(library.free_functions.len(), 1);
}

#[test]
fn explicit_empty_registry_rejects_all_imports() {
    let source = r#"
import { bstOk } from "@beanstalk/runtime";
"#;
    let registry = RuntimeModuleRegistry::empty();
    let library = parse_js_library(source, &registry);
    assert!(
        library
            .diagnostics
            .iter()
            .any(|d| d.kind == JsDiagnosticKind::ArbitraryImport),
        "expected ArbitraryImport for empty registry"
    );
}

#[test]
fn export_keywords_inside_comments_and_strings_are_ignored() {
    let source = r#"
// export function commentedOut() {}
const text = "export function stringOnly() {}";
/*
export function blockCommented() {}
*/
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert!(library.free_functions.is_empty());
}

#[test]
fn export_body_with_brace_in_string_does_not_break_scanning() {
    let source = r#"
/**
 * @bst.sig tricky || -> String
 */
export function tricky() {
    const text = "} export function fake() {}";
    return text;
}

/**
 * @bst.sig next || -> Int
 */
export function next() {
    return 1;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["tricky", "next"]);
}

#[test]
fn export_body_with_import_in_string_does_not_emit_import_diagnostic() {
    let source = r#"
/**
 * @bst.sig tricky || -> String
 */
export function tricky() {
    const text = "import { foo } from './bar.js';";
    return text;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["tricky"]);
}

#[test]
fn export_body_comments_containing_export_are_ignored() {
    let source = r#"
/**
 * @bst.sig tricky || -> Int
 */
export function tricky() {
    // export function fake() {}
    /* export function fake() {} */
    return 1;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["tricky"]);
}

#[test]
fn template_literal_with_braces_does_not_break_scanning() {
    let source = r#"
/**
 * @bst.sig tricky || -> String
 */
export function tricky() {
    const text = `value ${"{ }"}`;
    return text;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["tricky"]);
}

#[test]
fn arrow_block_body_with_brace_in_string_handled() {
    let source = r#"
/**
 * @bst.sig tricky || -> String
 */
export const tricky = () => {
    const text = "}";
    return text;
};
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["tricky"]);
}

#[test]
fn expression_bodied_arrow_export_rejected() {
    let source = r#"
/**
 * @bst.sig add |a Int, b Int| -> Int
 */
export const add = (a, b) => a + b;
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::ExpressionBodiedArrowExport,
            JsDiagnosticKind::MissingExportAfterSig,
        ],
    );
    assert!(library.free_functions.is_empty());
}

// ------------------------
//  Unsupported parameter patterns
// ------------------------

#[test]
fn rest_parameter_rejected() {
    let source = r#"
/**
 * @bst.sig sum |...values| -> Int
 */
export function sum(...values) {
    return values.reduce((a, b) => a + b, 0);
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnsupportedParameterPattern,
            JsDiagnosticKind::UnsupportedParameterPattern,
        ],
    );
}

#[test]
fn default_parameter_rejected() {
    let source = r#"
/**
 * @bst.sig greet |name String| -> String
 */
export function greet(name = "world") {
    return name;
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedParameterPattern]);
}

#[test]
fn destructuring_parameter_rejected() {
    let source = r#"
/**
 * @bst.sig unpack |point| -> Int
 */
export function unpack({ x }) {
    return x;
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(
        &library,
        &[
            JsDiagnosticKind::UnsupportedParameterPattern,
            JsDiagnosticKind::UnsupportedTypeSyntax,
            JsDiagnosticKind::ArityMismatch,
        ],
    );
}

// ------------------------
//  Unsupported type syntax
// ------------------------

#[test]
fn collection_type_in_signature_rejected() {
    let source = r#"
/**
 * @bst.sig process |items {String}| -> String
 */
export function process(items) {
    return items[0];
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedTypeSyntax]);
}

#[test]
fn option_type_in_signature_rejected() {
    let source = r#"
/**
 * @bst.sig maybe |name String?| -> String
 */
export function maybe(name) {
    return name || "";
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::UnsupportedTypeSyntax]);
}

#[test]
fn void_return_rejected() {
    let source = r#"
/**
 * @bst.sig noop || -> Void
 */
export function noop() {}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::VoidReturn]);
}

#[test]
fn multi_success_return_rejected() {
    let source = r#"
/**
 * @bst.sig pair || -> Int, String
 */
export function pair() {
    return [1, "a"];
}
"#;
    let library = parse(source);
    assert_diagnostic_kinds(&library, &[JsDiagnosticKind::MultiSuccessReturn]);
}

// ------------------------
//  Snake-case / camelCase mapping
// ------------------------

#[test]
fn snake_case_beanstalk_name_maps_to_camel_case_js() {
    let source = r#"
/**
 * @bst.sig get_canvas_context |id String| -> String
 */
export function getCanvasContext(id) {
    return id;
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_eq!(
        library.free_functions[0].beanstalk_name,
        "get_canvas_context"
    );
    assert_eq!(library.free_functions[0].js_name, "getCanvasContext");
}

// ------------------------
//  Private helpers (unexported) are allowed
// ------------------------

#[test]
fn unexported_helpers_are_allowed() {
    let source = r#"
function privateHelper(x) {
    return x * 2;
}

/**
 * @bst.sig double |x Int| -> Int
 */
export function double(x) {
    return privateHelper(x);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_free_functions(&library, &["double"]);
}

// ------------------------
//  Multiple annotations and exports
// ------------------------

#[test]
fn full_library_parse() {
    let source = r#"
import { bstOk, bstErr } from "@beanstalk/runtime";

/**
 * @bst.opaque Canvas
 * @bst.opaque Canvas2d
 */

/**
 * @bst.sig get_canvas |id String| -> Canvas, Error!
 */
export function getCanvas(id) {
    const canvas = document.getElementById(id);
    if (!canvas) {
        return bstErr(404, "Canvas not found");
    }
    return bstOk(canvas);
}

/**
 * @bst.sig fill_rect |this ~Canvas2d, x Float, y Float, width Float, height Float|
 */
export function fillRect(ctx, x, y, width, height) {
    ctx.fillRect(x, y, width, height);
}
"#;
    let library = parse(source);
    assert_no_diagnostics(&library);
    assert_opaque_types(&library, &["Canvas", "Canvas2d"]);
    assert_free_functions(&library, &["get_canvas"]);
    assert_receiver_methods(&library, &["fill_rect"]);
}
