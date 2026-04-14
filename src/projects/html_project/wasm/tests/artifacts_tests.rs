//! Tests for HTML+Wasm artifact planning and emission.

use super::*;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::tests::test_support::{
    create_test_module, expect_js_output,
};
use std::path::{Path, PathBuf};

#[test]
fn compile_html_module_wasm_exports_bst_start_directly() {
    // WHAT: verify that the export plan exports entry start() as "bst_start", not per-function
    //       wrappers discovered by entry-body call scanning.
    // WHY: entry start() is the sole runtime fragment producer; JS calls it directly.
    let mut string_table = StringTable::new();
    let module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);

    let compiled = compile_html_module_wasm(
        &module.hir,
        &[],
        &module.borrow_analysis,
        &mut string_table,
        Path::new("index.html"),
        "",
        &HtmlDocumentConfig::default(),
        false,
    )
    .expect("wasm mode compilation should succeed");
    let js = expect_js_output(&compiled.output_files, "page.js");

    assert!(
        js.contains("instance.exports.bst_start()"),
        "bootstrap must call bst_start() directly, got:\n{js}"
    );
    assert!(
        !js.contains("bst_call_0"),
        "per-function wrapper exports must not appear in the new architecture"
    );
    assert!(
        !js.contains("__bst_install_wasm_wrappers"),
        "wrapper installation must not appear in the new architecture"
    );
}

#[test]
fn wasm_export_plan_contains_single_entry_start_export() {
    // WHAT: export plan must contain exactly one function export: bst_start for the start function.
    let mut string_table = StringTable::new();
    let module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);

    let plan_a = build_html_wasm_plan(&module.hir, Vec::new())
        .expect("wasm plan should build");
    let plan_b = build_html_wasm_plan(&module.hir, Vec::new())
        .expect("wasm plan should build");

    assert_eq!(
        plan_a.export_plan.function_exports.len(),
        1,
        "export plan must have exactly one function export"
    );
    assert_eq!(
        plan_a.export_plan.function_exports[0].function_id,
        module.hir.start_function,
        "exported function must be the start function"
    );
    assert_eq!(
        plan_a.export_plan.function_exports[0].export_name,
        "bst_start",
        "export name must be bst_start"
    );
    // Verify determinism.
    assert_eq!(
        plan_a.export_plan.function_exports[0].export_name,
        plan_b.export_plan.function_exports[0].export_name,
    );
}

#[test]
fn wasm_export_plan_wires_required_helper_exports() {
    let mut string_table = StringTable::new();
    let module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);

    let plan = build_html_wasm_plan(&module.hir, Vec::new())
        .expect("wasm plan should build");
    let helper = plan.wasm_request.export_policy.helper_exports;

    assert!(helper.export_memory);
    assert!(helper.export_str_ptr);
    assert!(helper.export_str_len);
    assert!(helper.export_release);
}

#[test]
fn compile_html_module_wasm_preserves_nested_logical_html_route() {
    let mut string_table = StringTable::new();
    let module = create_test_module(PathBuf::from("docs/#page.bst"), &mut string_table);

    let compiled = compile_html_module_wasm(
        &module.hir,
        &[],
        &module.borrow_analysis,
        &mut string_table,
        Path::new("docs/index.html"),
        "",
        &HtmlDocumentConfig::default(),
        false,
    )
    .expect("wasm mode compilation should succeed for nested route");

    let output_paths: Vec<PathBuf> = compiled
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect();
    assert!(
        output_paths.contains(&PathBuf::from("docs/index.html")),
        "nested HTML route should be preserved, got: {output_paths:?}"
    );
    assert!(
        output_paths.contains(&PathBuf::from("docs/page.js")),
        "nested JS artifact should be colocated, got: {output_paths:?}"
    );
    assert!(
        output_paths.contains(&PathBuf::from("docs/page.wasm")),
        "nested Wasm artifact should be colocated, got: {output_paths:?}"
    );
    assert_eq!(compiled.html_output_path, PathBuf::from("docs/index.html"));
}
