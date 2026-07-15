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

#[test]
fn single_file_route_uses_exact_utf8_stem() {
    let mut string_table = StringTable::new();
    let route = derive_logical_html_path(Path::new("main.bst"), None, &mut string_table)
        .expect("ordinary single-file route should resolve");

    assert_eq!(route, PathBuf::from("main.html"));
}

#[test]
fn single_file_page_route_maps_to_homepage() {
    let mut string_table = StringTable::new();
    let route = derive_logical_html_path(Path::new("#page.bst"), None, &mut string_table)
        .expect("#page single-file route should resolve");

    assert_eq!(route, PathBuf::from("index.html"));
}

#[test]
fn single_file_hash_prefix_strips_cosmetic_hash() {
    let mut string_table = StringTable::new();
    let route = derive_logical_html_path(Path::new("#about.bst"), None, &mut string_table)
        .expect("hash-prefixed single-file route should resolve");

    assert_eq!(route, PathBuf::from("about.html"));
}

#[test]
fn single_file_missing_stem_is_rejected_not_main() {
    let mut string_table = StringTable::new();
    let error = derive_logical_html_path(Path::new("."), None, &mut string_table)
        .expect_err("missing single-file stem should be rejected, never fall back to main");

    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::File,
        "missing stem should surface as a File infrastructure error"
    );
}

#[cfg(unix)]
#[test]
fn single_file_non_utf8_stem_is_rejected() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let mut string_table = StringTable::new();
    let bad_stem = OsString::from_vec(vec![0xC3, 0x28]);
    let entry = Path::new(&bad_stem).with_extension("bst");

    let error = derive_logical_html_path(&entry, None, &mut string_table)
        .expect_err("non-UTF-8 single-file stem should be rejected");

    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::File,
        "non-UTF-8 stem should surface as a File infrastructure error"
    );
}

#[test]
fn single_file_hash_only_stem_is_rejected_not_empty_route() {
    let mut string_table = StringTable::new();
    let error = derive_logical_html_path(Path::new("#.bst"), None, &mut string_table)
        .expect_err("hash-only stem should be rejected, not produce an empty route name");

    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::File,
        "hash-only stem should surface as a File infrastructure error"
    );
}
