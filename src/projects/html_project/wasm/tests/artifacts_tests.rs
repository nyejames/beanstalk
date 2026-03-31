//! Tests for HTML+Wasm artifact planning and emission.

use super::*;
use crate::backends::js::test_symbol_helpers::expected_dev_function_name;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, StartFragment};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::tests::test_support::{
    add_callable_function, add_start_call, create_test_module, expect_js_output,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[test]
fn compile_html_module_wasm_uses_wrapper_exports_not_internal_names() {
    let mut string_table = StringTable::new();
    let mut module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);
    add_callable_function(&mut module, FunctionId(1), "helper_fn", &mut string_table);
    add_start_call(&mut module, "helper_fn", 11, &mut string_table);

    let compiled = compile_html_module_wasm(
        &module.hir,
        &module.borrow_analysis,
        &mut string_table,
        Path::new("index.html"),
        "",
        &HtmlDocumentConfig::default(),
        false,
    )
    .expect("wasm mode compilation should succeed");
    let js = expect_js_output(&compiled.output_files, "page.js");
    let helper_name = expected_dev_function_name("helper_fn", 1);

    assert!(js.contains(&format!("{helper_name} = (...args) =>")));
    assert!(js.contains("bst_call_0"));
    assert!(js.contains("const slots = ["));
}

#[test]
fn wasm_export_plan_is_deterministic_with_stable_wrapper_names() {
    let mut string_table = StringTable::new();
    let mut module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);
    add_callable_function(&mut module, FunctionId(2), "helper_b", &mut string_table);
    add_callable_function(&mut module, FunctionId(1), "helper_a", &mut string_table);
    add_start_call(&mut module, "helper_b", 41, &mut string_table);
    add_start_call(&mut module, "helper_a", 42, &mut string_table);
    module.hir.start_fragments = vec![StartFragment::RuntimeStringFn(FunctionId(2))];

    let function_name_by_id = HashMap::from([
        (FunctionId(0), String::from("start_entry")),
        (FunctionId(1), String::from("helper_a")),
        (FunctionId(2), String::from("helper_b")),
    ]);

    let plan_a = build_html_wasm_plan(&module.hir, &function_name_by_id, Vec::new())
        .expect("wasm plan should build");
    let plan_b = build_html_wasm_plan(&module.hir, &function_name_by_id, Vec::new())
        .expect("wasm plan should build");

    assert_eq!(
        plan_a
            .export_plan
            .function_exports
            .iter()
            .map(|item| (item.function_id.0, item.export_name.clone()))
            .collect::<Vec<_>>(),
        vec![
            (FunctionId(1).0, String::from("bst_call_0")),
            (FunctionId(2).0, String::from("bst_call_1")),
        ]
    );
    assert_eq!(
        plan_a
            .export_plan
            .function_exports
            .iter()
            .map(|item| item.export_name.clone())
            .collect::<Vec<_>>(),
        plan_b
            .export_plan
            .function_exports
            .iter()
            .map(|item| item.export_name.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn wasm_export_plan_wires_required_helper_exports() {
    let mut string_table = StringTable::new();
    let module = create_test_module(PathBuf::from("#page.bst"), &mut string_table);
    let function_name_by_id = HashMap::from([(FunctionId(0), String::from("start_entry"))]);

    let plan = build_html_wasm_plan(&module.hir, &function_name_by_id, Vec::new())
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
