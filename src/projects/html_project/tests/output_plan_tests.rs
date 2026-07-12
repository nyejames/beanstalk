//! Tests for canonical HTML output planning.

use super::*;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::path::{Path, PathBuf};

#[test]
fn wasm_root_page_colocates_artifacts_at_root() {
    let plan =
        plan_wasm_output_from_logical_html_path(Path::new("index.html")).expect("should plan");
    assert_eq!(plan.logical_html_path, PathBuf::from("index.html"));
    assert_eq!(plan.html_path, PathBuf::from("index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("page.wasm")));
}

#[test]
fn wasm_nested_route_colocates_artifacts_in_route_folder() {
    let plan = plan_wasm_output_from_logical_html_path(Path::new("about/index.html"))
        .expect("should plan");
    assert_eq!(plan.logical_html_path, PathBuf::from("about/index.html"));
    assert_eq!(plan.html_path, PathBuf::from("about/index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("about/page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("about/page.wasm")));
}

#[test]
fn wasm_deep_nested_route_preserves_full_directory_structure() {
    let plan = plan_wasm_output_from_logical_html_path(Path::new("docs/basics/index.html"))
        .expect("should plan");
    assert_eq!(
        plan.logical_html_path,
        PathBuf::from("docs/basics/index.html")
    );
    assert_eq!(plan.html_path, PathBuf::from("docs/basics/index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("docs/basics/page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("docs/basics/page.wasm")));
}

#[test]
fn directory_route_uses_module_directory_not_hash_root_filename() {
    let mut string_table = StringTable::new();
    let route = derive_logical_html_path(
        Path::new("/project/src/blog/#arbitrary.bst"),
        Some(Path::new("/project/src")),
        &mut string_table,
    )
    .expect("directory route should resolve");

    assert_eq!(route, PathBuf::from("blog/index.html"));
}

#[test]
fn directory_root_route_ignores_hash_root_filename() {
    let mut string_table = StringTable::new();
    let route = derive_logical_html_path(
        Path::new("/project/src/#home.bst"),
        Some(Path::new("/project/src")),
        &mut string_table,
    )
    .expect("directory route should resolve");

    assert_eq!(route, PathBuf::from("index.html"));
}
