//! Tests for the JS-only HTML rendering path.

use super::*;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::tests::test_support::{
    add_callable_function, assert_has_basic_shell, create_test_hir_module, create_test_module,
};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn js_lifecycle_order_is_static_then_bundle_then_hydration_then_start() {
    let mut module = create_test_module(std::path::PathBuf::from("#page.bst"));
    module.hir.start_fragments = vec![
        StartFragment::ConstString(crate::compiler_frontend::hir::hir_nodes::ConstStringId(0)),
        StartFragment::RuntimeStringFn(FunctionId(0)),
    ];
    module.hir.const_string_pool = vec![String::from("<h1>Hello</h1>")];
    let function_names = HashMap::from([(FunctionId(0), String::from("start_entry"))]);

    let html = render_html_document(
        &module.hir,
        &module.string_table,
        &HtmlDocumentConfig::default(),
        Path::new("index.html"),
        "",
        "function start_entry() { return; }",
        &function_names,
    )
    .expect("render_html_document should succeed");

    let static_pos = html
        .find("<h1>Hello</h1>")
        .expect("static fragment must be present");
    let slot_pos = html
        .find("<div id=\"bst-slot-0\">")
        .expect("runtime slot must be present");
    let bundle_pos = html
        .find("<script>")
        .expect("JS bundle script block must be present");
    let hydration_pos = html
        .find("insertAdjacentHTML")
        .expect("slot hydration must be present");
    let start_pos = html
        .find("if (typeof start_entry")
        .expect("start() invocation must be present");

    assert_has_basic_shell(&html);
    assert!(
        static_pos < slot_pos,
        "const fragment must appear before runtime slot"
    );
    assert!(
        slot_pos < bundle_pos,
        "runtime slot must appear before the JS bundle script tag"
    );
    assert!(
        bundle_pos < hydration_pos,
        "JS bundle must be loaded before slot hydration"
    );
    assert!(
        hydration_pos < start_pos,
        "slot hydration must complete before start() is called"
    );
}

#[test]
fn render_entry_fragments_preserves_runtime_slot_order() {
    let mut module = create_test_module(std::path::PathBuf::from("#page.bst"));
    add_callable_function(&mut module, FunctionId(1), "frag_b");
    module.hir.start_fragments = vec![
        StartFragment::RuntimeStringFn(FunctionId(0)),
        StartFragment::RuntimeStringFn(FunctionId(1)),
    ];

    let (body_html, runtime_slots) =
        render_entry_fragments(&module.hir).expect("fragments should render");
    let slot0_pos = body_html
        .find("bst-slot-0")
        .expect("bst-slot-0 must be present");
    let slot1_pos = body_html
        .find("bst-slot-1")
        .expect("bst-slot-1 must be present");

    assert!(
        slot0_pos < slot1_pos,
        "runtime slots must appear in source fragment order"
    );
    assert_eq!(runtime_slots.len(), 2);
    assert_eq!(runtime_slots[0].function_id, FunctionId(0));
    assert_eq!(runtime_slots[1].function_id, FunctionId(1));
}

#[test]
fn no_runtime_fragments_still_emits_start_call() {
    let module = create_test_module(std::path::PathBuf::from("#page.bst"));
    let function_names = HashMap::from([(FunctionId(0), String::from("start_entry"))]);

    let html = render_html_document(
        &module.hir,
        &module.string_table,
        &HtmlDocumentConfig::default(),
        Path::new("index.html"),
        "",
        "function start_entry() { return; }",
        &function_names,
    )
    .expect("render_html_document should succeed");

    assert!(
        !html.contains("bst-slot-"),
        "no runtime slots should be present when there are no runtime fragments"
    );
    assert!(
        html.contains("if (typeof start_entry === \"function\") start_entry();"),
        "start() must still be called when there are no runtime fragments"
    );
}

#[test]
fn escape_inline_script_replaces_closing_tag_sequence() {
    let js = "const x = \"</script>\";";
    let escaped = escape_inline_script(js);

    assert_eq!(escaped, "const x = \"<\\/script>\";");
    assert!(
        !escaped.contains("</"),
        "escaped JS must not contain any '</' sequence"
    );
}

#[test]
fn inline_js_bundle_with_closing_script_tag_is_escaped_in_html() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![];
    let function_names = HashMap::from([(FunctionId(0), String::from("start_entry"))]);

    let html = render_html_document(
        &hir_module,
        &crate::compiler_frontend::string_interning::StringTable::new(),
        &HtmlDocumentConfig::default(),
        Path::new("index.html"),
        "",
        "const msg = \"</script>\";\n",
        &function_names,
    )
    .expect("render_html_document should succeed");

    assert!(
        !html.contains("</script>\";"),
        "raw </script> inside a JS string must not appear unescaped in HTML output"
    );
    assert!(
        html.contains("<\\/script>"),
        "the closing-tag sequence must be escaped as <\\/script> in the output"
    );
}

#[test]
fn render_entry_fragments_errors_on_missing_const_string() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![StartFragment::ConstString(
        crate::compiler_frontend::hir::hir_nodes::ConstStringId(99),
    )];

    let error = render_entry_fragments(&hir_module)
        .expect_err("should fail when const fragment ID is out of bounds");
    assert!(
        error.msg.contains("const fragment"),
        "error message must mention the missing const fragment"
    );
}

#[test]
fn render_html_document_errors_on_missing_function_name() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![StartFragment::RuntimeStringFn(FunctionId(99))];

    let error = render_html_document(
        &hir_module,
        &crate::compiler_frontend::string_interning::StringTable::new(),
        &HtmlDocumentConfig::default(),
        Path::new("index.html"),
        "",
        "// bundle",
        &HashMap::new(),
    )
    .expect_err("should fail when runtime fragment function name is missing");
    assert!(
        error.msg.contains("runtime fragment function"),
        "error message must mention the missing runtime fragment function"
    );
}
