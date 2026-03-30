//! Tests for canonical HTML output planning.

use super::*;
use crate::compiler_frontend::string_interning::StringTable;
use std::path::{Path, PathBuf};

#[test]
fn wasm_root_page_colocates_artifacts_at_root() {
    let mut string_table = StringTable::new();
    let plan = plan_wasm_output(
        Path::new("/project/src/#page.bst"),
        Some(Path::new("/project/src")),
        &mut string_table,
    )
    .expect("should plan");
    assert_eq!(plan.html_path, PathBuf::from("index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("page.wasm")));
}

#[test]
fn wasm_about_route_colocates_artifacts_in_route_folder() {
    let mut string_table = StringTable::new();
    let plan = plan_wasm_output(
        Path::new("/project/src/#about.bst"),
        Some(Path::new("/project/src")),
        &mut string_table,
    )
    .expect("should plan");
    assert_eq!(plan.html_path, PathBuf::from("about/index.html"));
    assert_eq!(plan.js_path, Some(PathBuf::from("about/page.js")));
    assert_eq!(plan.wasm_path, Some(PathBuf::from("about/page.wasm")));
}
