//! Tests for HTML page metadata extraction.

use super::*;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::hir_display::HirSideTable;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirConstId, HirModule, HirModuleConst};
use crate::compiler_frontend::interned_path::InternedPath;
use std::path::PathBuf;

fn test_module(string_table: &mut StringTable) -> HirModule {
    let mut module = HirModule::new();
    let start_path =
        InternedPath::from_path_buf(PathBuf::from("docs/#page.bst").as_path(), string_table)
            .join_str("start", string_table);
    let mut side_table = HirSideTable::default();
    side_table.bind_function_name(FunctionId(0), start_path);
    module.start_function = FunctionId(0);
    module.side_table = side_table;
    module
}

fn string_constant(name: &str, value: &str) -> HirModuleConst {
    HirModuleConst {
        id: HirConstId(0),
        name: name.to_owned(),
        ty: TypeId(0),
        value: HirConstValue::String(value.to_owned()),
    }
}

#[test]
fn extracts_reserved_entry_metadata() {
    let mut string_table = StringTable::new();
    let mut module = test_module(&mut string_table);
    module.module_constants = vec![
        string_constant("docs/#page.bst/page_title", "Home"),
        string_constant(
            "docs/#page.bst/page_head",
            "<meta name=\"x\" content=\"y\">",
        ),
        string_constant("page_description", "Landing page"),
    ];

    let metadata =
        extract_html_page_metadata(&module, &string_table).expect("metadata should parse");
    assert_eq!(metadata.title, Some(String::from("Home")));
    assert_eq!(
        metadata.extra_head_html,
        Some(String::from("<meta name=\"x\" content=\"y\">"))
    );
    assert_eq!(metadata.description, Some(String::from("Landing page")));
}

#[test]
fn ignores_non_entry_constants() {
    let mut string_table = StringTable::new();
    let mut module = test_module(&mut string_table);
    module.module_constants = vec![
        string_constant("docs/#page.bst/page_title", "Home"),
        string_constant("docs/shared.bst/page_title", "Shared"),
    ];

    let metadata =
        extract_html_page_metadata(&module, &string_table).expect("metadata should parse");
    assert_eq!(metadata.title, Some(String::from("Home")));
}

#[test]
fn rejects_non_string_reserved_values() {
    let mut string_table = StringTable::new();
    let mut module = test_module(&mut string_table);
    module.module_constants = vec![HirModuleConst {
        id: HirConstId(0),
        name: String::from("page_title"),
        ty: TypeId(0),
        value: HirConstValue::Bool(true),
    }];

    let error = extract_html_page_metadata(&module, &string_table)
        .expect_err("non-string metadata should fail");
    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(error.msg.contains("page_title"));
}

#[test]
fn rejects_duplicate_reserved_values() {
    let mut string_table = StringTable::new();
    let mut module = test_module(&mut string_table);
    module.module_constants = vec![
        string_constant("page_title", "Home"),
        string_constant("docs/#page.bst/page_title", "Another"),
    ];

    let error = extract_html_page_metadata(&module, &string_table)
        .expect_err("duplicate metadata should fail");
    assert!(error.msg.contains("declared more than once"));
}
